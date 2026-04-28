use egui::Ui;
use crate::daemon::SharedState;
use crate::startup;

pub struct SettingsTab;

impl SettingsTab {
    pub fn show(ui: &mut Ui, state: &SharedState) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // ── Daemon ────────────────────────────────────────────────────
            ui.heading("Daemon");

            let (mut enabled, mut interval) = {
                let s = state.read().unwrap();
                (s.daemon_running, s.config.check_interval_secs)
            };

            if ui.checkbox(&mut enabled, "Enforcement daemon active").changed() {
                let mut s = state.write().unwrap();
                s.daemon_running      = enabled;
                s.config.daemon_enabled = enabled;
                s.save_config();
            }

            ui.horizontal(|ui| {
                ui.label("Check interval:");
                if ui.add(
                    egui::DragValue::new(&mut interval).range(1..=60).suffix(" s")
                ).changed() {
                    let mut s = state.write().unwrap();
                    s.config.check_interval_secs = interval;
                    s.save_config();
                }
            });

            ui.separator();

            // ── Systemd service ───────────────────────────────────────────
            ui.heading("Systemd User Service");
            ui.label(egui::RichText::new(
                "Install a systemd user service so the daemon starts automatically on login."
            ).small().weak());

            let installed = startup::is_service_installed();
            let running   = startup::is_service_running();

            ui.horizontal(|ui| {
                let status = if running { "● running" }
                             else if installed { "○ stopped" }
                             else { "not installed" };
                ui.label(format!("Service status: {status}"));
            });

            ui.horizontal(|ui| {
                if !installed {
                    if ui.button("Install & Enable").clicked() {
                        if let Err(e) = startup::install_daemon_service() {
                            log::error!("service install: {e}");
                        }
                    }
                } else {
                    if ui.button("Remove").clicked() {
                        if let Err(e) = startup::remove_daemon_service() {
                            log::error!("service remove: {e}");
                        }
                    }
                }
            });

            ui.separator();

            // ── Tray icon ─────────────────────────────────────────────────
            ui.heading("System Tray");

            let mut show_tray = state.read().unwrap().config.show_tray_icon;
            if ui.checkbox(&mut show_tray, "Show tray icon (takes effect on next launch)")
                .changed()
            {
                let mut s = state.write().unwrap();
                s.config.show_tray_icon = show_tray;
                s.save_config();
            }

            let mut start_min = state.read().unwrap().config.start_minimized;
            if ui.checkbox(&mut start_min, "Start minimised to tray").changed() {
                let mut s = state.write().unwrap();
                s.config.start_minimized = start_min;
                s.save_config();
            }

            ui.separator();

            // ── Blocking notes ────────────────────────────────────────────
            ui.heading("Blocking Methods — Notes");
            egui::Grid::new("notes_grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.strong("Kill Process");
                    ui.label("Terminates the process whenever it is detected. \
                              Reliable; may need the daemon running continuously.");
                    ui.end_row();

                    ui.strong("PATH Wrapper");
                    ui.label("Installs a script at ~/.local/bin/<name> that \
                              blocks execution before the app starts. Works when \
                              launched from PATH-aware contexts (terminal, KRunner).");
                    ui.end_row();

                    ui.strong("Network Block");
                    ui.label("Adds an nftables OUTPUT rule for your UID via pkexec. \
                              Blocks all outbound traffic for your user while active. \
                              You will be prompted for root when enabling this rule.");
                    ui.end_row();
                });

            ui.separator();

            // ── Config path ───────────────────────────────────────────────
            ui.heading("Config File");
            let path = crate::config::AppConfig::config_path();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(path.display().to_string()).monospace().small());
            });
        });
    }
}
