#pragma once

#include <QObject>
#include <QJsonObject>
#include <QJsonArray>
#include <QString>
#include <QVector>

struct ProxyNode {
    QString id;              // 唯一标识
    QString name;            // 节点名称
    QString serverAddr;      // 服务器地址
    QString token;           // 认证 Token
    bool useEch;             // 启用 ECH
    QString echDomain;       // ECH 域名
    QString dohServer;       // DoH 服务器
    bool useYamux;           // Yamux 多路复用
    QString tlsProfile;      // TLS 指纹
    
    // 统计信息
    qint64 lastUsedTime;     // 最后使用时间
    quint64 totalTraffic;    // 累计流量
    int ping;                // 延迟 (ms)
    
    QJsonObject toJson() const;
    static ProxyNode fromJson(const QJsonObject &json);
};

class NodeManager : public QObject {
    Q_OBJECT

public:
    explicit NodeManager(QObject *parent = nullptr);
    
    // 节点管理
    bool addNode(const ProxyNode &node);
    bool removeNode(const QString &id);
    bool updateNode(const QString &id, const ProxyNode &node);
    ProxyNode getNode(const QString &id) const;
    QVector<ProxyNode> getAllNodes() const;
    
    // 当前节点
    bool setCurrentNode(const QString &id);
    ProxyNode getCurrentNode() const;
    QString getCurrentNodeId() const { return m_currentNodeId; }
    
    // 持久化
    bool save();
    bool load();
    QString configPath() const;
    
    // 节点测试
    void testNodeLatency(const QString &id);
    
signals:
    void nodeAdded(const QString &id);
    void nodeRemoved(const QString &id);
    void nodeUpdated(const QString &id);
    void currentNodeChanged(const QString &id);
    void latencyTestResult(const QString &id, int ping);

private:
    QVector<ProxyNode> m_nodes;
    QString m_currentNodeId;
    QString m_configPath;
};
