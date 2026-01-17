#include "systemproxy.h"
#include <QDebug>

#ifdef Q_OS_WIN
#include <Windows.h>
#include <wininet.h>
#pragma comment(lib, "wininet.lib")
#endif

SystemProxy::SystemProxy(QObject *parent)
    : QObject(parent)
{
#ifdef Q_OS_WIN
    // 保存原始代理设置
    INTERNET_PER_CONN_OPTION_LIST list;
    DWORD dwBufSize = sizeof(list);
    
    INTERNET_PER_CONN_OPTION options[2];
    options[0].dwOption = INTERNET_PER_CONN_FLAGS;
    options[1].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
    
    list.dwSize = sizeof(list);
    list.pszConnection = nullptr;
    list.dwOptionCount = 2;
    list.dwOptionError = 0;
    list.pOptions = options;
    
    if (InternetQueryOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, &dwBufSize)) {
        m_originalProxyEnabled = (options[0].Value.dwValue & PROXY_TYPE_PROXY) != 0;
        if (options[1].Value.pszValue) {
            m_originalProxyServer = QString::fromWCharArray(options[1].Value.pszValue);
            GlobalFree(options[1].Value.pszValue);
        }
    }
#endif
}

SystemProxy::~SystemProxy() {
    if (m_mode == System) {
        disableSystemProxy();
    }
}

bool SystemProxy::enableSystemProxy(const QString &address, quint16 port) {
    QString server = QString("%1:%2").arg(address).arg(port);
    QString bypass = "localhost;127.*;10.*;172.16.*;172.31.*;192.168.*;<local>";
    
    if (!setWindowsProxy(true, server, bypass)) {
        emit errorOccurred("Failed to enable system proxy");
        return false;
    }
    
    m_mode = System;
    m_lastProxyAddress = address;
    m_lastProxyPort = port;
    
    emit modeChanged(m_mode);
    return true;
}

bool SystemProxy::disableSystemProxy() {
    // 幂等设计：如果已经是直连模式，直接返回成功
    if (m_mode == Direct && !isSystemProxyEnabled()) {
        return true;
    }
    
    // CRITICAL: 错误静默处理，防止退出时弹窗
    if (!setWindowsProxy(false)) {
        qWarning() << "[SystemProxy] Failed to disable proxy (non-fatal on exit)";
        // 不发射 errorOccurred 信号，避免退出时弹窗
        m_mode = Direct;
        return false;
    }
    
    m_mode = Direct;
    emit modeChanged(m_mode);
    return true;
}

bool SystemProxy::isSystemProxyEnabled() const {
#ifdef Q_OS_WIN
    INTERNET_PER_CONN_OPTION_LIST list;
    DWORD dwBufSize = sizeof(list);
    
    INTERNET_PER_CONN_OPTION option;
    option.dwOption = INTERNET_PER_CONN_FLAGS;
    
    list.dwSize = sizeof(list);
    list.pszConnection = nullptr;
    list.dwOptionCount = 1;
    list.dwOptionError = 0;
    list.pOptions = &option;
    
    if (InternetQueryOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, &dwBufSize)) {
        return (option.Value.dwValue & PROXY_TYPE_PROXY) != 0;
    }
#endif
    return false;
}

QString SystemProxy::getSystemProxyAddress() const {
#ifdef Q_OS_WIN
    INTERNET_PER_CONN_OPTION_LIST list;
    DWORD dwBufSize = sizeof(list);
    
    INTERNET_PER_CONN_OPTION option;
    option.dwOption = INTERNET_PER_CONN_PROXY_SERVER;
    
    list.dwSize = sizeof(list);
    list.pszConnection = nullptr;
    list.dwOptionCount = 1;
    list.dwOptionError = 0;
    list.pOptions = &option;
    
    if (InternetQueryOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, &dwBufSize)) {
        if (option.Value.pszValue) {
            QString server = QString::fromWCharArray(option.Value.pszValue);
            GlobalFree(option.Value.pszValue);
            return server;
        }
    }
#endif
    return QString();
}

bool SystemProxy::enablePacProxy(const QString &pacUrl) {
#ifdef Q_OS_WIN
    INTERNET_PER_CONN_OPTION_LIST list;
    INTERNET_PER_CONN_OPTION options[2];
    
    options[0].dwOption = INTERNET_PER_CONN_FLAGS;
    options[0].Value.dwValue = PROXY_TYPE_AUTO_PROXY_URL;
    
    options[1].dwOption = INTERNET_PER_CONN_AUTOCONFIG_URL;
    options[1].Value.pszValue = (LPWSTR)pacUrl.toStdWString().c_str();
    
    list.dwSize = sizeof(list);
    list.pszConnection = nullptr;
    list.dwOptionCount = 2;
    list.dwOptionError = 0;
    list.pOptions = options;
    
    if (!InternetSetOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, sizeof(list))) {
        return false;
    }
    
    return refreshProxySettings();
#else
    return false;
#endif
}

bool SystemProxy::setMode(ProxyMode mode, const QString &address, quint16 port) {
    switch (mode) {
        case Direct:
            return disableSystemProxy();
        
        case System:
            if (address.isEmpty() || port == 0) {
                if (m_lastProxyAddress.isEmpty()) {
                    emit errorOccurred("No proxy address specified");
                    return false;
                }
                return enableSystemProxy(m_lastProxyAddress, m_lastProxyPort);
            }
            return enableSystemProxy(address, port);
        
        case TunMode:
            // TUN 模式由 ProcessManager 启动后端时处理
            m_mode = TunMode;
            emit modeChanged(m_mode);
            return true;
        
        default:
            return false;
    }
}

bool SystemProxy::setWindowsProxy(bool enable, const QString &server, const QString &bypass) {
#ifdef Q_OS_WIN
    INTERNET_PER_CONN_OPTION_LIST list;
    INTERNET_PER_CONN_OPTION options[3];
    
    DWORD dwFlags = enable ? PROXY_TYPE_PROXY : PROXY_TYPE_DIRECT;
    
    options[0].dwOption = INTERNET_PER_CONN_FLAGS;
    options[0].Value.dwValue = dwFlags;
    
    options[1].dwOption = INTERNET_PER_CONN_PROXY_SERVER;
    options[1].Value.pszValue = enable ? (LPWSTR)server.toStdWString().c_str() : nullptr;
    
    options[2].dwOption = INTERNET_PER_CONN_PROXY_BYPASS;
    options[2].Value.pszValue = enable ? (LPWSTR)bypass.toStdWString().c_str() : nullptr;
    
    list.dwSize = sizeof(list);
    list.pszConnection = nullptr;
    list.dwOptionCount = 3;
    list.dwOptionError = 0;
    list.pOptions = options;
    
    if (!InternetSetOption(nullptr, INTERNET_OPTION_PER_CONNECTION_OPTION, &list, sizeof(list))) {
        qWarning() << "Failed to set proxy options. Error:" << GetLastError();
        return false;
    }
    
    return refreshProxySettings();
#else
    Q_UNUSED(enable);
    Q_UNUSED(server);
    Q_UNUSED(bypass);
    return false;
#endif
}

bool SystemProxy::refreshProxySettings() {
#ifdef Q_OS_WIN
    if (!InternetSetOption(nullptr, INTERNET_OPTION_SETTINGS_CHANGED, nullptr, 0)) {
        qWarning() << "Failed to refresh proxy settings";
        return false;
    }
    
    if (!InternetSetOption(nullptr, INTERNET_OPTION_REFRESH, nullptr, 0)) {
        qWarning() << "Failed to refresh Internet options";
        return false;
    }
    
    return true;
#else
    return false;
#endif
}
