//! 配置面板

use egui::RichText;
use crate::gui::config::GuiConfig;

pub struct SettingsPanel;

impl SettingsPanel {
    pub fn show(ui: &mut egui::Ui, config: &mut GuiConfig) -> bool {
        let mut changed = false;

        ui.heading("⚙ 设置");
        ui.add_space(10.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // 基本设置
            ui.collapsing(RichText::new("📡 基本设置").strong(), |ui| {
                changed |= Self::show_basic_settings(ui, &mut config.basic);
            });

            ui.add_space(10.0);

            // ECH 设置
            ui.collapsing(RichText::new("🔒 ECH 设置").strong(), |ui| {
                changed |= Self::show_ech_settings(ui, &mut config.ech);
            });

            ui.add_space(10.0);

            // 高级设置
            ui.collapsing(RichText::new("🔧 高级设置").strong(), |ui| {
                changed |= Self::show_advanced_settings(ui, &mut config.advanced);
            });

            ui.add_space(10.0);

            // 应用设置
            ui.collapsing(RichText::new("🖥 应用设置").strong(), |ui| {
                changed |= Self::show_app_settings(ui, &mut config.app);
            });
        });

        changed
    }

    fn show_basic_settings(ui: &mut egui::Ui, config: &mut crate::gui::config::BasicConfig) -> bool {
        let mut changed = false;

        ui.label("监听地址:");
        changed |= ui.text_edit_singleline(&mut config.listen_addr).changed();
        ui.label("格式: IP:端口 (例如: 127.0.0.1:1080)");
        ui.add_space(5.0);

        ui.label("服务器地址:");
        changed |= ui.text_edit_singleline(&mut config.server_addr).changed();
        ui.label("Cloudflare Workers 地址");
        ui.add_space(5.0);

        ui.label("认证 Token:");
        changed |= ui.add(egui::TextEdit::singleline(&mut config.token).password(true)).changed();
        ui.add_space(5.0);

        changed |= ui.checkbox(&mut config.enable_tun, "启用 TUN 全局模式").changed();
        ui.label("⚠ 需要管理员权限");

        changed
    }

    fn show_ech_settings(ui: &mut egui::Ui, config: &mut crate::gui::config::EchConfig) -> bool {
        let mut changed = false;

        changed |= ui.checkbox(&mut config.enabled, "启用 ECH (Encrypted Client Hello)").changed();
        ui.label("加密 SNI，防止 TLS 指纹识别");
        ui.add_space(5.0);

        ui.add_enabled_ui(config.enabled, |ui| {
            ui.label("ECH 域名:");
            changed |= ui.text_edit_singleline(&mut config.domain).changed();
            ui.add_space(5.0);

            ui.label("DoH 服务器:");
            changed |= ui.text_edit_singleline(&mut config.doh_server).changed();
            ui.label("用于查询 ECH 配置");
        });

        changed
    }

    fn show_advanced_settings(ui: &mut egui::Ui, config: &mut crate::gui::config::AdvancedConfig) -> bool {
        let mut changed = false;

        changed |= ui.checkbox(&mut config.enable_yamux, "启用 Yamux 多路复用").changed();
        ui.label("提升连接复用效率");
        ui.add_space(5.0);

        changed |= ui.checkbox(&mut config.enable_fingerprint_randomization, "启用指纹随机化").changed();
        ui.label("GREASE + 扩展顺序随机化");
        ui.add_space(5.0);

        ui.label("TLS 指纹配置:");
        egui::ComboBox::from_id_source("tls_profile")
            .selected_text(&config.tls_profile)
            .show_ui(ui, |ui| {
                changed |= ui.selectable_value(&mut config.tls_profile, "Chrome".to_string(), "Chrome 120+").changed();
                changed |= ui.selectable_value(&mut config.tls_profile, "BoringSSLDefault".to_string(), "BoringSSL 默认").changed();
            });

        changed
    }

    fn show_app_settings(ui: &mut egui::Ui, config: &mut crate::gui::config::AppConfig) -> bool {
        let mut changed = false;

        changed |= ui.checkbox(&mut config.auto_start, "开机自启").changed();
        ui.add_space(5.0);

        changed |= ui.checkbox(&mut config.start_minimized, "启动时最小化").changed();
        ui.add_space(5.0);

        changed |= ui.checkbox(&mut config.minimize_to_tray, "最小化到系统托盘").changed();
        ui.add_space(5.0);

        changed |= ui.checkbox(&mut config.close_to_tray, "关闭时最小化到托盘").changed();

        changed
    }
}
