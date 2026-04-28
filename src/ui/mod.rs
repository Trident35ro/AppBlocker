mod rule_editor;
mod rules_tab;
mod monitor_tab;
mod settings_tab;

use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

use crate::daemon::SharedState;
use crate::tray::TrayFlags;
use rules_tab::RulesTab;
use monitor_tab::MonitorTab;
use settings_tab::SettingsTab;

#[derive(PartialEq)]
enum Tab { Rules, Monitor, Settings }

pub struct AppBlockerApp {
    state:       SharedState,
    tray_flags:  Arc<TrayFlags>,
    active_tab:  Tab,
    rules_tab:   RulesTab,
    monitor_tab: MonitorTab,
    tray_active: bool,
}

impl AppBlockerApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        state: SharedState,
        tray_flags: Arc<TrayFlags>,
        start_hidden: bool,
    ) -> Self {
        let tray_active = state.read().unwrap().config.show_tray_icon;

        let app = Self {
            state,
            tray_flags,
            active_tab:  Tab::Rules,
            rules_tab:   RulesTab::new(),
            monitor_tab: MonitorTab::new(),
            tray_active,
        };

        if start_hidden {
            // Handled in first update() frame via minimise.
        }

        app
    }
}

impl eframe::App for AppBlockerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Repaint every 5 s for the live monitor tab.
        ctx.request_repaint_after(Duration::from_secs(5));

        // Handle close → minimise (when tray is active).
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray_active {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        // Tray "Show" button → restore window.
        if self.tray_flags.show_window.load(Ordering::Relaxed) {
            self.tray_flags.show_window.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Quit flag (shouldn't normally be triggered since we call exit(0) in tray).
        if self.tray_flags.quit.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // ── Top bar ────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("AppBlocker");
                ui.separator();
                ui.selectable_value(&mut self.active_tab, Tab::Rules,    "📋 Rules");
                ui.selectable_value(&mut self.active_tab, Tab::Monitor,  "📊 Monitor");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ Settings");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let running = self.state.read().unwrap().daemon_running;
                    let dot = if running { "🟢" } else { "🔴" };
                    ui.label(format!("{dot} daemon"));
                });
            });
        });

        // ── Main panel ─────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Rules   => self.rules_tab.show(ui, &self.state),
                Tab::Monitor => self.monitor_tab.show(ui, &self.state),
                Tab::Settings => SettingsTab::show(ui, &self.state),
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Drop session-only rules before saving.
        {
            let mut s = self.state.write().unwrap();
            s.config.rules.retain(|r| r.persist_across_reboots);
            s.save_config();
        }
    }
}
