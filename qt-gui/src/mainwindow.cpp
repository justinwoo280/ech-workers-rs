#include "mainwindow.h"
#include "settingsdialog.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGroupBox>
#include <QGridLayout>
#include <QMessageBox>
#include <QCloseEvent>
#include <QDateTime>

MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
    , m_processManager(std::make_unique<ProcessManager>(this))
    , m_configManager(std::make_unique<ConfigManager>())
    , m_trayManager(std::make_unique<TrayManager>(this))
{
    setupUi();
    connectSignals();

    m_updateTimer.setInterval(1000);
    connect(&m_updateTimer, &QTimer::timeout, this, &MainWindow::updateDashboard);
    m_updateTimer.start();

    setWindowTitle("ECH Workers RS");
    resize(1024, 768);
}

MainWindow::~MainWindow() {
    // 析构时强制清理系统代理（兜底保护）
    if (m_systemProxy) {
        m_systemProxy->disableSystemProxy();
    }
    m_processManager->stop();
}

void MainWindow::setupUi() {
    QWidget *centralWidget = new QWidget(this);
    setCentralWidget(centralWidget);

    QVBoxLayout *mainLayout = new QVBoxLayout(centralWidget);

    QHBoxLayout *topBar = new QHBoxLayout();
    QLabel *titleLabel = new QLabel("<h2>🚀 ECH Workers RS</h2>");
    topBar->addWidget(titleLabel);
    topBar->addStretch();

    m_startStopButton = new QPushButton("▶ 启动");
    m_startStopButton->setMinimumWidth(100);
    topBar->addWidget(m_startStopButton);

    QPushButton *settingsButton = new QPushButton("⚙ 设置");
    topBar->addWidget(settingsButton);
    connect(settingsButton, &QPushButton::clicked, this, &MainWindow::onSettingsClicked);

    mainLayout->addLayout(topBar);

    m_tabWidget = new QTabWidget();
    createDashboard();
    createLogsPanel();

    mainLayout->addWidget(m_tabWidget);

    QHBoxLayout *statusBar = new QHBoxLayout();
    m_statusLabel = new QLabel("状态: 已停止");
    statusBar->addWidget(m_statusLabel);
    mainLayout->addLayout(statusBar);
}

void MainWindow::createDashboard() {
    QWidget *dashboard = new QWidget();
    QVBoxLayout *layout = new QVBoxLayout(dashboard);

    QGroupBox *statusGroup = new QGroupBox("📊 连接状态");
    QVBoxLayout *statusLayout = new QVBoxLayout(statusGroup);

    QHBoxLayout *statusRow = new QHBoxLayout();
    m_statusIndicator = new QLabel("●");
    m_statusIndicator->setStyleSheet("QLabel { color: gray; font-size: 24px; }");
    statusRow->addWidget(m_statusIndicator);

    QLabel *statusTextLabel = new QLabel("代理状态:");
    statusRow->addWidget(statusTextLabel);

    QLabel *statusValueLabel = new QLabel("已停止");
    statusValueLabel->setObjectName("statusValue");
    statusRow->addWidget(statusValueLabel);
    statusRow->addStretch();

    statusLayout->addLayout(statusRow);

    m_uptimeLabel = new QLabel("⏱ 运行时间: 00:00");
    m_uptimeLabel->setStyleSheet("QLabel { color: #90EE90; font-weight: bold; }");
    statusLayout->addWidget(m_uptimeLabel);

    layout->addWidget(statusGroup);

    QGroupBox *statsGroup = new QGroupBox("📈 流量统计");
    QGridLayout *statsLayout = new QGridLayout(statsGroup);

    statsLayout->addWidget(new QLabel("⬆ 上传:"), 0, 0);
    m_uploadLabel = new QLabel("0 B");
    m_uploadLabel->setStyleSheet("QLabel { color: #87CEEB; font-weight: bold; }");
    statsLayout->addWidget(m_uploadLabel, 0, 1);

    statsLayout->addWidget(new QLabel("⬇ 下载:"), 1, 0);
    m_downloadLabel = new QLabel("0 B");
    m_downloadLabel->setStyleSheet("QLabel { color: #90EE90; font-weight: bold; }");
    statsLayout->addWidget(m_downloadLabel, 1, 1);

    statsLayout->addWidget(new QLabel("🔗 活跃连接:"), 2, 0);
    m_connectionsLabel = new QLabel("0");
    m_connectionsLabel->setStyleSheet("QLabel { color: #FFFF00; font-weight: bold; }");
    statsLayout->addWidget(m_connectionsLabel, 2, 1);

    statsLayout->addWidget(new QLabel("📊 总连接数:"), 3, 0);
    m_totalConnectionsLabel = new QLabel("0");
    m_totalConnectionsLabel->setStyleSheet("QLabel { font-weight: bold; }");
    statsLayout->addWidget(m_totalConnectionsLabel, 3, 1);

    layout->addWidget(statsGroup);
    layout->addStretch();

    m_tabWidget->addTab(dashboard, "📊 状态");
}

