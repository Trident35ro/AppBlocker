use egui::Ui;
use crate::config::{BreakConfig, ScheduledBlock, SessionMode, SessionTemplate};
use crate::daemon::{self, CancelResult, SharedState};

// ── Tab state ─────────────────────────────────────────────────────────────────

pub struct SessionsTab {
    // Template editor
    editor:          Option<TemplateEditor>,
    selected_tmpl:   Option<String>,

    // Scheduled blocks editor
    sched_editor:    Option<SchedEditor>,
    selected_sched:  Option<String>,

    // Cancel UI state
    cancel_password: String,
    cancel_msg:      Option<String>,
    last_cancel_res: Option<CancelResult>,

    // Quick-launch: custom one-off session
    quick_hours:     u64,
    quick_mins:      u64,
}

impl SessionsTab {
    pub fn new() -> Self {
        Self {
            editor:          None,
            selected_tmpl:   None,
            sched_editor:    None,
            selected_sched:  None,
            cancel_password: String::new(),
            cancel_msg:      None,
            last_cancel_res: None,
            quick_hours:     1,
            quick_mins:      0,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &SharedState) {
        // ── Active session banner (always at the top) ──────────────────────
        self.show_active_banner(ui, state);
        ui.separator();

        // ── Two-pane layout: templates left, scheduled blocks right ────────
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |cols| {
                self.show_templates_panel(&mut cols[0], state);
                self.show_scheduled_panel(&mut cols[1], state);
            });
        });

        // ── Editors (shown as floating windows) ───────────────────────────
        if let Some(ed) = &mut self.editor {
            if let Some(tmpl) = ed.show(ui.ctx(), state) {
                let mut s = state.write().unwrap();
                if ed.is_new {
                    s.config.session_templates.push(tmpl);
                } else {
                    if let Some(slot) = s.config.session_templates.iter_mut()
                        .find(|t| t.id == ed.id) { *slot = tmpl; }
                }
                s.save_config();
            }
            if !ed.open { self.editor = None; }
        }

        if let Some(ed) = &mut self.sched_editor {
            if let Some(sb) = ed.show(ui.ctx(), state) {
                let mut s = state.write().unwrap();
                if ed.is_new {
                    s.config.scheduled_blocks.push(sb);
                } else {
                    if let Some(slot) = s.config.scheduled_blocks.iter_mut()
                        .find(|b| b.id == ed.id) { *slot = sb; }
                }
                s.save_config();
            }
            if !ed.open { self.sched_editor = None; }
        }
    }

    // ── Active session banner ─────────────────────────────────────────────────

    fn show_active_banner(&mut self, ui: &mut Ui, state: &SharedState) {
        let (has_session, name, remaining, on_break, break_remaining,
             next_break_in, is_strict, cancel_pending_secs) = {
            let s = state.read().unwrap();
            match &s.active_session {
                None => {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("No active focus session.").weak());
                    });
                    return;
                }
                Some(sess) => {
                    let pending = if let SessionMode::FlexibleDelay { delay_secs } = &sess.mode {
                        sess.cancel_requested_at.map(|at| {
                            let elapsed = at.elapsed().as_secs();
                            delay_secs.saturating_sub(elapsed)
                        })
                    } else { None };
                    (
                        true,
                        sess.template_name.clone(),
                        sess.remaining(),
                        sess.on_break(),
                        sess.break_remaining(),
                        sess.next_break_in(),
                        matches!(sess.mode, SessionMode::Strict),
                        pending,
                    )
                }
            }
        };

        if !has_session { return; }

        let frame_color = if on_break {
            egui::Color32::from_rgb(40, 80, 40)
        } else {
            egui::Color32::from_rgb(30, 50, 80)
        };

        egui::Frame::none()
            .fill(frame_color)
            .rounding(6.0)
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Left: session info
                    ui.vertical(|ui| {
                        if on_break {
                            ui.label(egui::RichText::new(
                                format!("☕ BREAK — {}", &name)
                            ).strong().size(15.0).color(egui::Color32::from_rgb(120, 220, 120)));
                            if let Some(br) = break_remaining {
                                ui.label(format!("Break ends in  {}", fmt_dur(br.as_secs())));
                            }
                        } else {
                            ui.label(egui::RichText::new(
                                format!("🎯 FOCUS — {}", &name)
                            ).strong().size(15.0).color(egui::Color32::from_rgb(100, 180, 255)));
                            ui.label(format!("Time remaining:  {}", fmt_dur(remaining.as_secs())));
                            if let Some(nb) = next_break_in {
                                ui.label(egui::RichText::new(
                                    format!("Next break in:  {}", fmt_dur(nb.as_secs()))
                                ).small().weak());
                            }
                        }
                        if is_strict {
                            ui.label(egui::RichText::new("🔒 Strict mode — cannot be cancelled")
                                .small().color(egui::Color32::from_rgb(255, 140, 60)));
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !is_strict {
                            self.show_cancel_controls(ui, state, cancel_pending_secs);
                        }
                    });
                });
            });

        if let Some(msg) = &self.cancel_msg {
            let col = if self.last_cancel_res == Some(CancelResult::WrongPassword) {
                egui::Color32::from_rgb(220, 80, 80)
            } else {
                egui::Color32::from_rgb(220, 180, 80)
            };
            ui.label(egui::RichText::new(msg).small().color(col));
        }
    }

    fn show_cancel_controls(
        &mut self,
        ui: &mut Ui,
        state: &SharedState,
        pending_secs: Option<u64>,
    ) {
        let needs_password = {
            let s = state.read().unwrap();
            s.active_session.as_ref().map(|sess|
                matches!(sess.mode, SessionMode::FlexiblePassword { .. })
            ).unwrap_or(false)
        };

        if needs_password {
            ui.add(egui::TextEdit::singleline(&mut self.cancel_password)
                .password(true)
                .hint_text("enter password to cancel")
                .desired_width(180.0));
            if ui.button("Cancel Session").clicked() {
                let pwd = self.cancel_password.clone();
                let res = daemon::request_cancel(state, Some(&pwd));
                self.handle_cancel_result(res);
                self.cancel_password.clear();
            }
        } else if let Some(secs) = pending_secs {
            ui.label(egui::RichText::new(
                format!("Cancelling in {}s…", secs)
            ).color(egui::Color32::from_rgb(220, 180, 80)));
            if ui.button("Abort Cancel").clicked() {
                // Remove the cancel request
                let mut s = state.write().unwrap();
                if let Some(sess) = &mut s.active_session {
                    sess.cancel_requested_at = None;
                }
                self.cancel_msg = Some("Cancel request withdrawn.".into());
                self.last_cancel_res = None;
            }
        } else {
            if ui.button("Cancel Session").clicked() {
                let res = daemon::request_cancel(state, None);
                self.handle_cancel_result(res);
            }
        }
    }

    fn handle_cancel_result(&mut self, res: CancelResult) {
        self.cancel_msg = Some(match &res {
            CancelResult::Cancelled      => "Session cancelled.".into(),
            CancelResult::Denied         => "Strict mode — cannot cancel.".into(),
            CancelResult::WrongPassword  => "Wrong password.".into(),
            CancelResult::NoSession      => "No active session.".into(),
            CancelResult::PendingDelay { remaining_secs } =>
                format!("Cancel request received. Session will end in {}s.", remaining_secs),
        });
        self.last_cancel_res = Some(res);
    }

    // ── Templates panel ───────────────────────────────────────────────────────

    fn show_templates_panel(&mut self, ui: &mut Ui, state: &SharedState) {
        ui.heading("Session Templates");
        ui.label(egui::RichText::new(
            "Templates define which apps to block, for how long, and with what rules."
        ).small().weak());
        ui.add_space(4.0);

        // Toolbar
        ui.horizontal(|ui| {
            if ui.button("➕ New").clicked() {
                self.editor = Some(TemplateEditor::new());
            }
            let has = self.selected_tmpl.is_some();
            ui.add_enabled_ui(has, |ui| {
                if ui.button("✏ Edit").clicked() {
                    if let Some(id) = &self.selected_tmpl {
                        let s = state.read().unwrap();
                        if let Some(t) = s.config.session_templates.iter().find(|t| &t.id == id) {
                            self.editor = Some(TemplateEditor::from_template(t));
                        }
                    }
                }
                if ui.button("🗑 Delete").clicked() {
                    if let Some(id) = self.selected_tmpl.take() {
                        let mut s = state.write().unwrap();
                        s.config.session_templates.retain(|t| t.id != id);
                        s.save_config();
                    }
                }
                if ui.button("▶ Start").clicked() {
                    if let Some(id) = &self.selected_tmpl {
                        let tmpl = state.read().unwrap()
                            .config.session_templates.iter()
                            .find(|t| &t.id == id).cloned();
                        if let Some(t) = tmpl {
                            daemon::start_session(state, &t);
                        }
                    }
                }
            });
        });

        ui.separator();

        let tmpls: Vec<(String, String, u64, String)> = state.read().unwrap()
            .config.session_templates.iter()
            .map(|t| (
                t.id.clone(),
                t.name.clone(),
                t.duration_secs,
                mode_label(&t.mode),
            )).collect();

        if tmpls.is_empty() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("No templates yet. Click ➕ New to create one.").weak());
        } else {
            egui::ScrollArea::vertical().id_source("tmpl_scroll").max_height(260.0).show(ui, |ui| {
                for (id, name, dur, mode) in &tmpls {
                    let sel = self.selected_tmpl.as_deref() == Some(id);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(sel, name).clicked() {
                            self.selected_tmpl = Some(id.clone());
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(mode).small().weak());
                            ui.label(egui::RichText::new(fmt_dur(*dur)).small());
                        });
                    });
                }
            });
        }

        // Quick-launch: one-off session using all current rules
        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new("Quick session (blocks all enabled rules)").small().strong());
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.quick_hours).range(0..=23).suffix("h"));
            ui.add(egui::DragValue::new(&mut self.quick_mins).range(0..=59).suffix("m"));
            if ui.button("▶ Start Quick Session").clicked() {
                let secs = self.quick_hours * 3600 + self.quick_mins * 60;
                if secs > 0 {
                    let rule_ids: Vec<String> = state.read().unwrap()
                        .config.rules.iter()
                        .filter(|r| r.enabled)
                        .map(|r| r.id.clone())
                        .collect();
                    let tmpl = SessionTemplate {
                        id:            uuid::Uuid::new_v4().to_string(),
                        name:          "Quick Session".into(),
                        rule_ids,
                        duration_secs: secs,
                        mode:          SessionMode::FlexibleDelay { delay_secs: 30 },
                        break_config:  None,
                    };
                    daemon::start_session(state, &tmpl);
                }
            }
        });
    }

    // ── Scheduled blocks panel ────────────────────────────────────────────────

    fn show_scheduled_panel(&mut self, ui: &mut Ui, state: &SharedState) {
        ui.heading("Scheduled Blocks");
        ui.label(egui::RichText::new(
            "Automatically start a session template at a given time on selected days."
        ).small().weak());
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button("➕ New").clicked() {
                // Default to first template if one exists
                let first_id = state.read().unwrap()
                    .config.session_templates.first()
                    .map(|t| t.id.clone())
                    .unwrap_or_default();
                self.sched_editor = Some(SchedEditor::new(first_id));
            }
            let has = self.selected_sched.is_some();
            ui.add_enabled_ui(has, |ui| {
                if ui.button("✏ Edit").clicked() {
                    if let Some(id) = &self.selected_sched {
                        let s = state.read().unwrap();
                        if let Some(sb) = s.config.scheduled_blocks.iter().find(|b| &b.id == id) {
                            self.sched_editor = Some(SchedEditor::from_block(sb));
                        }
                    }
                }
                if ui.button("🗑 Delete").clicked() {
                    if let Some(id) = self.selected_sched.take() {
                        let mut s = state.write().unwrap();
                        s.config.scheduled_blocks.retain(|b| b.id != id);
                        s.save_config();
                    }
                }
            });
        });

        ui.separator();

        let blocks: Vec<(String, bool, String, String, u8, u8)> = {
            let s = state.read().unwrap();
            s.config.scheduled_blocks.iter().map(|sb| {
                let tmpl_name = s.config.session_templates.iter()
                    .find(|t| t.id == sb.template_id)
                    .map(|t| t.name.clone())
                    .unwrap_or_else(|| "Unknown".into());
                let days: String = sb.days.iter()
                    .map(|&d| crate::config::UnavailPeriod::day_name(d))
                    .collect::<Vec<_>>().join(" ");
                (sb.id.clone(), sb.enabled, tmpl_name, days, sb.start_hour, sb.start_min)
            }).collect()
        };

        if blocks.is_empty() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("No scheduled blocks yet.").weak());
        } else {
            egui::ScrollArea::vertical().id_source("sched_scroll").max_height(260.0).show(ui, |ui| {
                egui::Grid::new("sched_grid").num_columns(4).spacing([8.0, 4.0]).striped(true).show(ui, |ui| {
                    for (id, enabled, tmpl_name, days, h, m) in &blocks {
                        let mut en = *enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            let mut s = state.write().unwrap();
                            if let Some(b) = s.config.scheduled_blocks.iter_mut().find(|b| &b.id == id) {
                                b.enabled = en;
                            }
                            s.save_config();
                        }
                        let sel = self.selected_sched.as_deref() == Some(id);
                        if ui.selectable_label(sel, tmpl_name).clicked() {
                            self.selected_sched = Some(id.clone());
                        }
                        ui.label(egui::RichText::new(format!("{:02}:{:02}", h, m)).monospace().small());
                        ui.label(egui::RichText::new(days).small().weak());
                        ui.end_row();
                    }
                });
            });
        }
    }
}

