use egui::Ui;
use crate::config::{NetworkBlockMethod, NetworkEntry, NetworkPreset, NetworkRule};
use crate::daemon::SharedState;
use crate::network;

pub struct NetworkTab {
    selected_id: Option<String>,
    editor:      Option<NetEditor>,
    status_msg:  Option<(String, bool)>, // (text, is_error)
}

impl NetworkTab {
    pub fn new() -> Self {
        Self { selected_id: None, editor: None, status_msg: None }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &SharedState) {
        // ── Toolbar ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            if ui.button("➕  Add Rule").clicked() {
                self.editor = Some(NetEditor::new());
            }

            let has_sel = self.selected_id.is_some();
            ui.add_enabled_ui(has_sel, |ui| {
                if ui.button("✏  Edit").clicked() {
                    if let Some(id) = &self.selected_id {
                        let s = state.read().unwrap();
                        if let Some(r) = s.config.network_rules.iter().find(|r| &r.id == id) {
                            self.editor = Some(NetEditor::from_rule(r));
                        }
                    }
                }

                if ui.button("🗑  Delete").clicked() {
                    if let Some(id) = self.selected_id.take() {
                        let rule = {
                            let s = state.read().unwrap();
                            s.config.network_rules.iter().find(|r| r.id == id).cloned()
                        };
                        if let Some(r) = rule {
                            if r.applied {
                                let _ = network::remove_network_rule(&r);
                            }
                        }
                        let mut s = state.write().unwrap();
                        s.config.network_rules.retain(|r| r.id != id);
                        s.save_config();
                    }
                }

                if ui.button("✅  Apply").clicked() {
                    if let Some(id) = &self.selected_id {
                        let rule = state.read().unwrap()
                            .config.network_rules.iter().find(|r| &r.id == id).cloned();
                        if let Some(rule) = rule {
                            match network::apply_network_rule(&rule) {
                                Ok(_) => {
                                    let mut s = state.write().unwrap();
                                    if let Some(r) = s.config.network_rules.iter_mut().find(|r| r.id == rule.id) {
                                        r.applied = true;
                                    }
                                    s.save_config();
                                    self.status_msg = Some(("Rule applied successfully.".into(), false));
                                }
                                Err(e) => {
                                    self.status_msg = Some((format!("Apply failed: {e}"), true));
                                }
                            }
                        }
                    }
                }

                if ui.button("⊘  Remove").clicked() {
                    if let Some(id) = &self.selected_id {
                        let rule = state.read().unwrap()
                            .config.network_rules.iter().find(|r| &r.id == id).cloned();
                        if let Some(rule) = rule {
                            match network::remove_network_rule(&rule) {
                                Ok(_) => {
                                    let mut s = state.write().unwrap();
                                    if let Some(r) = s.config.network_rules.iter_mut().find(|r| r.id == rule.id) {
                                        r.applied = false;
                                    }
                                    s.save_config();
                                    self.status_msg = Some(("Rule removed from system.".into(), false));
                                }
                                Err(e) => {
                                    self.status_msg = Some((format!("Remove failed: {e}"), true));
                                }
                            }
                        }
                    }
                }
            });
        });

        if let Some((msg, is_err)) = &self.status_msg {
            let color = if *is_err { egui::Color32::from_rgb(220, 80, 80) }
                        else       { egui::Color32::from_rgb(80, 200, 80) };
            ui.label(egui::RichText::new(msg).small().color(color));
        }

        ui.label(egui::RichText::new(
            "Network rules require root (via pkexec). Press Apply to push a rule to the system, Remove to undo it."
        ).small().weak());
        ui.separator();

        // ── Column headers ─────────────────────────────────────────────────
        egui::Grid::new("net_hdr").num_columns(5).min_col_width(60.0).show(ui, |ui| {
            ui.strong("On");
            ui.strong("Name");
            ui.strong("Method");
            ui.strong("Entries");
            ui.strong("Status");
            ui.end_row();
        });
        ui.separator();

        // ── Rule rows ──────────────────────────────────────────────────────
        let ids: Vec<String> = state.read().unwrap()
            .config.network_rules.iter().map(|r| r.id.clone()).collect();

        if ids.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(
                    "No network rules yet. Click ➕ Add Rule to create one."
                ).weak());
            });
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("net_grid")
                    .num_columns(5).min_col_width(60.0).striped(true)
                    .show(ui, |ui| {
                        for id in &ids {
                            self.show_row(ui, state, id);
                        }
                    });
            });
        }

        // ── Editor ─────────────────────────────────────────────────────────
        if let Some(editor) = &mut self.editor {
            if let Some(rule) = editor.show(ui.ctx()) {
                let mut s = state.write().unwrap();
                if editor.is_new {
                    s.config.network_rules.push(rule);
                } else {
                    if let Some(slot) = s.config.network_rules.iter_mut().find(|r| r.id == editor.id) {
                        *slot = rule;
                    }
                }
                s.save_config();
                self.status_msg = None;
            }
            if !editor.open {
                self.editor = None;
            }
        }
    }

    fn show_row(&mut self, ui: &mut Ui, state: &SharedState, id: &str) {
        let (name, method, entry_count, enabled, applied) = {
            let s = state.read().unwrap();
            let r = match s.config.network_rules.iter().find(|r| r.id == id) {
                Some(r) => r, None => { ui.end_row(); return; }
            };
            (r.name.clone(), format!("{}", r.method), r.entries.len(), r.enabled, r.applied)
        };

        let mut en = enabled;
        if ui.checkbox(&mut en, "").changed() {
            let mut s = state.write().unwrap();
            if let Some(r) = s.config.network_rules.iter_mut().find(|r| r.id == id) {
                r.enabled = en;
            }
            s.save_config();
        }

        let sel = self.selected_id.as_deref() == Some(id);
        if ui.selectable_label(sel, &name).clicked() {
            self.selected_id = Some(id.to_owned());
        }

        ui.label(egui::RichText::new(&method).small());
        ui.label(format!("{entry_count} entr{}", if entry_count == 1 { "y" } else { "ies" }));

        let (status_text, status_color) = if applied {
            ("● applied", egui::Color32::from_rgb(80, 200, 80))
        } else {
            ("○ not applied", egui::Color32::GRAY)
        };
        ui.label(egui::RichText::new(status_text).small().color(status_color));
        ui.end_row();
    }
}

