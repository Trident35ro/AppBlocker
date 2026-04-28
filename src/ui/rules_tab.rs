use egui::Ui;
use crate::config::BlockingMethod;
use crate::daemon::SharedState;
use crate::startup;
use super::rule_editor::RuleEditor;

/// Simple name match shared between "Block Now" and the daemon.
fn exe_matches(proc_name: &str, rule_exe: &str) -> bool {
    let base = std::path::Path::new(rule_exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(rule_exe);
    if rule_exe.contains('/') {
        proc_name == base
    } else {
        proc_name.to_lowercase() == base.to_lowercase()
    }
}

pub struct RulesTab {
    pub editor:      Option<RuleEditor>,
    pub selected_id: Option<String>,
}

impl RulesTab {
    pub fn new() -> Self {
        Self { editor: None, selected_id: None }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &SharedState) {
        // ── Toolbar ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            if ui.button("➕  Add Rule").clicked() {
                self.editor = Some(RuleEditor::new_rule());
            }

            let has_sel = self.selected_id.is_some();
            ui.add_enabled_ui(has_sel, |ui| {
                if ui.button("✏  Edit").clicked() {
                    if let Some(id) = &self.selected_id {
                        let s = state.read().unwrap();
                        if let Some(r) = s.config.rules.iter().find(|r| &r.id == id) {
                            self.editor = Some(RuleEditor::from_rule(r));
                        }
                    }
                }
                if ui.button("🗑  Delete").clicked() {
                    if let Some(id) = self.selected_id.take() {
                        // Collect cleanup data before mutating.
                        let cleanup = {
                            let s = state.read().unwrap();
                            s.config.rules.iter().find(|r| r.id == id).map(|r| {
                                (r.executable.clone(), r.blocking_method.clone(), r.name.clone())
                            })
                        };
                        if let Some((exe, method, name)) = cleanup {
                            let _ = startup::remove_app_autostart(&name);
                            match method {
                                BlockingMethod::Wrapper   => { let _ = crate::blocker::remove_wrapper(&exe); }
                                BlockingMethod::Network   => { let _ = crate::blocker::remove_network_block(&exe); }
                                _ => {}
                            }
                        }
                        let mut s = state.write().unwrap();
                        s.config.rules.retain(|r| r.id != id);
                        s.save_config();
                    }
                }
            });
        });

        ui.separator();

        // ── Column headers ─────────────────────────────────────────────────
        egui::Grid::new("rules_header")
            .num_columns(6)
            .min_col_width(60.0)
            .striped(false)
            .show(ui, |ui| {
                ui.strong("On");
                ui.strong("Name");
                ui.strong("Executable");
                ui.strong("Method");
                ui.strong("Schedule");
                ui.strong("Actions");
                ui.end_row();
            });

        ui.separator();

        // ── Rule rows ──────────────────────────────────────────────────────
        let rule_ids: Vec<String> = state.read().unwrap()
            .config.rules.iter().map(|r| r.id.clone()).collect();

        if rule_ids.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("No rules yet. Click ➕ Add Rule to create one.")
                    .weak());
            });
            self.show_editor(ui, state);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("rules_grid")
                .num_columns(6)
                .min_col_width(60.0)
                .striped(true)
                .show(ui, |ui| {
                    for id in rule_ids {
                        self.show_row(ui, state, &id);
                    }
                });
        });

        self.show_editor(ui, state);
    }

    fn show_row(&mut self, ui: &mut Ui, state: &SharedState, id: &str) {
        let (name, exe, method, sched, enabled) = {
            let s = state.read().unwrap();
            let r = match s.config.rules.iter().find(|r| r.id == id) {
                Some(r) => r,
                None    => { ui.end_row(); return; }
            };
            (r.name.clone(), r.exe_name().to_owned(), format!("{}", r.blocking_method),
             format!("{}", r.schedule), r.enabled)
        };

        // Enabled toggle
        let mut en = enabled;
        if ui.checkbox(&mut en, "").changed() {
            let mut s = state.write().unwrap();
            if let Some(r) = s.config.rules.iter_mut().find(|r| r.id == id) {
                r.enabled = en;
            }
            s.save_config();
        }

        // Selectable name
        let selected = self.selected_id.as_deref() == Some(id);
        if ui.selectable_label(selected, &name).clicked() {
            self.selected_id = Some(id.to_owned());
        }

        ui.label(egui::RichText::new(truncate(&exe, 28)).monospace().small());
        ui.label(&method);
        ui.label(&sched);

        // Quick-action buttons
        ui.horizontal(|ui| {
            if ui.small_button("Block Now").clicked() {
                let id = id.to_owned();
                let mut s = state.write().unwrap();

                // Enable rule and wipe any pending grace window.
                if let Some(r) = s.config.rules.iter_mut().find(|r| r.id == id) {
                    r.enabled = true;
                }
                s.grace_timers.remove(&id);
                s.grace_warned.remove(&id);

                // Collect matching PIDs and method from current snapshot.
                let snap = s.config.rules.iter().find(|r| r.id == id)
                    .map(|r| (r.executable.clone(), r.blocking_method.clone()));
                let pids: Vec<i32> = snap.as_ref().map(|(exe, _)| {
                    s.processes.iter()
                        .filter(|p| exe_matches(&p.name, exe))
                        .map(|p| p.pid)
                        .collect()
                }).unwrap_or_default();

                drop(s); // release lock before syscalls

                if let Some((exe, method)) = snap {
                    for pid in &pids {
                        let result = match &method {
                            BlockingMethod::ForceKill => crate::blocker::force_kill_process(*pid),
                            _                        => crate::blocker::kill_process(*pid),
                        };
                        match result {
                            Ok(_)  => log::info!("Block Now: killed PID {pid}"),
                            Err(e) => log::warn!("Block Now: kill PID {pid} failed: {e}"),
                        }
                    }
                    if matches!(method, BlockingMethod::Network) {
                        let _ = crate::blocker::install_network_block(&exe);
                    }
                    if pids.is_empty() {
                        log::info!("Block Now: no matching processes found yet — daemon will block on next scan");
                    }
                }

                state.write().unwrap().save_config();
            }
            if ui.small_button("Rest of Day").clicked() {
                let mut s = state.write().unwrap();
                if let Some(r) = s.config.rules.iter_mut().find(|r| r.id == id) {
                    r.block_rest_of_day();
                }
                s.save_config();
            }
        });

        ui.end_row();
    }

    fn show_editor(&mut self, ui: &mut Ui, state: &SharedState) {
        if let Some(editor) = &mut self.editor {
            if let Some(rule) = editor.show(ui.ctx()) {
                // Persist startup action side-effects.
                match &rule.startup_action {
                    crate::config::StartupAction::LaunchOnStartup => {
                        let _ = startup::install_app_autostart(&rule.name, &rule.executable);
                    }
                    crate::config::StartupAction::None => {
                        let _ = startup::remove_app_autostart(&rule.name);
                    }
                    _ => {}
                }

                // Install / remove wrapper script.
                match &rule.blocking_method {
                    crate::config::BlockingMethod::Wrapper => {
                        if let Err(e) = crate::blocker::install_wrapper(&rule.executable) {
                            log::error!("wrapper install failed: {e}");
                        }
                    }
                    _ => {}
                }

                let mut s = state.write().unwrap();
                if editor.is_new {
                    s.config.rules.push(rule);
                } else {
                    if let Some(slot) = s.config.rules.iter_mut()
                        .find(|r| r.id == editor.build_rule().id)
                    {
                        *slot = rule;
                    }
                }
                s.save_config();
            }

            if !editor.open {
                self.editor = None;
            }
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_owned() }
    else { format!("…{}", &s[s.len().saturating_sub(max - 1)..]) }
}
