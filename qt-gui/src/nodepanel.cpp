#include "nodepanel.h"
#include "nodedialog.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QGroupBox>
#include <QMessageBox>

NodePanel::NodePanel(NodeManager *nodeManager, SystemProxy *systemProxy, QWidget *parent)
    : QWidget(parent)
    , m_nodeManager(nodeManager)
    , m_systemProxy(systemProxy)
{
    setupUi();
    refreshNodeList();
    
    connect(m_nodeManager, &NodeManager::nodeAdded, this, &NodePanel::refreshNodeList);
    connect(m_nodeManager, &NodeManager::nodeRemoved, this, &NodePanel::refreshNodeList);
    connect(m_nodeManager, &NodeManager::nodeUpdated, this, &NodePanel::refreshNodeList);
}

void NodePanel::setupUi() {
    QVBoxLayout *mainLayout = new QVBoxLayout(this);
    
    // 模式选择
    QGroupBox *modeGroup = new QGroupBox("代理模式");
    QHBoxLayout *modeLayout = new QHBoxLayout(modeGroup);
    
    m_modeCombo = new QComboBox();
    m_modeCombo->addItem("🌐 系统代理模式", static_cast<int>(SystemProxy::System));
    m_modeCombo->addItem("🚀 TUN 全局模式", static_cast<int>(SystemProxy::TunMode));
    m_modeCombo->addItem("🔌 直连模式", static_cast<int>(SystemProxy::Direct));
    connect(m_modeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged),
            this, &NodePanel::onModeChanged);
    modeLayout->addWidget(m_modeCombo);
    
    m_currentModeLabel = new QLabel("当前: 直连");
    m_currentModeLabel->setStyleSheet("QLabel { color: #888; }");
    modeLayout->addWidget(m_currentModeLabel);
    modeLayout->addStretch();
    
    mainLayout->addWidget(modeGroup);
    
    // 节点列表
    QGroupBox *nodesGroup = new QGroupBox("节点列表");
    QVBoxLayout *nodesLayout = new QVBoxLayout(nodesGroup);
    
    m_nodeList = new QListWidget();
    m_nodeList->setMinimumHeight(200);
    connect(m_nodeList, &QListWidget::itemSelectionChanged,
            this, &NodePanel::onNodeSelectionChanged);
    nodesLayout->addWidget(m_nodeList);
    
    // 节点操作按钮
    QHBoxLayout *nodeButtonsLayout = new QHBoxLayout();
    
    m_addButton = new QPushButton("➕ 添加");
    connect(m_addButton, &QPushButton::clicked, this, &NodePanel::onAddNodeClicked);
    nodeButtonsLayout->addWidget(m_addButton);
    
    m_editButton = new QPushButton("✏ 编辑");
    m_editButton->setEnabled(false);
    connect(m_editButton, &QPushButton::clicked, this, &NodePanel::onEditNodeClicked);
    nodeButtonsLayout->addWidget(m_editButton);
    
    m_removeButton = new QPushButton("🗑 删除");
    m_removeButton->setEnabled(false);
    connect(m_removeButton, &QPushButton::clicked, this, &NodePanel::onRemoveNodeClicked);
    nodeButtonsLayout->addWidget(m_removeButton);
    
    m_testButton = new QPushButton("🔍 测速");
    m_testButton->setEnabled(false);
    connect(m_testButton, &QPushButton::clicked, this, &NodePanel::onTestNodeClicked);
    nodeButtonsLayout->addWidget(m_testButton);
    
    nodeButtonsLayout->addStretch();
    nodesLayout->addLayout(nodeButtonsLayout);
    
    mainLayout->addWidget(nodesGroup);
    
    // 节点信息和连接按钮
    QGroupBox *actionGroup = new QGroupBox("当前选中节点");
    QVBoxLayout *actionLayout = new QVBoxLayout(actionGroup);
    
    m_selectedNodeInfo = new QLabel("未选择节点");
    m_selectedNodeInfo->setStyleSheet("QLabel { padding: 10px; background: #2b2b2b; border-radius: 5px; }");
    actionLayout->addWidget(m_selectedNodeInfo);
    
    m_connectButton = new QPushButton("🚀 连接到此节点");
    m_connectButton->setEnabled(false);
    m_connectButton->setStyleSheet("QPushButton { padding: 10px; font-size: 14px; font-weight: bold; }");
    connect(m_connectButton, &QPushButton::clicked, this, &NodePanel::onConnectClicked);
    actionLayout->addWidget(m_connectButton);
    
    mainLayout->addWidget(actionGroup);
    
    mainLayout->addStretch();
}

void NodePanel::refreshNodeList() {
    m_nodeList->clear();
    
    QVector<ProxyNode> nodes = m_nodeManager->getAllNodes();
    for (const ProxyNode &node : nodes) {
        QListWidgetItem *item = new QListWidgetItem();
        item->setData(Qt::UserRole, node.id);
        m_nodeList->addItem(item);
        updateNodeItem(item, node);
    }
    
    // 选中当前节点
    QString currentId = m_nodeManager->getCurrentNodeId();
    if (!currentId.isEmpty()) {
        for (int i = 0; i < m_nodeList->count(); ++i) {
            QListWidgetItem *item = m_nodeList->item(i);
            if (item->data(Qt::UserRole).toString() == currentId) {
                m_nodeList->setCurrentItem(item);
                break;
            }
        }
    }
}

