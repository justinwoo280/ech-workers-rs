#include "traymanager.h"
#include <QAction>
#include <QApplication>
#include <QIcon>

TrayManager::TrayManager(QObject *parent)
    : QObject(parent)
    , m_trayIcon(new QSystemTrayIcon(this))
    , m_menu(new QMenu())
{
    setupMenu();

    m_trayIcon->setContextMenu(m_menu);
    m_trayIcon->setToolTip("ECH Workers RS - 已停止");

    // TODO: Set proper icon
    // m_trayIcon->setIcon(QIcon(":/icons/tray_stopped.png"));

    connect(m_trayIcon, &QSystemTrayIcon::activated, this, &TrayManager::onTrayActivated);

    m_trayIcon->show();
}

TrayManager::~TrayManager() {
    m_trayIcon->hide();
    delete m_menu;
}

void TrayManager::setupMenu() {
    QAction *showAction = new QAction("显示窗口", this);
    connect(showAction, &QAction::triggered, this, &TrayManager::onShowTriggered);
    m_menu->addAction(showAction);

    m_menu->addSeparator();

    QAction *quitAction = new QAction("退出", this);
    connect(quitAction, &QAction::triggered, this, &TrayManager::onQuitTriggered);
    m_menu->addAction(quitAction);
}

void TrayManager::show() {
    m_trayIcon->show();
}

void TrayManager::hide() {
    m_trayIcon->hide();
}

void TrayManager::updateStatus(bool running) {
    m_isRunning = running;

    if (running) {
        m_trayIcon->setToolTip("ECH Workers RS - 运行中");
        // TODO: Change icon
        // m_trayIcon->setIcon(QIcon(":/icons/tray_running.png"));
    } else {
        m_trayIcon->setToolTip("ECH Workers RS - 已停止");
        // TODO: Change icon
        // m_trayIcon->setIcon(QIcon(":/icons/tray_stopped.png"));
    }
}

void TrayManager::onTrayActivated(QSystemTrayIcon::ActivationReason reason) {
    if (reason == QSystemTrayIcon::Trigger) {
        emit activated();
    }
}

void TrayManager::onShowTriggered() {
    emit actionTriggered("show");
}

void TrayManager::onQuitTriggered() {
    emit actionTriggered("quit");
}