void MainWindow::createLogsPanel() {
    QWidget *logsPanel = new QWidget();
    QVBoxLayout *layout = new QVBoxLayout(logsPanel);

    m_logsTextEdit = new QTextEdit();
    m_logsTextEdit->setReadOnly(true);
    m_logsTextEdit->setStyleSheet("QTextEdit { background-color: #1E1E1E; color: #FFFFFF; font-family: Consolas, monospace; }");
    
    // CRITICAL: 限制日志最大行数，防止OOM (循环缓冲区)
    m_logsTextEdit->document()->setMaximumBlockCount(5000);
    
    layout->addWidget(m_logsTextEdit);

    QHBoxLayout *buttonsLayout = new QHBoxLayout();
    QPushButton *clearButton = new QPushButton("清空日志");
    connect(clearButton, &QPushButton::clicked, m_logsTextEdit, &QTextEdit::clear);
    buttonsLayout->addStretch();
    buttonsLayout->addWidget(clearButton);
    layout->addLayout(buttonsLayout);

    m_tabWidget->addTab(logsPanel, "📝 日志");
}

void MainWindow::connectSignals() {
    connect(m_startStopButton, &QPushButton::clicked, this, &MainWindow::onStartStopClicked);
    connect(m_processManager.get(), &ProcessManager::statusChanged, this, &MainWindow::onStatusChanged);
    connect(m_processManager.get(), &ProcessManager::logReceived, this, &MainWindow::onLogReceived);
    connect(m_processManager.get(), &ProcessManager::statisticsUpdated, this, &MainWindow::onStatisticsUpdated);
    connect(m_processManager.get(), &ProcessManager::errorOccurred, this, &MainWindow::onErrorOccurred);
    connect(m_trayManager.get(), &TrayManager::activated, this, &MainWindow::onTrayActivated);
    connect(m_trayManager.get(), &TrayManager::actionTriggered, this, &MainWindow::onTrayActionTriggered);
}

void MainWindow::onStartStopClicked() {
    if (m_processManager->status() == ProcessManager::ProxyStatus::Running) {
        // 停止代理时清理系统代理
        if (m_systemProxy) {
            m_systemProxy->disableSystemProxy();
        }
        m_processManager->stop();
    } else {
        QJsonObject config = m_configManager->loadConfig();
        if (!m_processManager->start(config)) {
            QMessageBox::critical(this, "错误", "启动失败: " + m_processManager->lastError());
        }
    }
}

void MainWindow::onSettingsClicked() {
    SettingsDialog dialog(m_configManager.get(), this);
    if (dialog.exec() == QDialog::Accepted) {
        onLogReceived("INFO", "配置已保存", QDateTime::currentDateTime().toString(Qt::ISODate));
    }
}

void MainWindow::onStatusChanged(ProcessManager::ProxyStatus status) {
    QString statusText = statusToString(status);
    QColor color = statusColor(status);

    m_statusLabel->setText("状态: " + statusText);
    m_statusIndicator->setStyleSheet(QString("QLabel { color: %1; font-size: 24px; }").arg(color.name()));
    findChild<QLabel*>("statusValue")->setText(statusText);

    if (status == ProcessManager::ProxyStatus::Running) {
        m_startStopButton->setText("⏹ 停止");
    } else {
        m_startStopButton->setText("▶ 启动");
    }

    m_trayManager->updateStatus(status == ProcessManager::ProxyStatus::Running);
}

