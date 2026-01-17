#pragma once

#include <QObject>
#include <QJsonObject>
#include <QString>

class ConfigManager : public QObject {
    Q_OBJECT

public:
    explicit ConfigManager(QObject *parent = nullptr);

    QJsonObject loadConfig();
    bool saveConfig(const QJsonObject &config);
    QString configPath() const;

private:
    QString m_configPath;
    QJsonObject m_defaultConfig;

    QJsonObject createDefaultConfig() const;
    void ensureConfigDir();
};
