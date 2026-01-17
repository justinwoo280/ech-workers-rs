#pragma once

#include <QWidget>
#include <QListWidget>
#include <QPushButton>
#include <QLabel>
#include <QComboBox>
#include "nodemanager.h"
#include "systemproxy.h"

class NodePanel : public QWidget {
    Q_OBJECT

public:
    explicit NodePanel(NodeManager *nodeManager, SystemProxy *systemProxy, QWidget *parent = nullptr);

signals:
    void nodeSelected(const QString &id);
    void currentNodeChanged(const QString &id);

public:
    QString getCurrentNodeId() const;
    ProxyNode getCurrentNode() const;
    SystemProxy::ProxyMode getCurrentMode() const;

private slots:
    void onAddNodeClicked();
    void onEditNodeClicked();
    void onRemoveNodeClicked();
    void onTestNodeClicked();
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
    
    QComboBox *m_modeCombo;
    QLabel *m_currentModeLabel;
    QLabel *m_selectedNodeInfo;
};
