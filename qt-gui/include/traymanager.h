#pragma once

#include <QObject>
#include <QSystemTrayIcon>
#include <QMenu>

class TrayManager : public QObject {
    Q_OBJECT

public:
    explicit TrayManager(QObject *parent = nullptr);
    ~TrayManager();

    void show();
    void hide();
    void updateStatus(bool running);

signals:
    void activated();
    void actionTriggered(const QString &action);

private slots:
    void onTrayActivated(QSystemTrayIcon::ActivationReason reason);
    void onShowTriggered();
    void onQuitTriggered();

private:
    void setupMenu();

    QSystemTrayIcon *m_trayIcon;
    QMenu *m_menu;
    bool m_isRunning = false;
};
