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
                s.daemon_running       = enabled;
                s.config.daemon_enabled = enabled;
                s.save_config();
            }
            ui.horizontal(|ui| {
                ui.label("Check interval:");
                if ui.add(egui::DragValue::new(&mut interval).range(1..=60).suffix(" s")).changed() {
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
                } else if ui.button("Remove").clicked() {
                    if let Err(e) = startup::remove_daemon_service() {
                        log::error!("service remove: {e}");
                    }
                }
            });

            ui.separator();

            // ── Tray icon ─────────────────────────────────────────────────
            ui.heading("System Tray");
            let mut show_tray = state.read().unwrap().config.show_tray_icon;
            if ui.checkbox(&mut show_tray, "Show tray icon (takes effect on next launch)").changed() {
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

            // ── Usage tracking ────────────────────────────────────────────
            ui.heading("Usage Tracking");
            {
                let days_opt = state.read().unwrap().config.usage_retention_days;
                let mut keep_forever = days_opt.is_none();

                if ui.checkbox(&mut keep_forever, "Keep usage data forever (no auto-delete)").changed() {
                    let mut s = state.write().unwrap();
                    s.config.usage_retention_days = if keep_forever { None } else { Some(90) };
                    s.save_config();
                }

                if !keep_forever {
                    let mut days = days_opt.unwrap_or(90);
                    ui.horizontal(|ui| {
                        ui.label("Delete records older than:");
                        if ui.add(egui::DragValue::new(&mut days).range(7..=3650).suffix(" days")).changed() {
                            let mut s = state.write().unwrap();
                            s.config.usage_retention_days = Some(days);
                            s.save_config();
                        }
                    });
                }
            }

            ui.separator();

            // ── Import / Export ───────────────────────────────────────────
            ui.heading("Import / Export Rules");
            ui.label(egui::RichText::new(
                "Export your rules as a TOML file to back them up or share with others."
            ).small().weak());
            ui.horizontal(|ui| {
                if ui.button("Export Rules…").clicked() {
                    let rules = state.read().unwrap().config.rules.clone();
                    if let Some(path) = kdialog_save("rules.toml", "*.toml") {
                        match toml::to_string_pretty(&rules) {
                            Ok(text) => {
                                if let Err(e) = std::fs::write(&path, text) {
                                    log::error!("export failed: {e}");
                                } else {
                                    log::info!("rules exported to {}", path.display());
                                }
                            }
                            Err(e) => log::error!("export serialise: {e}"),
                        }
                    }
                }

                if ui.button("Import Rules…").clicked() {
                    if let Some(path) = kdialog_open("*.toml") {
                        match std::fs::read_to_string(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|t| toml::from_str::<Vec<crate::config::Rule>>(&t)
                                .map_err(|e| e.to_string()))
                        {
                            Ok(imported) => {
                                let mut s = state.write().unwrap();
                                let existing_ids: std::collections::HashSet<_> =
                                    s.config.rules.iter().map(|r| r.id.clone()).collect();
                                let added = imported.iter()
                                    .filter(|r| !existing_ids.contains(&r.id))
                                    .count();
                                for rule in imported {
                                    if !existing_ids.contains(&rule.id) {
                                        s.config.rules.push(rule);
                                    }
                                }
                                s.save_config();
                                log::info!("imported {added} rule(s)");
                            }
                            Err(e) => log::error!("import failed: {e}"),
                        }
                    }
                }
            });

            ui.separator();

            // ── Mindful log ───────────────────────────────────────────────
            ui.heading("Mindful Log");
            let log_path = crate::daemon::mindful_log_path();
            ui.label(egui::RichText::new(
                "When Mindful mode rules prompt you for a reason, the response is logged here:"
            ).small().weak());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(log_path.display().to_string()).monospace().small());
                if ui.small_button("Clear").clicked() {
                    let _ = std::fs::remove_file(&log_path);
                }
            });

            ui.separator();

            // ── Blocking method notes ─────────────────────────────────────
            ui.heading("Blocking Methods — Notes");
            egui::Grid::new("notes_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.strong("Kill Process");
                ui.label("Terminates the process whenever detected. Reliable.");
                ui.end_row();
                ui.strong("PATH Wrapper");
                ui.label("Installs ~/.local/bin/<name> script. Blocks/intercepts at launch.");
                ui.end_row();
                ui.strong("Network Block");
                ui.label("Adds an nftables OUTPUT rule for your UID via pkexec.");
                ui.end_row();
            });

            ui.separator();

            ui.heading("Config File");
            ui.label(egui::RichText::new(
                crate::config::AppConfig::config_path().display().to_string()
            ).monospace().small());
        });
    }
}

fn kdialog_open(filter: &str) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("kdialog")
        .args(["--getopenfilename", ".", filter])
        .output().ok()?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() { Some(std::path::PathBuf::from(p)) } else { None }
    } else { None }
}

fn kdialog_save(default_name: &str, filter: &str) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("kdialog")
        .args(["--getsavefilename", default_name, filter])
        .output().ok()?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() { Some(std::path::PathBuf::from(p)) } else { None }
    } else { None }
}
