# 防呆安全设计 (Fail-Safe Design)

## 核心问题

**用户关闭软件后，系统代理仍然开启 → 用户网络断开 → 被骂流氓软件**

---

## 解决方案：多层清理机制

### 1. 主动停止时清理 ✅

**位置**: `mainwindow.cpp::onStartStopClicked()`

```cpp
void MainWindow::onStartStopClicked() {
    if (m_processManager->status() == ProcessManager::ProxyStatus::Running) {
        // 停止代理时立即清理系统代理
        if (m_systemProxy) {
            m_systemProxy->disableSystemProxy();
        }
        m_processManager->stop();
    }
}
```

---

### 2. 窗口关闭时清理 ✅

**位置**: `mainwindow.cpp::closeEvent()`

```cpp
void MainWindow::closeEvent(QCloseEvent *event) {
    // 用户点击关闭按钮时
    // CRITICAL: 强制清理系统代理，防止用户网络断开
    if (m_systemProxy) {
        m_systemProxy->disableSystemProxy();
    }
    
    m_processManager->stop();
    event->accept();
}
```

**覆盖场景**:
- 用户点击窗口 X 按钮
- Alt+F4 快捷键
- 任务管理器结束任务（部分）

---

### 3. 托盘退出时清理 ✅

**位置**: `mainwindow.cpp::onTrayActionTriggered()`

```cpp
void MainWindow::onTrayActionTriggered(const QString &action) {
    if (action == "quit") {
        // CRITICAL: 托盘退出时强制清理系统代理
        if (m_systemProxy) {
            m_systemProxy->disableSystemProxy();
        }
        m_processManager->stop();
        QApplication::quit();
    }
}
```

**覆盖场景**:
- 托盘右键 → 退出
- 最小化到托盘后退出

---

### 4. 析构函数兜底 ✅

**位置**: `mainwindow.cpp::~MainWindow()`

```cpp
MainWindow::~MainWindow() {
    // 析构时强制清理系统代理（兜底保护）
    if (m_systemProxy) {
        m_systemProxy->disableSystemProxy();
    }
    m_processManager->stop();
}
```

**覆盖场景**:
- 所有对象销毁路径
- 程序正常退出
- 部分异常退出

---

### 5. 全局退出清理 ✅

**位置**: `main.cpp::cleanupOnExit()`

```cpp
void cleanupOnExit() {
    SystemProxy cleanup;
    cleanup.disableSystemProxy();
}

int main() {
    // CRITICAL: 注册退出清理函数，防止系统代理残留
    qAddPostRoutine(cleanupOnExit);
}
```

**覆盖场景**:
- `QApplication::quit()` 调用后
- 程序正常退出流程
- 最后一道防线

---

## 幂等设计：防止重复清理错误

**位置**: `systemproxy.cpp::disableSystemProxy()`

```cpp
bool SystemProxy::disableSystemProxy() {
    // 幂等设计：如果已经是直连模式，直接返回成功
    if (m_mode == Direct && !isSystemProxyEnabled()) {
        return true;
    }
    
    // CRITICAL: 错误静默处理，防止退出时弹窗
    if (!setWindowsProxy(false)) {
        qWarning() << "[SystemProxy] Failed to disable proxy (non-fatal on exit)";
        // 不发射 errorOccurred 信号，避免退出时弹窗打断用户
        m_mode = Direct;
        return false;
    }
    
    m_mode = Direct;
    emit modeChanged(m_mode);
    return true;
}
```

**关键点**:
- ✅ 多次调用不会出错
- ✅ 失败时不弹窗（退出时体验优先）
- ✅ 记录警告日志供调试

---

## 清理顺序保证

```
用户操作 (点击关闭/托盘退出/Alt+F4)
    ↓
closeEvent() / onTrayActionTriggered()
    ↓
m_systemProxy->disableSystemProxy()  ← 第一层清理
    ↓
m_processManager->stop()
    ↓
~MainWindow()                         ← 第二层清理 (析构)
    ↓
qAddPostRoutine(cleanupOnExit)        ← 第三层清理 (全局)
    ↓
程序退出
```

---

## 异常情况覆盖

| 场景 | 是否清理 | 清理路径 |
|------|---------|---------|
| 正常关闭窗口 | ✅ | closeEvent() |
| 托盘退出 | ✅ | onTrayActionTriggered() |
| Alt+F4 | ✅ | closeEvent() |
| 任务管理器结束 | ⚠️ 部分 | 析构函数 (如果来得及) |
| 程序崩溃 | ❌ | **无法保证** |
| 断电/蓝屏 | ❌ | **物理层面无法处理** |

---

## 崩溃保护：启动时检测

**建议添加**: 程序启动时检测并清理残留代理

```cpp
// main.cpp
int main() {
    // 启动时清理残留代理（防上次崩溃）
    SystemProxy startup;
    if (startup.isSystemProxyEnabled()) {
        QString proxyAddr = startup.getSystemProxyAddress();
        if (proxyAddr.contains("127.0.0.1:1080")) {
            qWarning() << "Detected stale proxy, cleaning up...";
            startup.disableSystemProxy();
        }
    }
}
```

---

## 用户提示优化

### 停止代理时提示

```cpp
void MainWindow::onStartStopClicked() {
    if (running) {
        m_systemProxy->disableSystemProxy();
        m_processManager->stop();
        
        // 可选：状态栏提示
        statusBar()->showMessage("已停止代理并恢复系统设置", 3000);
    }
}
```

### 退出时提示（可选）

```cpp
void MainWindow::closeEvent(QCloseEvent *event) {
    if (m_processManager->status() == Running) {
        QMessageBox::StandardButton reply = QMessageBox::question(
            this, "确认退出",
            "代理正在运行，退出将自动停止代理并恢复系统设置。\n确定退出？",
            QMessageBox::Yes | QMessageBox::No
        );
        
        if (reply == QMessageBox::No) {
            event->ignore();
            return;
        }
    }
    
    m_systemProxy->disableSystemProxy();
    m_processManager->stop();
    event->accept();
}
```

---

## 测试清单

- [ ] 正常关闭窗口 → 系统代理已清除
- [ ] 托盘退出 → 系统代理已清除
- [ ] Alt+F4 → 系统代理已清除
- [ ] 任务管理器结束进程 → 系统代理已清除（尽力而为）
- [ ] 多次点击停止按钮 → 不报错
- [ ] 停止后再退出 → 不重复清理出错
- [ ] 启动时检测到残留代理 → 自动清理

---

## 总结

**5 层防呆机制**:
1. **主动停止**: 用户点击停止时清理
2. **窗口关闭**: closeEvent() 清理
3. **托盘退出**: 托盘菜单清理
4. **析构兜底**: ~MainWindow() 清理
5. **全局清理**: qAddPostRoutine() 最后一道防线

**设计原则**:
- ✅ **宁可多清理，不能少清理**
- ✅ **幂等设计，多次调用安全**
- ✅ **错误静默，退出时不弹窗**
- ✅ **启动检测，清理上次残留**

**用户体验**:
- 不会出现"关闭软件后网络断开"
- 不会被骂流氓软件
- 退出流程丝滑无阻
