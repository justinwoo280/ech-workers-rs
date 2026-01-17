#pragma once

#include <QObject>
#include <QProcess>
#include <QJsonObject>
#include <QJsonArray>
#include <QString>
#include <QTimer>
#include <memory>

class ProcessManager : public QObject {
    Q_OBJECT

public:
    explicit ProcessManager(QObject *parent = nullptr);
    ~ProcessManager();

    enum class ProxyStatus {
        Stopped,
        Starting,
        Running,
        Stopping,
        Error
    };

    struct Statistics {
        quint64 uploadBytes = 0;
        quint64 downloadBytes = 0;
        quint32 activeConnections = 0;
        quint64 totalConnections = 0;
        quint64 uptimeSeconds = 0;
    };

    bool start(const QJsonObject &config);
    void stop();
    void restart();

    ProxyStatus status() const { return m_status; }
    const Statistics& statistics() const { return m_stats; }
    QString lastError() const { return m_lastError; }

signals:
    void statusChanged(ProxyStatus status);
    void logReceived(const QString &level, const QString &message, const QString &timestamp);
    void statisticsUpdated(const Statistics &stats);
    void errorOccurred(const QString &error);

private slots:
    void onProcessStarted();
    void onProcessFinished(int exitCode, QProcess::ExitStatus exitStatus);
    void onProcessErrorOccurred(QProcess::ProcessError error);
    void onReadyReadStandardOutput();
    void onReadyReadStandardError();
    void onHeartbeatTimeout();

private:
    void sendCommand(const QString &method, const QJsonObject &params = QJsonObject());
    void processJsonResponse(const QJsonObject &response);
    void handleEvent(const QString &event, const QJsonObject &data);
    void updateStatus(ProxyStatus newStatus);

    std::unique_ptr<QProcess> m_process;
    ProxyStatus m_status = ProxyStatus::Stopped;
    Statistics m_stats;
    QString m_lastError;
    quint64 m_requestId = 0;
    QTimer m_heartbeatTimer;
    QString m_backendPath;
};