// ── Template editor ───────────────────────────────────────────────────────────

pub struct TemplateEditor {
    pub open:   bool,
    pub is_new: bool,
    pub id:     String,

    name:          String,
    dur_hours:     u64,
    dur_mins:      u64,
    mode_idx:      usize, // 0=strict 1=delay 2=password
    delay_secs:    u64,
    password:      String,
    password2:     String,
    break_enabled: bool,
    break_every_h: u64,
    break_every_m: u64,
    break_dur_m:   u64,
    selected_rules: Vec<String>,
    pwd_error:     Option<String>,
}

impl TemplateEditor {
    pub fn new() -> Self {
        Self {
            open: true, is_new: true,
            id:   uuid::Uuid::new_v4().to_string(),
            name: String::new(),
            dur_hours: 1, dur_mins: 0,
            mode_idx: 1, delay_secs: 30,
            password: String::new(), password2: String::new(),
            break_enabled: false,
            break_every_h: 1, break_every_m: 0, break_dur_m: 5,
            selected_rules: Vec::new(),
            pwd_error: None,
        }
    }

    pub fn from_template(t: &SessionTemplate) -> Self {
        let (mode_idx, delay_secs, password) = match &t.mode {
            SessionMode::Strict                          => (0, 30, String::new()),
            SessionMode::FlexibleDelay { delay_secs }   => (1, *delay_secs, String::new()),
            SessionMode::FlexiblePassword { .. }        => (2, 30, String::new()),
        };
        let (be, beh, bem, bdm) = t.break_config.as_ref().map(|b| {
            (true, b.every_secs / 3600, (b.every_secs % 3600) / 60, b.duration_secs / 60)
        }).unwrap_or((false, 1, 0, 5));

        Self {
            open: true, is_new: false,
            id: t.id.clone(),
            name: t.name.clone(),
            dur_hours: t.duration_secs / 3600,
            dur_mins:  (t.duration_secs % 3600) / 60,
            mode_idx, delay_secs, password, password2: String::new(),
            break_enabled: be, break_every_h: beh, break_every_m: bem, break_dur_m: bdm,
            selected_rules: t.rule_ids.clone(),
            pwd_error: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &SharedState) -> Option<SessionTemplate> {
        let mut result  = None;
        let mut is_open = self.open;
        let title = if self.is_new { "New Session Template".into() }
                    else { format!("Edit Template: {}", self.name) };

        egui::Window::new(title)
            .open(&mut is_open)
            .resizable(true)
            .min_width(440.0)
            .show(ctx, |ui| { result = self.body(ui, state); });

        if !is_open { self.open = false; }
        result
    }

    fn body(&mut self, ui: &mut egui::Ui, state: &SharedState) -> Option<SessionTemplate> {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Name + duration
            egui::Grid::new("te_basic").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();
                ui.label("Duration:");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.dur_hours).range(0..=23).suffix("h"));
                    ui.add(egui::DragValue::new(&mut self.dur_mins).range(0..=59).suffix("m"));
                });
                ui.end_row();
            });

            ui.separator();
            ui.heading("Mode");

            ui.radio_value(&mut self.mode_idx, 0, "🔒 Strict — cannot be cancelled");
            ui.radio_value(&mut self.mode_idx, 1, "⏳ Flexible — cancel after a delay");
            if self.mode_idx == 1 {
                ui.indent("delay_indent", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Cancel delay:");
                        ui.add(egui::DragValue::new(&mut self.delay_secs).range(5..=3600).suffix("s"));
                        ui.label(egui::RichText::new("(must wait this long before cancellation takes effect)").small().weak());
                    });
                });
            }
            ui.radio_value(&mut self.mode_idx, 2, "🔑 Flexible — cancel with a password");
            if self.mode_idx == 2 {
                ui.indent("pwd_indent", |ui| {
                    egui::Grid::new("pwd_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Password:");
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                        ui.end_row();
                        ui.label("Confirm:");
                        ui.add(egui::TextEdit::singleline(&mut self.password2).password(true));
                        ui.end_row();
                    });
                    if let Some(err) = &self.pwd_error {
                        ui.label(egui::RichText::new(err).small().color(egui::Color32::from_rgb(220,80,80)));
                    }
                    ui.label(egui::RichText::new(
                        "Tip: use a password you won't remember easily, e.g. a random string you write on paper."
                    ).small().weak());
                });
            }

            ui.separator();
            ui.heading("Breaks");
            ui.checkbox(&mut self.break_enabled, "Enable scheduled breaks");
            if self.break_enabled {
                ui.indent("break_indent", |ui| {
                    egui::Grid::new("break_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                        ui.label("Break every:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.break_every_h).range(0..=23).suffix("h"));
                            ui.add(egui::DragValue::new(&mut self.break_every_m).range(0..=59).suffix("m"));
                        });
                        ui.end_row();
                        ui.label("Break duration:");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.break_dur_m).range(1..=120).suffix("m"));
                        });
                        ui.end_row();
                    });
                    ui.label(egui::RichText::new(
                        "During breaks, blocked apps are temporarily allowed."
                    ).small().weak());
                });
            }

            ui.separator();
            ui.heading("Rules to Block");
            ui.label(egui::RichText::new("Select which rules are enforced during this session.").small().weak());

            let all_rules: Vec<(String, String)> = state.read().unwrap()
                .config.rules.iter()
                .map(|r| (r.id.clone(), r.name.clone()))
                .collect();

            if all_rules.is_empty() {
                ui.label(egui::RichText::new("No rules defined yet — add rules in the Rules tab first.").weak());
            } else {
                egui::ScrollArea::vertical().id_source("rule_sel_scroll").max_height(160.0).show(ui, |ui| {
                    for (rid, rname) in &all_rules {
                        let mut sel = self.selected_rules.contains(rid);
                        if ui.checkbox(&mut sel, rname).changed() {
                            if sel { self.selected_rules.push(rid.clone()); }
                            else   { self.selected_rules.retain(|id| id != rid); }
                        }
                    }
                });
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() { self.open = false; }
            let can_save = !self.name.is_empty()
                && (self.dur_hours > 0 || self.dur_mins > 0)
                && !self.selected_rules.is_empty();
            ui.add_enabled_ui(can_save, |ui| {
                if ui.button(egui::RichText::new("  Save  ").strong()).clicked() {
                    // Validate password mode
                    if self.mode_idx == 2 {
                        if self.password.is_empty() {
                            self.pwd_error = Some("Password cannot be empty.".into());
                            return None;
                        }
                        if self.password != self.password2 {
                            self.pwd_error = Some("Passwords do not match.".into());
                            return None;
                        }
                    }
                    self.pwd_error = None;

                    let mode = match self.mode_idx {
                        0 => SessionMode::Strict,
                        2 => SessionMode::FlexiblePassword {
                            password_hash: crate::config::hash_password(&self.password),
                        },
                        _ => SessionMode::FlexibleDelay { delay_secs: self.delay_secs },
                    };
                    let break_config = self.break_enabled.then(|| BreakConfig {
                        every_secs:    self.break_every_h * 3600 + self.break_every_m * 60,
                        duration_secs: self.break_dur_m * 60,
                    });
                    self.open = false;
                    return Some(SessionTemplate {
                        id:            self.id.clone(),
                        name:          self.name.clone(),
                        duration_secs: self.dur_hours * 3600 + self.dur_mins * 60,
                        rule_ids:      self.selected_rules.clone(),
                        mode,
                        break_config,
                    });
                }
                None
            }).inner
        }).inner
    }
}

