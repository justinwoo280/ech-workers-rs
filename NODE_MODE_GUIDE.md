#pragma once

#include <QWidget>
#include <QListWidget>
#include <QPushButton>
#include <QLabel>
#include "nodemanager.h"
#include "systemproxy.h"

class NodePanel : public QWidget {
    Q_OBJECT

public:
    explicit NodePanel(NodeManager *nodeManager, SystemProxy *systemProxy, QWidget *parent = nullptr);

signals:
    void nodeSelected(const QString &id);
    void startRequested(const ProxyNode &node, SystemProxy::ProxyMode mode);

private slots:
    void onAddNodeClicked();
    void onEditNodeClicked();
    void onRemoveNodeClicked();
    void onTestNodeClicked();
    void onConnectClicked();
    void onModeChanged();
    void refreshNodeList();
    void onNodeSelectionChanged();

private:
    void setupUi();
    void updateNodeItem(QListWidgetItem *item, const ProxyNode &node);

    NodeManager *m_nodeManager;
    SystemProxy *m_systemProxy;
    
    QListWidget *m_nodeList;
    QPushButton *m_addButton;
    QPushButton *m_editButton;
    QPushButton *m_removeButton;
    QPushButton *m_testButton;
    QPushButton *m_connectButton;
    
    QComboBox *m_modeCombo;
    QLabel *m_currentModeLabel;
    QLabel *m_selectedNodeInfo;
};
