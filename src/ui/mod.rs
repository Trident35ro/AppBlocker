mod rule_editor;
mod rules_tab;
mod monitor_tab;
mod settings_tab;
mod usage_tab;
mod network_tab;
mod sessions_tab;

use sessions_tab::SessionsTab;
use std::sync::{atomic::Ordering, Arc};
use std::time::Duration;

use crate::daemon::SharedState;
use crate::tray::TrayFlags;
use rules_tab::RulesTab;
use monitor_tab::MonitorTab;
use settings_tab::SettingsTab;
use usage_tab::UsageTab;
use network_tab::NetworkTab;

#[derive(PartialEq)]
enum Tab { Rules, Monitor, Usage, Network, Settings, Sessions }

pub struct AppBlockerApp {
    state:        SharedState,
    tray_flags:   Arc<TrayFlags>,
    active_tab:   Tab,
    rules_tab:    RulesTab,
    monitor_tab:  MonitorTab,
    usage_tab:    UsageTab,
    network_tab:  NetworkTab,
    sessions_tab: SessionsTab,
    tray_active:  bool,
}

impl AppBlockerApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        state: SharedState,
        tray_flags: Arc<TrayFlags>,
        _start_hidden: bool,
    ) -> Self {
        let tray_active = state.read().unwrap().config.show_tray_icon;
        Self {
            state,
            tray_flags,
            active_tab:   Tab::Rules,
            rules_tab:    RulesTab::new(),
            monitor_tab:  MonitorTab::new(),
            usage_tab:    UsageTab::new(),
            network_tab:  NetworkTab::new(),
            sessions_tab: SessionsTab::new(),
            tray_active,
        }
    }
}

impl eframe::App for AppBlockerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let repaint_interval = {
            let s = self.state.read().unwrap();
            if s.active_session.is_some() {
                Duration::from_secs(1)   // fast refresh when session is active
            } else {
                Duration::from_secs(5)   // normal refresh otherwise
            }
        };
        ctx.request_repaint_after(repaint_interval);

        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray_active {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            }
        }

        if self.tray_flags.show_window.load(Ordering::Relaxed) {
            self.tray_flags.show_window.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        if self.tray_flags.quit.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::TopBottomPanel::top("topbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("AppBlocker");
                ui.separator();
                ui.selectable_value(&mut self.active_tab, Tab::Rules,    "📋 Rules");
                ui.selectable_value(&mut self.active_tab, Tab::Monitor,  "📊 Monitor");
                ui.selectable_value(&mut self.active_tab, Tab::Usage,    "📈 Usage");
                ui.selectable_value(&mut self.active_tab, Tab::Network,  "🌐 Network");
                ui.selectable_value(&mut self.active_tab, Tab::Sessions, "🎯 Sessions");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ Settings");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let running = self.state.read().unwrap().daemon_running;
                    let dot = if running { "🟢" } else { "🔴" };
                    ui.label(format!("{dot} daemon"));
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Rules    => self.rules_tab.show(ui, &self.state),
                Tab::Monitor  => self.monitor_tab.show(ui, &self.state),
                Tab::Usage    => self.usage_tab.show(ui, &self.state),
                Tab::Network  => self.network_tab.show(ui, &self.state),
                Tab::Sessions => self.sessions_tab.show(ui, &self.state),
                Tab::Settings => SettingsTab::show(ui, &self.state),
            }
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let mut s = self.state.write().unwrap();
        s.config.rules.retain(|r| r.persist_across_reboots);
        s.save_config();
    }
}
