#include "configmanager.h"
#include <QStandardPaths>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QDebug>

ConfigManager::ConfigManager(QObject *parent)
    : QObject(parent)
{
    QString configDir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    m_configPath = configDir + "/config.json";
    m_defaultConfig = createDefaultConfig();
    ensureConfigDir();
}

QJsonObject ConfigManager::loadConfig() {
    QFile file(m_configPath);
    if (!file.exists()) {
        saveConfig(m_defaultConfig);
        return m_defaultConfig;
    }

    if (!file.open(QIODevice::ReadOnly)) {
        qWarning() << "Failed to open config file:" << m_configPath;
        return m_defaultConfig;
    }

    QByteArray data = file.readAll();
    file.close();

    QJsonParseError error;
    QJsonDocument doc = QJsonDocument::fromJson(data, &error);

    if (error.error != QJsonParseError::NoError) {
        qWarning() << "Failed to parse config:" << error.errorString();
        return m_defaultConfig;
    }

    return doc.object();
}

bool ConfigManager::saveConfig(const QJsonObject &config) {
    ensureConfigDir();

    QFile file(m_configPath);
    if (!file.open(QIODevice::WriteOnly)) {
        qWarning() << "Failed to write config file:" << m_configPath;
        return false;
    }

    QJsonDocument doc(config);
    file.write(doc.toJson(QJsonDocument::Indented));
    file.close();

    return true;
}

QString ConfigManager::configPath() const {
    return m_configPath;
}

QJsonObject ConfigManager::createDefaultConfig() const {
    QJsonObject config;

    QJsonObject basic;
    basic["listen_addr"] = "127.0.0.1:1080";
    basic["server_addr"] = "your-worker.workers.dev";
    basic["token"] = "";
    basic["enable_tun"] = false;
    config["basic"] = basic;

    QJsonObject ech;
    ech["enabled"] = true;
    ech["domain"] = "cloudflare-ech.com";
    ech["doh_server"] = "223.5.5.5/dns-query";
    config["ech"] = ech;

    QJsonObject advanced;
    advanced["enable_yamux"] = true;
    advanced["enable_fingerprint_randomization"] = true;
    advanced["tls_profile"] = "Chrome";
    config["advanced"] = advanced;

    QJsonObject app;
    app["auto_start"] = false;
    app["start_minimized"] = false;
    app["minimize_to_tray"] = true;
    app["close_to_tray"] = true;
    config["app"] = app;

    return config;
}

void ConfigManager::ensureConfigDir() {
    QFileInfo fileInfo(m_configPath);
    QDir dir = fileInfo.absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }
}
