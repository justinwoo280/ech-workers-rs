#pragma once

#include <QDialog>
#include <QLineEdit>
#include <QCheckBox>
#include <QComboBox>
#include <QTabWidget>
#include "configmanager.h"

class SettingsDialog : public QDialog {
    Q_OBJECT

public:
    explicit SettingsDialog(ConfigManager *configManager, QWidget *parent = nullptr);

private slots:
    void onSaveClicked();
    void onCancelClicked();

private:
    void setupUi();
    void loadSettings();
    void saveSettings();

    ConfigManager *m_configManager;
    QJsonObject m_config;

    QLineEdit *m_listenAddrEdit;
    QLineEdit *m_serverAddrEdit;
    QLineEdit *m_tokenEdit;
    QCheckBox *m_enableTunCheck;

    QCheckBox *m_echEnabledCheck;
    QLineEdit *m_echDomainEdit;
    QLineEdit *m_dohServerEdit;

    QCheckBox *m_yamuxCheck;
    QCheckBox *m_fingerprintCheck;
    QComboBox *m_tlsProfileCombo;

    QCheckBox *m_autoStartCheck;
    QCheckBox *m_startMinimizedCheck;
    QCheckBox *m_minimizeToTrayCheck;
    QCheckBox *m_closeToTrayCheck;
};