// ── Scheduled block editor ────────────────────────────────────────────────────

pub struct SchedEditor {
    pub open:    bool,
    pub is_new:  bool,
    pub id:      String,
    enabled:     bool,
    template_id: String,
    days:        Vec<u8>,
    start_hour:  u8,
    start_min:   u8,
}

impl SchedEditor {
    pub fn new(template_id: String) -> Self {
        Self {
            open: true, is_new: true,
            id:          uuid::Uuid::new_v4().to_string(),
            enabled:     true,
            template_id,
            days:        (0u8..5).collect(),
            start_hour:  9, start_min: 0,
        }
    }
    pub fn from_block(sb: &ScheduledBlock) -> Self {
        Self {
            open: true, is_new: false,
            id:          sb.id.clone(),
            enabled:     sb.enabled,
            template_id: sb.template_id.clone(),
            days:        sb.days.clone(),
            start_hour:  sb.start_hour,
            start_min:   sb.start_min,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &SharedState) -> Option<ScheduledBlock> {
        let mut result  = None;
        let mut is_open = self.open;
        egui::Window::new(if self.is_new { "New Scheduled Block" } else { "Edit Scheduled Block" })
            .open(&mut is_open)
            .resizable(false)
            .min_width(360.0)
            .show(ctx, |ui| { result = self.body(ui, state); });
        if !is_open { self.open = false; }
        result
    }

    fn body(&mut self, ui: &mut egui::Ui, state: &SharedState) -> Option<ScheduledBlock> {
        // Template picker
        ui.label("Session template:");
        let tmpls: Vec<(String, String)> = state.read().unwrap()
            .config.session_templates.iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect();

        let sel_name = tmpls.iter().find(|(id, _)| id == &self.template_id)
            .map(|(_, n)| n.as_str()).unwrap_or("(none)");

        egui::ComboBox::from_id_source("sched_tmpl_combo")
            .selected_text(sel_name)
            .show_ui(ui, |ui| {
                for (id, name) in &tmpls {
                    ui.selectable_value(&mut self.template_id, id.clone(), name);
                }
            });

        ui.separator();
        ui.label("Start time:");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.start_hour).range(0..=23));
            ui.label(":");
            ui.add(egui::DragValue::new(&mut self.start_min).range(0..=59));
        });

        ui.label("Days:");
        ui.horizontal(|ui| {
            for d in 0u8..7 {
                let name = crate::config::UnavailPeriod::day_name(d);
                let mut c = self.days.contains(&d);
                if ui.checkbox(&mut c, name).changed() {
                    if c { self.days.push(d); self.days.sort(); }
                    else { self.days.retain(|&x| x != d); }
                }
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() { self.open = false; }
            let can_save = !self.template_id.is_empty() && !self.days.is_empty();
            ui.add_enabled_ui(can_save, |ui| {
                if ui.button(egui::RichText::new("  Save  ").strong()).clicked() {
                    self.open = false;
                    return Some(ScheduledBlock {
                        id:          self.id.clone(),
                        enabled:     self.enabled,
                        template_id: self.template_id.clone(),
                        days:        self.days.clone(),
                        start_hour:  self.start_hour,
                        start_min:   self.start_min,
                        fired_today: false,
                    });
                }
                None
            }).inner
        }).inner
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_dur(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0      { format!("{h}h {m:02}m") }
    else if m > 0 { format!("{m}m {s:02}s") }
    else          { format!("{s}s") }
}

fn mode_label(mode: &SessionMode) -> String {
    match mode {
        SessionMode::Strict                        => "🔒 strict".into(),
        SessionMode::FlexibleDelay { delay_secs }  => format!("⏳ {}s delay", delay_secs),
        SessionMode::FlexiblePassword { .. }       => "🔑 password".into(),
    }
}
