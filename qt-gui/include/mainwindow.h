#pragma once

#include <QMainWindow>
#include <QLabel>
#include <QPushButton>
#include <QTabWidget>
#include <QTextEdit>
#include <QTimer>
#include <memory>
#include "processmanager.h"
#include "configmanager.h"
#include "traymanager.h"

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(QWidget *parent = nullptr);
    ~MainWindow();

protected:
    void closeEvent(QCloseEvent *event) override;

private slots:
    void onStartStopClicked();
    void onSettingsClicked();
    void onStatusChanged(ProcessManager::ProxyStatus status);
    void onLogReceived(const QString &level, const QString &message, const QString &timestamp);
    void onStatisticsUpdated(const ProcessManager::Statistics &stats);
    void onErrorOccurred(const QString &error);
    void onTrayActivated();
    void onTrayActionTriggered(const QString &action);
    void updateDashboard();

private:
    void setupUi();
    void createDashboard();
    void createSettingsPanel();
    void createLogsPanel();
    void connectSignals();
    QString formatBytes(quint64 bytes) const;
    QString formatUptime(quint64 seconds) const;
    QString statusToString(ProcessManager::ProxyStatus status) const;
    QColor statusColor(ProcessManager::ProxyStatus status) const;

    std::unique_ptr<ProcessManager> m_processManager;
    std::unique_ptr<ConfigManager> m_configManager;
    std::unique_ptr<TrayManager> m_trayManager;

    QTabWidget *m_tabWidget;
    QPushButton *m_startStopButton;
    QLabel *m_statusLabel;
    QLabel *m_statusIndicator;
    QLabel *m_uptimeLabel;
    QLabel *m_uploadLabel;
    QLabel *m_downloadLabel;
    QLabel *m_connectionsLabel;
    QLabel *m_totalConnectionsLabel;
    QTextEdit *m_logsTextEdit;
    QTimer m_updateTimer;
};
