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
    setMinimumWidth(600);

    QVBoxLayout *mainLayout = new QVBoxLayout(this);

    QTabWidget *tabs = new QTabWidget();

    QWidget *basicTab = new QWidget();
    QFormLayout *basicLayout = new QFormLayout(basicTab);
    m_listenAddrEdit = new QLineEdit();
    m_serverAddrEdit = new QLineEdit();
    m_tokenEdit = new QLineEdit();
    m_tokenEdit->setEchoMode(QLineEdit::Password);
    m_enableTunCheck = new QCheckBox("启用 TUN 全局模式 (需要管理员权限)");

    basicLayout->addRow("监听地址:", m_listenAddrEdit);
    basicLayout->addRow("服务器地址:", m_serverAddrEdit);
    basicLayout->addRow("认证 Token:", m_tokenEdit);
    basicLayout->addRow(m_enableTunCheck);

    tabs->addTab(basicTab, "📡 基本设置");

    QWidget *echTab = new QWidget();
    QFormLayout *echLayout = new QFormLayout(echTab);
    m_echEnabledCheck = new QCheckBox("启用 ECH (Encrypted Client Hello)");
    m_echDomainEdit = new QLineEdit();
    m_dohServerEdit = new QLineEdit();

    echLayout->addRow(m_echEnabledCheck);
    echLayout->addRow("ECH 域名:", m_echDomainEdit);
    echLayout->addRow("DoH 服务器:", m_dohServerEdit);

    tabs->addTab(echTab, "🔒 ECH 设置");

    QWidget *advancedTab = new QWidget();
    QFormLayout *advancedLayout = new QFormLayout(advancedTab);
    m_yamuxCheck = new QCheckBox("启用 Yamux 多路复用");
    m_fingerprintCheck = new QCheckBox("启用指纹随机化");
    m_tlsProfileCombo = new QComboBox();
    m_tlsProfileCombo->addItem("Chrome 120+", "Chrome");
    m_tlsProfileCombo->addItem("BoringSSL 默认", "BoringSSLDefault");

    advancedLayout->addRow(m_yamuxCheck);
    advancedLayout->addRow(m_fingerprintCheck);
    advancedLayout->addRow("TLS 指纹:", m_tlsProfileCombo);

    tabs->addTab(advancedTab, "🔧 高级设置");

    QWidget *appTab = new QWidget();
    QVBoxLayout *appLayout = new QVBoxLayout(appTab);
    m_autoStartCheck = new QCheckBox("开机自启");
    m_startMinimizedCheck = new QCheckBox("启动时最小化");
    m_minimizeToTrayCheck = new QCheckBox("最小化到系统托盘");
    m_closeToTrayCheck = new QCheckBox("关闭时最小化到托盘");

    appLayout->addWidget(m_autoStartCheck);
    appLayout->addWidget(m_startMinimizedCheck);
    appLayout->addWidget(m_minimizeToTrayCheck);
    appLayout->addWidget(m_closeToTrayCheck);
    appLayout->addStretch();

    tabs->addTab(appTab, "🖥 应用设置");

    mainLayout->addWidget(tabs);

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
    m_serverAddrEdit->setText(basic["server_addr"].toString());
    m_tokenEdit->setText(basic["token"].toString());
    m_enableTunCheck->setChecked(basic["enable_tun"].toBool());

    QJsonObject ech = m_config["ech"].toObject();
    m_echEnabledCheck->setChecked(ech["enabled"].toBool());
    m_echDomainEdit->setText(ech["domain"].toString());
    m_dohServerEdit->setText(ech["doh_server"].toString());

    QJsonObject advanced = m_config["advanced"].toObject();
    m_yamuxCheck->setChecked(advanced["enable_yamux"].toBool());
    m_fingerprintCheck->setChecked(advanced["enable_fingerprint_randomization"].toBool());
    
    QString tlsProfile = advanced["tls_profile"].toString();
    int index = m_tlsProfileCombo->findData(tlsProfile);
    if (index >= 0) m_tlsProfileCombo->setCurrentIndex(index);

    QJsonObject app = m_config["app"].toObject();
    m_autoStartCheck->setChecked(app["auto_start"].toBool());
    m_startMinimizedCheck->setChecked(app["start_minimized"].toBool());
    m_minimizeToTrayCheck->setChecked(app["minimize_to_tray"].toBool());
    m_closeToTrayCheck->setChecked(app["close_to_tray"].toBool());
}

void SettingsDialog::saveSettings() {
    QJsonObject basic;
    basic["listen_addr"] = m_listenAddrEdit->text();
    basic["server_addr"] = m_serverAddrEdit->text();
    basic["token"] = m_tokenEdit->text();
    basic["enable_tun"] = m_enableTunCheck->isChecked();
    m_config["basic"] = basic;

    QJsonObject ech;
    ech["enabled"] = m_echEnabledCheck->isChecked();
    ech["domain"] = m_echDomainEdit->text();
    ech["doh_server"] = m_dohServerEdit->text();
    m_config["ech"] = ech;

    QJsonObject advanced;
    advanced["enable_yamux"] = m_yamuxCheck->isChecked();
    advanced["enable_fingerprint_randomization"] = m_fingerprintCheck->isChecked();
    advanced["tls_profile"] = m_tlsProfileCombo->currentData().toString();
    m_config["advanced"] = advanced;

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
