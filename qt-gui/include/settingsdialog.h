#pragma once

#include <QDialog>
#include <QLineEdit>
#include <QCheckBox>
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

    // 代理设置
    QLineEdit *m_listenAddrEdit;
    QCheckBox *m_enableTunCheck;

    // 应用设置
    QCheckBox *m_autoStartCheck;
    QCheckBox *m_startMinimizedCheck;
    QCheckBox *m_minimizeToTrayCheck;
    QCheckBox *m_closeToTrayCheck;
};
