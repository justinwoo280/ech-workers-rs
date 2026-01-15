//! 状态面板

use egui::{RichText, Color32};
use crate::gui::state::{AppState, ProxyStatus, Statistics};

pub struct DashboardPanel;

impl DashboardPanel {
    pub fn show(ui: &mut egui::Ui, state: &AppState) {
        ui.heading("📊 状态面板");
        ui.add_space(10.0);

        // 连接状态卡片
        egui::Frame::group(ui.style())
            .fill(ui.style().visuals.faint_bg_color)
            .inner_margin(10.0)
            .show(ui, |ui| {
                Self::show_status_card(ui, state);
            });

        ui.add_space(10.0);

        // 流量统计卡片
        egui::Frame::group(ui.style())
            .fill(ui.style().visuals.faint_bg_color)
            .inner_margin(10.0)
            .show(ui, |ui| {
                Self::show_statistics_card(ui, &state.statistics);
            });

        ui.add_space(10.0);

        // 错误信息（如果有）
        if let Some(ref error) = state.last_error {
            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(60, 20, 20))
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("❌ 错误").color(Color32::RED).strong());
                    ui.add_space(5.0);
                    ui.label(RichText::new(error).color(Color32::LIGHT_RED));
                });
        }
    }

    fn show_status_card(ui: &mut egui::Ui, state: &AppState) {
        ui.horizontal(|ui| {
            // 状态指示器
            let status_color = state.status.color();
            ui.label(RichText::new("●").size(24.0).color(status_color));
            
            ui.vertical(|ui| {
                ui.label(RichText::new("代理状态").strong());
                ui.label(RichText::new(state.status.to_string()).color(status_color));
            });
        });

        ui.add_space(10.0);

        // 运行时间
        if state.status.is_running() {
            ui.horizontal(|ui| {
                ui.label(RichText::new("⏱").size(16.0));
                ui.label("运行时间:");
                ui.label(
                    RichText::new(Statistics::format_uptime(state.statistics.uptime()))
                        .strong()
                        .color(Color32::LIGHT_GREEN)
                );
            });
        }
    }

    fn show_statistics_card(ui: &mut egui::Ui, stats: &Statistics) {
        ui.label(RichText::new("📈 流量统计").strong());
        ui.add_space(5.0);

        egui::Grid::new("stats_grid")
            .num_columns(2)
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
                // 上传
                ui.label("⬆ 上传:");
                ui.label(
                    RichText::new(Statistics::format_bytes(stats.upload_bytes))
                        .strong()
                        .color(Color32::LIGHT_BLUE)
                );
                ui.end_row();

                // 下载
                ui.label("⬇ 下载:");
                ui.label(
                    RichText::new(Statistics::format_bytes(stats.download_bytes))
                        .strong()
                        .color(Color32::LIGHT_GREEN)
                );
                ui.end_row();

                // 活跃连接
                ui.label("🔗 活跃连接:");
                ui.label(
                    RichText::new(format!("{}", stats.active_connections))
                        .strong()
                        .color(Color32::YELLOW)
                );
                ui.end_row();

                // 总连接数
                ui.label("📊 总连接数:");
                ui.label(
                    RichText::new(format!("{}", stats.total_connections))
                        .strong()
                );
                ui.end_row();
            });
    }
}
