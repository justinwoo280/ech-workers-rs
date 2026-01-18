#include <QApplication>
#include <QSharedMemory>
#include <QMessageBox>
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

    // 单实例检查：使用共享内存防止多个实例运行
    QSharedMemory singleInstance("ECH_Workers_RS_SingleInstance_Lock");
    if (!singleInstance.create(1)) {
        // 如果创建失败，说明已有实例在运行
        QMessageBox::warning(nullptr, "ECH Workers RS", 
            "程序已在运行中！\n请检查系统托盘或任务管理器。");
        return 1;
    }

    // CRITICAL: 注册退出清理函数，防止系统代理残留
    qAddPostRoutine(cleanupOnExit);

    MainWindow window;
    window.show();

    return app.exec();
}
