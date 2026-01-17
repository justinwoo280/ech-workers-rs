#include "nodemanager.h"
#include <QStandardPaths>
#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QDateTime>
#include <QUuid>

QJsonObject ProxyNode::toJson() const {
    QJsonObject json;
    json["id"] = id;
    json["name"] = name;
    json["server_addr"] = serverAddr;
    json["token"] = token;
    json["use_ech"] = useEch;
    json["ech_domain"] = echDomain;
    json["doh_server"] = dohServer;
    json["use_yamux"] = useYamux;
    json["tls_profile"] = tlsProfile;
    json["last_used_time"] = lastUsedTime;
    json["total_traffic"] = static_cast<qint64>(totalTraffic);
    json["ping"] = ping;
    return json;
}

ProxyNode ProxyNode::fromJson(const QJsonObject &json) {
    ProxyNode node;
    node.id = json["id"].toString();
    node.name = json["name"].toString();
    node.serverAddr = json["server_addr"].toString();
    node.token = json["token"].toString();
    node.useEch = json["use_ech"].toBool(true);
    node.echDomain = json["ech_domain"].toString("cloudflare-ech.com");
    node.dohServer = json["doh_server"].toString("223.5.5.5/dns-query");
    node.useYamux = json["use_yamux"].toBool(true);
    node.tlsProfile = json["tls_profile"].toString("Chrome");
    node.lastUsedTime = json["last_used_time"].toVariant().toLongLong();
    node.totalTraffic = json["total_traffic"].toVariant().toULongLong();
    node.ping = json["ping"].toInt(-1);
    return node;
}

NodeManager::NodeManager(QObject *parent)
    : QObject(parent)
{
    QString configDir = QStandardPaths::writableLocation(QStandardPaths::AppConfigLocation);
    m_configPath = configDir + "/nodes.json";
    load();
}

bool NodeManager::addNode(const ProxyNode &node) {
    ProxyNode newNode = node;
    if (newNode.id.isEmpty()) {
        newNode.id = QUuid::createUuid().toString(QUuid::WithoutBraces);
    }
    
    for (const auto &n : m_nodes) {
        if (n.id == newNode.id) {
            return false;
        }
    }
    
    m_nodes.append(newNode);
    emit nodeAdded(newNode.id);
    save();
    return true;
}

bool NodeManager::removeNode(const QString &id) {
    for (int i = 0; i < m_nodes.size(); ++i) {
        if (m_nodes[i].id == id) {
            m_nodes.removeAt(i);
            if (m_currentNodeId == id) {
                m_currentNodeId.clear();
            }
            emit nodeRemoved(id);
            save();
            return true;
        }
    }
    return false;
}

bool NodeManager::updateNode(const QString &id, const ProxyNode &node) {
    for (int i = 0; i < m_nodes.size(); ++i) {
        if (m_nodes[i].id == id) {
            m_nodes[i] = node;
            m_nodes[i].id = id;
            emit nodeUpdated(id);
            save();
            return true;
        }
    }
    return false;
}

ProxyNode NodeManager::getNode(const QString &id) const {
    for (const auto &node : m_nodes) {
        if (node.id == id) {
            return node;
        }
    }
    return ProxyNode();
}

QVector<ProxyNode> NodeManager::getAllNodes() const {
    return m_nodes;
}

bool NodeManager::setCurrentNode(const QString &id) {
    for (auto &node : m_nodes) {
        if (node.id == id) {
            m_currentNodeId = id;
            node.lastUsedTime = QDateTime::currentMSecsSinceEpoch();
            emit currentNodeChanged(id);
            save();
            return true;
        }
    }
    return false;
}

ProxyNode NodeManager::getCurrentNode() const {
    if (m_currentNodeId.isEmpty()) {
        return ProxyNode();
    }
    return getNode(m_currentNodeId);
}

bool NodeManager::save() {
    QFileInfo fileInfo(m_configPath);
    QDir dir = fileInfo.absoluteDir();
    if (!dir.exists()) {
        dir.mkpath(".");
    }
    
    QJsonObject root;
    root["current_node_id"] = m_currentNodeId;
    
    QJsonArray nodesArray;
    for (const auto &node : m_nodes) {
        nodesArray.append(node.toJson());
    }
    root["nodes"] = nodesArray;
    
    QFile file(m_configPath);
    if (!file.open(QIODevice::WriteOnly)) {
        return false;
    }
    
    QJsonDocument doc(root);
    file.write(doc.toJson(QJsonDocument::Indented));
    file.close();
    
    return true;
}

bool NodeManager::load() {
    QFile file(m_configPath);
    if (!file.exists()) {
        return false;
    }
    
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    
    QByteArray data = file.readAll();
    file.close();
    
    QJsonParseError error;
    QJsonDocument doc = QJsonDocument::fromJson(data, &error);
    if (error.error != QJsonParseError::NoError) {
        return false;
    }
    
    QJsonObject root = doc.object();
    m_currentNodeId = root["current_node_id"].toString();
    
    QJsonArray nodesArray = root["nodes"].toArray();
    m_nodes.clear();
    for (const QJsonValue &value : nodesArray) {
        m_nodes.append(ProxyNode::fromJson(value.toObject()));
    }
    
    return true;
}

QString NodeManager::configPath() const {
    return m_configPath;
}

void NodeManager::testNodeLatency(const QString &id) {
    // TODO: 实现 ping 测试
    // 可以通过 QTcpSocket 连接测试延迟
    // 或使用 ICMP (需要额外权限)
    
    emit latencyTestResult(id, 100);
}
