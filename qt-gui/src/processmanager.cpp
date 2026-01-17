#include "processmanager.h"
#include <QJsonDocument>
#include <QJsonObject>
#include <QCoreApplication>
#include <QDir>
#include <QDebug>

ProcessManager::ProcessManager(QObject *parent)
    : QObject(parent)
    , m_process(std::make_unique<QProcess>(this))
{
    m_backendPath = QCoreApplication::applicationDirPath() + "/ech-workers-rs.exe";

    connect(m_process.get(), &QProcess::started, this, &ProcessManager::onProcessStarted);
    connect(m_process.get(), QOverload<int, QProcess::ExitStatus>::of(&QProcess::finished),
            this, &ProcessManager::onProcessFinished);
    connect(m_process.get(), &QProcess::errorOccurred, this, &ProcessManager::onProcessErrorOccurred);
    connect(m_process.get(), &QProcess::readyReadStandardOutput, this, &ProcessManager::onReadyReadStandardOutput);
    connect(m_process.get(), &QProcess::readyReadStandardError, this, &ProcessManager::onReadyReadStandardError);

    m_heartbeatTimer.setInterval(5000);
    connect(&m_heartbeatTimer, &QTimer::timeout, this, &ProcessManager::onHeartbeatTimeout);
}

ProcessManager::~ProcessManager() {
    stop();
}

bool ProcessManager::start(const QJsonObject &config) {
    if (m_status == ProxyStatus::Running || m_status == ProxyStatus::Starting) {
        return false;
    }

    updateStatus(ProxyStatus::Starting);

    QStringList arguments;
    arguments << "--json-rpc";

    m_process->setProcessChannelMode(QProcess::SeparateChannels);
    m_process->start(m_backendPath, arguments);

    if (!m_process->waitForStarted(5000)) {
        updateStatus(ProxyStatus::Error);
        m_lastError = "Failed to start backend process";
        emit errorOccurred(m_lastError);
        return false;
    }

    sendCommand("start", config);
    m_heartbeatTimer.start();

    return true;
}

void ProcessManager::stop() {
    if (m_status == ProxyStatus::Stopped) {
        return;
    }

    updateStatus(ProxyStatus::Stopping);
    m_heartbeatTimer.stop();

    sendCommand("stop");

    if (m_process->state() == QProcess::Running) {
        m_process->waitForFinished(3000);
        if (m_process->state() == QProcess::Running) {
            m_process->kill();
        }
    }

    updateStatus(ProxyStatus::Stopped);
    m_stats = Statistics();
}

void ProcessManager::restart() {
    stop();
    QTimer::singleShot(500, this, [this]() {
        // Restart with last config (TODO: store config)
        start(QJsonObject());
    });
}

void ProcessManager::sendCommand(const QString &method, const QJsonObject &params) {
    if (m_process->state() != QProcess::Running) {
        return;
    }

    QJsonObject request;
    request["id"] = static_cast<qint64>(++m_requestId);
    request["method"] = method;
    request["params"] = params;

    QJsonDocument doc(request);
    QByteArray data = doc.toJson(QJsonDocument::Compact) + "\n";

    m_process->write(data);
    m_process->waitForBytesWritten(1000);

    qDebug() << "Sent command:" << method << "id:" << m_requestId;
}

void ProcessManager::onProcessStarted() {
    qDebug() << "Backend process started";
}

void ProcessManager::onProcessFinished(int exitCode, QProcess::ExitStatus exitStatus) {
    qDebug() << "Backend process finished. Exit code:" << exitCode
             << "Status:" << (exitStatus == QProcess::NormalExit ? "Normal" : "Crashed");

    m_heartbeatTimer.stop();

    if (exitStatus == QProcess::CrashExit) {
        m_lastError = "Backend process crashed";
        updateStatus(ProxyStatus::Error);
    } else {
        updateStatus(ProxyStatus::Stopped);
    }
}

void ProcessManager::onProcessErrorOccurred(QProcess::ProcessError error) {
    QString errorStr;
    switch (error) {
        case QProcess::FailedToStart:
            errorStr = "Failed to start backend process. Check if ech-workers-rs.exe exists.";
            break;
        case QProcess::Crashed:
            errorStr = "Backend process crashed";
            break;
        case QProcess::Timedout:
            errorStr = "Backend process timed out";
            break;
        case QProcess::WriteError:
            errorStr = "Write error to backend process";
            break;
        case QProcess::ReadError:
            errorStr = "Read error from backend process";
            break;
        default:
            errorStr = "Unknown process error";
    }

    qDebug() << "Process error:" << errorStr;
    m_lastError = errorStr;
    updateStatus(ProxyStatus::Error);
    emit errorOccurred(errorStr);
}

