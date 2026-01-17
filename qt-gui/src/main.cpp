#include <QApplication>
#include "mainwindow.h"
#include "systemproxy.h"
#include <QObject>

// 全局退出清理函数（防呆设计）
void cleanupOnExit() {
    SystemProxy cleanup;
    cleanup.disableSystemProxy();
}

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);

    app.setOrganizationName("ech-workers");
    app.setOrganizationDomain("ech-workers.com");
    app.setApplicationName("ECH Workers RS");
    app.setApplicationVersion("0.1.0");

    // CRITICAL: 注册退出清理函数，防止系统代理残留
    qAddPostRoutine(cleanupOnExit);

    MainWindow window;
    window.show();

    return app.exec();
}