void NodePanel::updateNodeItem(QListWidgetItem *item, const ProxyNode &node) {
    QString pingText = node.ping > 0 ? QString::number(node.ping) + "ms" : "未测试";
    QString text = QString("📡 %1\n    服务器: %2\n    延迟: %3")
                       .arg(node.name, node.serverAddr, pingText);
    item->setText(text);
    
    // 当前节点高亮
    if (node.id == m_nodeManager->getCurrentNodeId()) {
        item->setBackground(QColor(60, 100, 60));
    }
}

void NodePanel::onNodeSelectionChanged() {
    bool hasSelection = m_nodeList->currentItem() != nullptr;
    m_editButton->setEnabled(hasSelection);
    m_removeButton->setEnabled(hasSelection);
    m_testButton->setEnabled(hasSelection);
    m_connectButton->setEnabled(hasSelection);
    
    if (hasSelection) {
        QString id = m_nodeList->currentItem()->data(Qt::UserRole).toString();
        ProxyNode node = m_nodeManager->getNode(id);
        
        QString info = QString(
            "<b>节点名称:</b> %1<br>"
            "<b>服务器:</b> %2<br>"
            "<b>ECH:</b> %3<br>"
            "<b>Yamux:</b> %4"
        ).arg(node.name,
              node.serverAddr,
              node.useEch ? "启用" : "禁用",
              node.useYamux ? "启用" : "禁用");
        
        m_selectedNodeInfo->setText(info);
        emit nodeSelected(id);
    } else {
        m_selectedNodeInfo->setText("未选择节点");
    }
}

void NodePanel::onAddNodeClicked() {
    NodeDialog dialog(this);
    if (dialog.exec() == QDialog::Accepted) {
        ProxyNode node = dialog.getNode();
        if (m_nodeManager->addNode(node)) {
            QMessageBox::information(this, "成功", "节点添加成功");
        } else {
            QMessageBox::warning(this, "失败", "节点添加失败");
        }
    }
}

void NodePanel::onEditNodeClicked() {
    QListWidgetItem *item = m_nodeList->currentItem();
    if (!item) return;
    
    QString id = item->data(Qt::UserRole).toString();
    ProxyNode node = m_nodeManager->getNode(id);
    
    NodeDialog dialog(node, this);
    if (dialog.exec() == QDialog::Accepted) {
        ProxyNode updatedNode = dialog.getNode();
        if (m_nodeManager->updateNode(id, updatedNode)) {
            QMessageBox::information(this, "成功", "节点更新成功");
        }
    }
}

void NodePanel::onRemoveNodeClicked() {
    QListWidgetItem *item = m_nodeList->currentItem();
    if (!item) return;
    
    QString id = item->data(Qt::UserRole).toString();
    ProxyNode node = m_nodeManager->getNode(id);
    
    QMessageBox::StandardButton reply = QMessageBox::question(
        this,
        "确认删除",
        QString("确定要删除节点 \"%1\" 吗？").arg(node.name),
        QMessageBox::Yes | QMessageBox::No
    );
    
    if (reply == QMessageBox::Yes) {
        if (m_nodeManager->removeNode(id)) {
            QMessageBox::information(this, "成功", "节点删除成功");
        }
    }
}

void NodePanel::onTestNodeClicked() {
    QListWidgetItem *item = m_nodeList->currentItem();
    if (!item) return;
    
    QString id = item->data(Qt::UserRole).toString();
    m_nodeManager->testNodeLatency(id);
    
    // TODO: 显示测试进度
    QMessageBox::information(this, "测速", "节点测速功能待实现");
}

void NodePanel::onConnectClicked() {
    QListWidgetItem *item = m_nodeList->currentItem();
    if (!item) return;
    
    QString id = item->data(Qt::UserRole).toString();
    ProxyNode node = m_nodeManager->getNode(id);
    
    SystemProxy::ProxyMode mode = static_cast<SystemProxy::ProxyMode>(
        m_modeCombo->currentData().toInt()
    );
    
    m_nodeManager->setCurrentNode(id);
    emit startRequested(node, mode);
}

void NodePanel::onModeChanged() {
    SystemProxy::ProxyMode mode = static_cast<SystemProxy::ProxyMode>(
        m_modeCombo->currentData().toInt()
    );
    
    QString modeText;
    switch (mode) {
        case SystemProxy::Direct:
            modeText = "直连";
            break;
        case SystemProxy::System:
            modeText = "系统代理";
            break;
        case SystemProxy::TunMode:
            modeText = "TUN 全局";
            break;
    }
    
    m_currentModeLabel->setText("当前: " + modeText);
}