void ProcessManager::onReadyReadStandardOutput() {
    int processedLines = 0;
    const int maxLinesPerBatch = 1000;
    
    while (m_process->canReadLine() && processedLines < maxLinesPerBatch) {
        QByteArray line = m_process->readLine().trimmed();
        if (line.isEmpty()) continue;
        
        // 单行大小限制: 10MB (防止恶意超长JSON)
        if (line.size() > 10 * 1024 * 1024) {
            qWarning() << "Skipped oversized JSON line:" << line.size() << "bytes";
            continue;
        }

        QJsonParseError parseError;
        QJsonDocument doc = QJsonDocument::fromJson(line, &parseError);

        if (parseError.error != QJsonParseError::NoError) {
            qWarning() << "Failed to parse JSON:" << parseError.errorString() << "Data:" << line;
            continue;
        }

        if (!doc.isObject()) {
            qWarning() << "Invalid JSON response: not an object";
            continue;
        }

        processJsonResponse(doc.object());
        ++processedLines;
    }
    
    // 如果还有剩余数据，下次事件循环继续处理
    if (m_process->canReadLine()) {
        QMetaObject::invokeMethod(this, &ProcessManager::onReadyReadStandardOutput, Qt::QueuedConnection);
    }
}

void ProcessManager::onReadyReadStandardError() {
    const qint64 maxStderrSize = 1024 * 1024;
    QByteArray data = m_process->readAllStandardError();
    
    // 限制stderr读取大小，防止后端崩溃时输出海量日志导致OOM
    if (data.size() > maxStderrSize) {
        data.truncate(maxStderrSize);
        data.append("\n[...truncated due to size limit]");
    }
    
    QString errorText = QString::fromUtf8(data).trimmed();
    if (!errorText.isEmpty()) {
        qDebug() << "Backend stderr:" << errorText;
        emit logReceived("ERROR", errorText, QDateTime::currentDateTime().toString(Qt::ISODate));
    }
}

void ProcessManager::onHeartbeatTimeout() {
    sendCommand("get_status");
}

void ProcessManager::processJsonResponse(const QJsonObject &response) {
    if (response.contains("event")) {
        QString event = response["event"].toString();
        QJsonObject data = response["data"].toObject();
        handleEvent(event, data);
    } else if (response.contains("id")) {
        quint64 id = response["id"].toVariant().toULongLong();
        
        if (response.contains("result")) {
            QJsonObject result = response["result"].toObject();
            qDebug() << "RPC result for id" << id << ":" << result;

            if (result.contains("status")) {
                QString status = result["status"].toString();
                if (status == "starting" || status == "running") {
                    updateStatus(ProxyStatus::Running);
                }
            }
        } else if (response.contains("error")) {
            QJsonObject error = response["error"].toObject();
            QString errorMsg = error["message"].toString();
            qWarning() << "RPC error for id" << id << ":" << errorMsg;
            m_lastError = errorMsg;
            emit errorOccurred(errorMsg);
        }
    }
}

void ProcessManager::handleEvent(const QString &event, const QJsonObject &data) {
    if (event == "log") {
        QString level = data["level"].toString().toUpper();
        QString message = data["message"].toString();
        QString timestamp = data["timestamp"].toString();
        emit logReceived(level, message, timestamp);
    }
    else if (event == "status") {
        QString statusStr = data["status"].toString();
        ProxyStatus newStatus = ProxyStatus::Stopped;

        if (statusStr == "stopped") newStatus = ProxyStatus::Stopped;
        else if (statusStr == "starting") newStatus = ProxyStatus::Starting;
        else if (statusStr == "running") newStatus = ProxyStatus::Running;
        else if (statusStr == "stopping") newStatus = ProxyStatus::Stopping;
        else if (statusStr == "error") newStatus = ProxyStatus::Error;

        updateStatus(newStatus);

        if (data.contains("uptime_secs")) {
            m_stats.uptimeSeconds = data["uptime_secs"].toVariant().toULongLong();
        }
    }
    else if (event == "stats") {
        m_stats.uploadBytes = data["upload_bytes"].toVariant().toULongLong();
        m_stats.downloadBytes = data["download_bytes"].toVariant().toULongLong();
        m_stats.activeConnections = data["active_connections"].toInt();
        m_stats.totalConnections = data["total_connections"].toVariant().toULongLong();
        emit statisticsUpdated(m_stats);
    }
}

void ProcessManager::updateStatus(ProxyStatus newStatus) {
    if (m_status != newStatus) {
        m_status = newStatus;
        emit statusChanged(newStatus);
    }
}
