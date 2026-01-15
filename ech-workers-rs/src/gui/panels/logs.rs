//! 日志面板

use egui::{RichText, Color32};
use crate::gui::state::{AppState, LogLevel};

pub struct LogsPanel {
    filter_level: LogLevel,
    search_text: String,
    auto_scroll: bool,
}

impl Default for LogsPanel {
    fn default() -> Self {
        Self {
            filter_level: LogLevel::Info,
            search_text: String::new(),
            auto_scroll: true,
        }
    }
}

impl LogsPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        ui.heading("📝 日志");
        ui.add_space(5.0);

        // 工具栏
        ui.horizontal(|ui| {
            // 日志级别过滤
            ui.label("级别:");
            egui::ComboBox::from_id_source("log_level_filter")
                .selected_text(self.filter_level.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter_level, LogLevel::Trace, "TRACE");
                    ui.selectable_value(&mut self.filter_level, LogLevel::Debug, "DEBUG");
                    ui.selectable_value(&mut self.filter_level, LogLevel::Info, "INFO");
                    ui.selectable_value(&mut self.filter_level, LogLevel::Warn, "WARN");
                    ui.selectable_value(&mut self.filter_level, LogLevel::Error, "ERROR");
                });

            ui.separator();

            // 搜索框
            ui.label("🔍");
            ui.add(
                egui::TextEdit::singleline(&mut self.search_text)
                    .hint_text("搜索日志...")
                    .desired_width(200.0)
            );

            ui.separator();

            // 自动滚动
            ui.checkbox(&mut self.auto_scroll, "自动滚动");

            ui.separator();

            // 清空日志
            if ui.button("🗑 清空").clicked() {
                state.clear_logs();
            }
        });

        ui.add_space(5.0);

        // 日志列表
        let text_height = egui::TextStyle::Body.resolve(ui.style()).size;
        
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(self.auto_scroll)
            .show_rows(
                ui,
                text_height,
                state.logs.len(),
                |ui, row_range| {
                    for i in row_range {
                        if let Some(entry) = state.logs.get(i) {
                            // 级别过滤
                            if (entry.level as u8) < (self.filter_level as u8) {
                                continue;
                            }

                            // 搜索过滤
                            if !self.search_text.is_empty() 
                                && !entry.message.to_lowercase().contains(&self.search_text.to_lowercase()) {
                                continue;
                            }

                            // 显示日志条目
                            ui.horizontal(|ui| {
                                // 时间戳
                                ui.label(
                                    RichText::new(entry.timestamp.format("%H:%M:%S").to_string())
                                        .color(Color32::DARK_GRAY)
                                        .monospace()
                                );

                                // 级别
                                ui.label(
                                    RichText::new(format!("[{}]", entry.level.to_string()))
                                        .color(entry.level.color())
                                        .strong()
                                        .monospace()
                                );

                                // 消息
                                ui.label(RichText::new(&entry.message).monospace());
                            });
                        }
                    }
                },
            );
    }
}