void MainWindow::onLogReceived(const QString &level, const QString &message, const QString &timestamp) {
    QString color;
    if (level == "ERROR") color = "#FF6B6B";
    else if (level == "WARN") color = "#FFD93D";
    else if (level == "INFO") color = "#FFFFFF";
    else if (level == "DEBUG") color = "#87CEEB";
    else color = "#808080";

    QString html = QString("<span style='color: %1;'>[%2] [%3] %4</span>")
        .arg(color, timestamp, level, message.toHtmlEscaped());

    m_logsTextEdit->append(html);
}

void MainWindow::onStatisticsUpdated(const ProcessManager::Statistics &stats) {
    m_uploadLabel->setText(formatBytes(stats.uploadBytes));
    m_downloadLabel->setText(formatBytes(stats.downloadBytes));
    m_connectionsLabel->setText(QString::number(stats.activeConnections));
    m_totalConnectionsLabel->setText(QString::number(stats.totalConnections));
}

void MainWindow::onErrorOccurred(const QString &error) {
    QMessageBox::critical(this, "错误", error);
}

void MainWindow::onTrayActivated() {
    if (isVisible()) {
        hide();
    } else {
        show();
        activateWindow();
        raise();
    }
}

void MainWindow::onTrayActionTriggered(const QString &action) {
    if (action == "show") {
        show();
        activateWindow();
        raise();
    } else if (action == "quit") {
        // CRITICAL: 托盘退出时强制清理系统代理
        if (m_systemProxy) {
            m_systemProxy->disableSystemProxy();
        }
        m_processManager->stop();
        QApplication::quit();
    }
}

void MainWindow::updateDashboard() {
    if (m_processManager->status() == ProcessManager::ProxyStatus::Running) {
        m_uptimeLabel->setText("⏱ 运行时间: " + formatUptime(m_processManager->statistics().uptimeSeconds));
    }
}

void MainWindow::closeEvent(QCloseEvent *event) {
    if (m_configManager->loadConfig()["app"].toObject()["close_to_tray"].toBool(true)) {
        hide();
        event->ignore();
    } else {
        // CRITICAL: 强制清理系统代理，防止用户网络断开
        if (m_systemProxy) {
            m_systemProxy->disableSystemProxy();
        }
        
        m_processManager->stop();
        event->accept();
    }
}

QString MainWindow::formatBytes(quint64 bytes) const {
    const quint64 KB = 1024;
    const quint64 MB = KB * 1024;
    const quint64 GB = MB * 1024;

    if (bytes >= GB) return QString("%1 GB").arg(bytes / double(GB), 0, 'f', 2);
    if (bytes >= MB) return QString("%1 MB").arg(bytes / double(MB), 0, 'f', 2);
    if (bytes >= KB) return QString("%1 KB").arg(bytes / double(KB), 0, 'f', 2);
    return QString("%1 B").arg(bytes);
}

QString MainWindow::formatUptime(quint64 seconds) const {
    quint64 hours = seconds / 3600;
    quint64 minutes = (seconds % 3600) / 60;
    quint64 secs = seconds % 60;

    if (hours > 0) return QString("%1:%2:%3").arg(hours, 2, 10, QChar('0'))
                                             .arg(minutes, 2, 10, QChar('0'))
                                             .arg(secs, 2, 10, QChar('0'));
    return QString("%1:%2").arg(minutes, 2, 10, QChar('0')).arg(secs, 2, 10, QChar('0'));
}

QString MainWindow::statusToString(ProcessManager::ProxyStatus status) const {
    switch (status) {
        case ProcessManager::ProxyStatus::Stopped: return "已停止";
        case ProcessManager::ProxyStatus::Starting: return "启动中...";
        case ProcessManager::ProxyStatus::Running: return "运行中";
        case ProcessManager::ProxyStatus::Stopping: return "停止中...";
        case ProcessManager::ProxyStatus::Error: return "错误";
        default: return "未知";
    }
}

QColor MainWindow::statusColor(ProcessManager::ProxyStatus status) const {
    switch (status) {
        case ProcessManager::ProxyStatus::Stopped: return QColor("#808080");
        case ProcessManager::ProxyStatus::Starting: return QColor("#FFD93D");
        case ProcessManager::ProxyStatus::Running: return QColor("#00FF00");
        case ProcessManager::ProxyStatus::Stopping: return QColor("#FFD93D");
        case ProcessManager::ProxyStatus::Error: return QColor("#FF0000");
        default: return QColor("#808080");
    }
}
