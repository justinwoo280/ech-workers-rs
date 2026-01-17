#pragma once

#include <QObject>
#include <QString>

class SystemProxy : public QObject {
    Q_OBJECT

public:
    enum ProxyMode {
        Direct,       // 直连
        System,       // 系统代理
        TunMode       // TUN 全局模式
    };

    explicit SystemProxy(QObject *parent = nullptr);
    ~SystemProxy();

    // 系统代理模式
    bool enableSystemProxy(const QString &address, quint16 port);
    bool disableSystemProxy();
    bool isSystemProxyEnabled() const;
    QString getSystemProxyAddress() const;

    // PAC 代理
    bool enablePacProxy(const QString &pacUrl);
    
    // 代理模式
    ProxyMode currentMode() const { return m_mode; }
    bool setMode(ProxyMode mode, const QString &address = "", quint16 port = 0);

signals:
    void modeChanged(ProxyMode mode);
    void errorOccurred(const QString &error);

private:
    bool setWindowsProxy(bool enable, const QString &server = "", const QString &bypass = "");
    bool refreshProxySettings();
    
    ProxyMode m_mode = Direct;
    QString m_lastProxyAddress;
    quint16 m_lastProxyPort = 0;
    
#ifdef Q_OS_WIN
    bool m_originalProxyEnabled = false;
    QString m_originalProxyServer;
#endif
};
