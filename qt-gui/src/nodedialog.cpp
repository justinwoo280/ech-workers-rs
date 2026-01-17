#include "nodedialog.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QFormLayout>
#include <QPushButton>
#include <QLabel>
#include <QGroupBox>
#include <QMessageBox>

NodeDialog::NodeDialog(QWidget *parent)
    : QDialog(parent)
{
    setupUi();
    setWindowTitle("添加节点");
}

NodeDialog::NodeDialog(const ProxyNode &node, QWidget *parent)
    : QDialog(parent)
    , m_node(node)
    , m_editMode(true)
{
    setupUi();
    loadNode(node);
    setWindowTitle("编辑节点");
}

void NodeDialog::setupUi() {
    setMinimumWidth(500);
    
    QVBoxLayout *mainLayout = new QVBoxLayout(this);
    
    QGroupBox *basicGroup = new QGroupBox("基本信息");
    QFormLayout *basicLayout = new QFormLayout(basicGroup);
    
    m_nameEdit = new QLineEdit();
    m_nameEdit->setPlaceholderText("例如: HK Node 1");
    basicLayout->addRow("节点名称:", m_nameEdit);
    
    m_serverAddrEdit = new QLineEdit();
    m_serverAddrEdit->setPlaceholderText("example.com:443");
    basicLayout->addRow("服务器地址:", m_serverAddrEdit);
    
    m_tokenEdit = new QLineEdit();
    m_tokenEdit->setEchoMode(QLineEdit::Password);
    m_tokenEdit->setPlaceholderText("认证密钥");
    basicLayout->addRow("Token:", m_tokenEdit);
    
    mainLayout->addWidget(basicGroup);
    
    QGroupBox *echGroup = new QGroupBox("ECH 设置");
    QFormLayout *echLayout = new QFormLayout(echGroup);
    
    m_useEchCheck = new QCheckBox("启用 ECH");
    m_useEchCheck->setChecked(true);
    echLayout->addRow(m_useEchCheck);
    
    m_echDomainEdit = new QLineEdit("cloudflare-ech.com");
    echLayout->addRow("ECH 域名:", m_echDomainEdit);
    
    m_dohServerEdit = new QLineEdit("223.5.5.5/dns-query");
    m_dohServerEdit->setPlaceholderText("无需 https:// 前缀");
    echLayout->addRow("DoH 服务器:", m_dohServerEdit);
    
    mainLayout->addWidget(echGroup);
    
    QGroupBox *advancedGroup = new QGroupBox("高级设置");
    QFormLayout *advancedLayout = new QFormLayout(advancedGroup);
    
    m_useYamuxCheck = new QCheckBox("启用 Yamux 多路复用");
    m_useYamuxCheck->setChecked(true);
    advancedLayout->addRow(m_useYamuxCheck);
    
    m_tlsProfileCombo = new QComboBox();
    m_tlsProfileCombo->addItem("Chrome 120+", "Chrome");
    m_tlsProfileCombo->addItem("BoringSSL 默认", "BoringSSLDefault");
    advancedLayout->addRow("TLS 指纹:", m_tlsProfileCombo);
    
    mainLayout->addWidget(advancedGroup);
    
    QHBoxLayout *buttonsLayout = new QHBoxLayout();
    buttonsLayout->addStretch();
    
    QPushButton *testButton = new QPushButton("测试连接");
    connect(testButton, &QPushButton::clicked, this, &NodeDialog::onTestConnectionClicked);
    buttonsLayout->addWidget(testButton);
    
    QPushButton *saveButton = new QPushButton("保存");
    connect(saveButton, &QPushButton::clicked, this, &NodeDialog::onSaveClicked);
    buttonsLayout->addWidget(saveButton);
    
    QPushButton *cancelButton = new QPushButton("取消");
    connect(cancelButton, &QPushButton::clicked, this, &NodeDialog::onCancelClicked);
    buttonsLayout->addWidget(cancelButton);
    
    mainLayout->addLayout(buttonsLayout);
}

void NodeDialog::loadNode(const ProxyNode &node) {
    m_nameEdit->setText(node.name);
    m_serverAddrEdit->setText(node.serverAddr);
    m_tokenEdit->setText(node.token);
    m_useEchCheck->setChecked(node.useEch);
    m_echDomainEdit->setText(node.echDomain);
    m_dohServerEdit->setText(node.dohServer);
    m_useYamuxCheck->setChecked(node.useYamux);
    
    int index = m_tlsProfileCombo->findData(node.tlsProfile);
    if (index >= 0) {
        m_tlsProfileCombo->setCurrentIndex(index);
    }
}

ProxyNode NodeDialog::getNode() const {
    ProxyNode node = m_node;
    node.name = m_nameEdit->text();
    node.serverAddr = m_serverAddrEdit->text();
    node.token = m_tokenEdit->text();
    node.useEch = m_useEchCheck->isChecked();
    node.echDomain = m_echDomainEdit->text();
    node.dohServer = m_dohServerEdit->text();
    node.useYamux = m_useYamuxCheck->isChecked();
    node.tlsProfile = m_tlsProfileCombo->currentData().toString();
    return node;
}

void NodeDialog::onSaveClicked() {
    if (m_nameEdit->text().trimmed().isEmpty()) {
        QMessageBox::warning(this, "输入错误", "节点名称不能为空");
        return;
    }
    
    if (m_serverAddrEdit->text().trimmed().isEmpty()) {
        QMessageBox::warning(this, "输入错误", "服务器地址不能为空");
        return;
    }
    
    if (m_tokenEdit->text().trimmed().isEmpty()) {
        QMessageBox::warning(this, "输入错误", "Token 不能为空");
        return;
    }
    
    accept();
}

void NodeDialog::onCancelClicked() {
    reject();
}

void NodeDialog::onTestConnectionClicked() {
    // TODO: 实现连接测试
    QMessageBox::information(this, "测试连接", "连接测试功能待实现");
}
