#include "settingsdialog.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFormLayout>
#include <QGroupBox>
#include <QPushButton>
#include <QLabel>

SettingsDialog::SettingsDialog(ConfigManager *configManager, QWidget *parent)
    : QDialog(parent)
    , m_configManager(configManager)
{
    setupUi();
    loadSettings();
}

void SettingsDialog::setupUi() {
    setWindowTitle("设置");
    setMinimumWidth(500);

    QVBoxLayout *mainLayout = new QVBoxLayout(this);

    // 代理设置
    QGroupBox *proxyGroup = new QGroupBox("📡 代理设置");
    QFormLayout *proxyLayout = new QFormLayout(proxyGroup);
    
    m_listenAddrEdit = new QLineEdit();
    m_listenAddrEdit->setPlaceholderText("127.0.0.1:1080");
    proxyLayout->addRow("监听地址:", m_listenAddrEdit);
    
    m_enableTunCheck = new QCheckBox("启用 TUN 全局模式 (需要管理员权限)");
    proxyLayout->addRow(m_enableTunCheck);
    
    mainLayout->addWidget(proxyGroup);

    // 应用设置
    QGroupBox *appGroup = new QGroupBox("🖥 应用设置");
    QVBoxLayout *appLayout = new QVBoxLayout(appGroup);
    
    m_autoStartCheck = new QCheckBox("开机自启");
    m_startMinimizedCheck = new QCheckBox("启动时最小化");
    m_minimizeToTrayCheck = new QCheckBox("最小化到系统托盘");
    m_closeToTrayCheck = new QCheckBox("关闭时最小化到托盘");

    appLayout->addWidget(m_autoStartCheck);
    appLayout->addWidget(m_startMinimizedCheck);
    appLayout->addWidget(m_minimizeToTrayCheck);
    appLayout->addWidget(m_closeToTrayCheck);
    
    mainLayout->addWidget(appGroup);
    
    // 提示信息
    QLabel *hintLabel = new QLabel("💡 服务器、ECH、Yamux 等连接配置请在节点面板中设置");
    hintLabel->setStyleSheet("QLabel { color: #888; font-style: italic; padding: 10px; }");
    mainLayout->addWidget(hintLabel);
    
    mainLayout->addStretch();

    QHBoxLayout *buttonsLayout = new QHBoxLayout();
    buttonsLayout->addStretch();

    QPushButton *saveButton = new QPushButton("💾 保存");
    connect(saveButton, &QPushButton::clicked, this, &SettingsDialog::onSaveClicked);
    buttonsLayout->addWidget(saveButton);

    QPushButton *cancelButton = new QPushButton("取消");
    connect(cancelButton, &QPushButton::clicked, this, &SettingsDialog::onCancelClicked);
    buttonsLayout->addWidget(cancelButton);

    mainLayout->addLayout(buttonsLayout);
}

void SettingsDialog::loadSettings() {
    m_config = m_configManager->loadConfig();

    QJsonObject basic = m_config["basic"].toObject();
    m_listenAddrEdit->setText(basic["listen_addr"].toString());
    m_enableTunCheck->setChecked(basic["enable_tun"].toBool());

    QJsonObject app = m_config["app"].toObject();
    m_autoStartCheck->setChecked(app["auto_start"].toBool());
    m_startMinimizedCheck->setChecked(app["start_minimized"].toBool());
    m_minimizeToTrayCheck->setChecked(app["minimize_to_tray"].toBool());
    m_closeToTrayCheck->setChecked(app["close_to_tray"].toBool());
}

void SettingsDialog::saveSettings() {
    QJsonObject basic = m_config["basic"].toObject();
    basic["listen_addr"] = m_listenAddrEdit->text();
    basic["enable_tun"] = m_enableTunCheck->isChecked();
    m_config["basic"] = basic;

    QJsonObject app;
    app["auto_start"] = m_autoStartCheck->isChecked();
    app["start_minimized"] = m_startMinimizedCheck->isChecked();
    app["minimize_to_tray"] = m_minimizeToTrayCheck->isChecked();
    app["close_to_tray"] = m_closeToTrayCheck->isChecked();
    m_config["app"] = app;

    m_configManager->saveConfig(m_config);
}

void SettingsDialog::onSaveClicked() {
    saveSettings();
    accept();
}

void SettingsDialog::onCancelClicked() {
    reject();
}
