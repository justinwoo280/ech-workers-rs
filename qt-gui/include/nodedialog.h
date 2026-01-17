#pragma once

#include <QDialog>
#include <QLineEdit>
#include <QCheckBox>
#include <QComboBox>
#include "nodemanager.h"

class NodeDialog : public QDialog {
    Q_OBJECT

public:
    explicit NodeDialog(QWidget *parent = nullptr);
    explicit NodeDialog(const ProxyNode &node, QWidget *parent = nullptr);

    ProxyNode getNode() const;

private slots:
    void onSaveClicked();
    void onCancelClicked();
    void onTestConnectionClicked();

private:
    void setupUi();
    void loadNode(const ProxyNode &node);

    QLineEdit *m_nameEdit;
    QLineEdit *m_serverAddrEdit;
    QLineEdit *m_tokenEdit;
    QCheckBox *m_useEchCheck;
    QLineEdit *m_echDomainEdit;
    QLineEdit *m_dohServerEdit;
    QCheckBox *m_useYamuxCheck;
    QComboBox *m_tlsProfileCombo;

    ProxyNode m_node;
    bool m_editMode = false;
};