// ── Network rule editor ───────────────────────────────────────────────────────

pub struct NetEditor {
    pub open:   bool,
    pub is_new: bool,
    pub id:     String,

    name:                String,
    enabled:             bool,
    method:              NetworkBlockMethod,
    entries:             Vec<NetworkEntry>,
    apply_to_apps:       Vec<String>,
    shutdown_on_connect: bool,

    new_entry:    String,
    new_app:      String,
    preset_combo: usize, // 0=none,1=nsfw,2=distracting,3=both
}

impl NetEditor {
    pub fn new() -> Self {
        Self {
            open:   true,
            is_new: true,
            id:     uuid::Uuid::new_v4().to_string(),
            name:   String::new(),
            enabled: true,
            method:  NetworkBlockMethod::default(),
            entries: Vec::new(),
            apply_to_apps: Vec::new(),
            shutdown_on_connect: false,
            new_entry:    String::new(),
            new_app:      String::new(),
            preset_combo: 0,
        }
    }

    pub fn from_rule(rule: &NetworkRule) -> Self {
        Self {
            open:   true,
            is_new: false,
            id:     rule.id.clone(),
            name:   rule.name.clone(),
            enabled: rule.enabled,
            method:  rule.method.clone(),
            entries: rule.entries.clone(),
            apply_to_apps: rule.apply_to_apps.clone(),
            shutdown_on_connect: rule.shutdown_on_connect,
            new_entry:    String::new(),
            new_app:      String::new(),
            preset_combo: 0,
        }
    }

    fn build_rule(&self) -> NetworkRule {
        NetworkRule {
            id:                  self.id.clone(),
            name:                self.name.clone(),
            enabled:             self.enabled,
            method:              self.method.clone(),
            entries:             self.entries.clone(),
            apply_to_apps:       self.apply_to_apps.clone(),
            shutdown_on_connect: self.shutdown_on_connect,
            applied:             false, // will be re-applied by user
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<NetworkRule> {
        let mut result  = None;
        let mut is_open = self.open;

        let title = if self.is_new { "Add Network Rule".into() }
                    else { format!("Edit Network Rule: {}", self.name) };

        egui::Window::new(title)
            .open(&mut is_open)
            .resizable(true)
            .min_width(480.0)
            .show(ctx, |ui| {
                result = self.body(ui);
            });

        if !is_open { self.open = false; }
        result
    }

    fn body(&mut self, ui: &mut egui::Ui) -> Option<NetworkRule> {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // ── Basic info ─────────────────────────────────────────────────
            egui::Grid::new("ne_basic").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Rule name:");
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();
            });

            ui.separator();

            // ── Method ─────────────────────────────────────────────────────
            ui.heading("Blocking Method");
            ui.radio_value(&mut self.method, NetworkBlockMethod::Nftables,
                "nftables — blocks by IP (domains are resolved at apply time)");
            ui.radio_value(&mut self.method, NetworkBlockMethod::Dns,
                "DNS — blocks via /etc/hosts (domain names only, system-wide)");

            if self.method == NetworkBlockMethod::Nftables {
                ui.label(egui::RichText::new(
                    "⚠ IP addresses can change for CDN-hosted domains. Re-apply periodically."
                ).small().color(egui::Color32::YELLOW));
            }

            ui.separator();

            // ── Presets ────────────────────────────────────────────────────
            ui.heading("Load Preset");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("preset_combo")
                    .selected_text(["None","Block NSFW","Block Distracting","Block Both"][self.preset_combo])
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.preset_combo, 0, "None");
                        ui.selectable_value(&mut self.preset_combo, 1, "Block NSFW Media");
                        ui.selectable_value(&mut self.preset_combo, 2, "Block Distracting Sites");
                        ui.selectable_value(&mut self.preset_combo, 3, "Block NSFW + Distracting");
                    });
                if ui.button("Load").clicked() && self.preset_combo > 0 {
                    let preset = match self.preset_combo {
                        1 => NetworkPreset::BlockNsfw,
                        2 => NetworkPreset::BlockDistracting,
                        _ => NetworkPreset::BlockBoth,
                    };
                    let new_entries = network::preset_entries(&preset);
                    for e in new_entries {
                        if !self.entries.iter().any(|x| x.value == e.value) {
                            self.entries.push(e);
                        }
                    }
                }
            });

            ui.separator();

            // ── Entries ────────────────────────────────────────────────────
            ui.heading("Domains / IPs");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_entry);
                if (ui.button("Add").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    && !self.new_entry.trim().is_empty()
                {
                    let val = self.new_entry.trim().to_owned();
                    if !self.entries.iter().any(|e| e.value == val) {
                        self.entries.push(NetworkEntry::new(val));
                    }
                    self.new_entry.clear();
                }
            });

            let mut remove_idx: Option<usize> = None;
            egui::ScrollArea::vertical().id_source("entries_scroll").max_height(200.0).show(ui, |ui| {
                egui::Grid::new("entries_grid").num_columns(3).spacing([8.0, 2.0]).striped(true).show(ui, |ui| {
                    for (i, entry) in self.entries.iter_mut().enumerate() {
                        ui.checkbox(&mut entry.enabled, "");
                        ui.label(egui::RichText::new(&entry.value).monospace().small());
                        if ui.small_button("✕").clicked() { remove_idx = Some(i); }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = remove_idx { self.entries.remove(i); }

            ui.separator();

            // ── Options ────────────────────────────────────────────────────
            ui.heading("Options");
            ui.checkbox(&mut self.shutdown_on_connect,
                "Drop connection silently (nftables: drop instead of reject)");
            if self.shutdown_on_connect {
                ui.label(egui::RichText::new(
                    "⚠ This silently drops packets. Not recommended — apps may hang waiting for timeout."
                ).small().color(egui::Color32::YELLOW));
            }

            ui.separator();

            // ── App scope ──────────────────────────────────────────────────
            ui.heading("App Scope (reference only)");
            ui.label(egui::RichText::new(
                "List apps this rule is intended for. Note: DNS/nftables rules are system-wide — \
                 this is for your reference only."
            ).small().weak());
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_app);
                if ui.button("Add").clicked() && !self.new_app.trim().is_empty() {
                    let a = self.new_app.trim().to_owned();
                    if !self.apply_to_apps.contains(&a) {
                        self.apply_to_apps.push(a);
                    }
                    self.new_app.clear();
                }
            });
            let mut rm_app: Option<usize> = None;
            for (i, app) in self.apply_to_apps.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(app).monospace().small());
                    if ui.small_button("✕").clicked() { rm_app = Some(i); }
                });
            }
            if let Some(i) = rm_app { self.apply_to_apps.remove(i); }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() { self.open = false; }
            let can_save = !self.name.is_empty();
            ui.add_enabled_ui(can_save, |ui| {
                if ui.button(egui::RichText::new("  Save  ").strong()).clicked() {
                    self.open = false;
                    return Some(self.build_rule());
                }
                None::<NetworkRule>
            }).inner
        }).inner
    }
}
