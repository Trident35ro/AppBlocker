use egui::Ui;
use crate::config::{
    BlockingMethod, GracePeriod, ResourceTrigger, Rule, RuleAction, ScheduleType,
    StartupAction, TimeLimit, TimeRangeConfig, UnavailPeriod,
};

#[derive(Debug, Clone, PartialEq)]
enum SchedVariant { Always, TimeRange, RestOfDay }

pub struct RuleEditor {
    pub open:    bool,
    pub is_new:  bool,

    id:          String,
    name:        String,
    executable:  String,
    method:      BlockingMethod,
    enabled:     bool,
    persist:     bool,

    sched_variant:  SchedVariant,
    tr_start_hour:  u8,
    tr_start_min:   u8,
    tr_end_hour:    u8,
    tr_end_min:     u8,

    grace_block_mins:   u64,
    grace_unblock_mins: u64,

    startup:     StartupAction,

    res_enabled:  bool,
    res_cpu:      f32,
    res_cpu_on:   bool,
    res_ram_mb:   u64,
    res_ram_on:   bool,
    res_dur_secs: u64,

    fuzzy_match: bool,

    // ── New fields ──────────────────────────────────────────────────────────
    rule_action:               RuleAction,
    mindful_intercept_running: bool,

    time_limit_enabled: bool,
    tl_hours:           u64,
    tl_mins:            u64,
    tl_reset_hour:      u8,
    tl_reset_min:       u8,
    tl_hard_block:      bool,
    tl_remind_10:       bool,
    tl_remind_5:        bool,
    tl_remind_1:        bool,

    unavail_periods:  Vec<UnavailPeriod>,
    adding_period:    bool,
    new_period:       UnavailPeriod,

    track_usage: bool,
}

impl RuleEditor {
    pub fn new_rule() -> Self {
        Self {
            open:   true,
            is_new: true,
            id:     uuid::Uuid::new_v4().to_string(),
            name:   String::new(),
            executable: String::new(),
            method:  BlockingMethod::Kill,
            enabled: true,
            persist: true,

            sched_variant:  SchedVariant::Always,
            tr_start_hour:  9,  tr_start_min: 0,
            tr_end_hour:   17,  tr_end_min:   0,

            grace_block_mins:   0,
            grace_unblock_mins: 0,

            startup: StartupAction::None,

            res_enabled:  false,
            res_cpu:      80.0,
            res_cpu_on:   true,
            res_ram_mb:   512,
            res_ram_on:   false,
            res_dur_secs: 300,

            fuzzy_match: false,

            rule_action:               RuleAction::Block,
            mindful_intercept_running: false,

            time_limit_enabled: false,
            tl_hours:           2,
            tl_mins:            0,
            tl_reset_hour:      0,
            tl_reset_min:       0,
            tl_hard_block:      false,
            tl_remind_10:       true,
            tl_remind_5:        true,
            tl_remind_1:        true,

            unavail_periods: Vec::new(),
            adding_period:   false,
            new_period:      UnavailPeriod::new(),

            track_usage: false,
        }
    }

    pub fn from_rule(rule: &Rule) -> Self {
        let (sched_variant, tr_sh, tr_sm, tr_eh, tr_em) = match &rule.schedule {
            ScheduleType::Always      => (SchedVariant::Always, 9, 0, 17, 0),
            ScheduleType::TimeRange(r) =>
                (SchedVariant::TimeRange, r.start_hour, r.start_min, r.end_hour, r.end_min),
            ScheduleType::RestOfDay   => (SchedVariant::RestOfDay, 0, 0, 0, 0),
        };

        let (res_enabled, res_cpu, res_cpu_on, res_ram_mb, res_ram_on, res_dur_secs) =
            if let Some(rt) = &rule.resource_trigger {
                (true, rt.cpu_percent.unwrap_or(80.0), rt.cpu_percent.is_some(),
                 rt.ram_mb.unwrap_or(512), rt.ram_mb.is_some(), rt.duration_secs)
            } else { (false, 80.0, true, 512, false, 300) };

        let (tl_enabled, tl_hours, tl_mins, tl_reset_hour, tl_reset_min,
             tl_hard, tl_r10, tl_r5, tl_r1) =
            if let Some(lim) = &rule.time_limit {
                let h   = lim.daily_limit_secs / 3600;
                let m   = (lim.daily_limit_secs % 3600) / 60;
                let r10 = lim.remind_thresholds.contains(&600);
                let r5  = lim.remind_thresholds.contains(&300);
                let r1  = lim.remind_thresholds.contains(&60);
                (true, h, m, lim.reset_hour, lim.reset_min, lim.hard_block, r10, r5, r1)
            } else { (false, 2, 0, 0, 0, false, true, true, true) };

        Self {
            open:   true,
            is_new: false,
            id:     rule.id.clone(),
            name:   rule.name.clone(),
            executable: rule.executable.clone(),
            method:  rule.blocking_method.clone(),
            enabled: rule.enabled,
            persist: rule.persist_across_reboots,

            sched_variant,
            tr_start_hour: tr_sh, tr_start_min: tr_sm,
            tr_end_hour:   tr_eh, tr_end_min:   tr_em,

            grace_block_mins:   rule.grace_period.warn_before_block_secs   / 60,
            grace_unblock_mins: rule.grace_period.warn_before_unblock_secs / 60,

            startup: rule.startup_action.clone(),

            res_enabled, res_cpu, res_cpu_on, res_ram_mb, res_ram_on, res_dur_secs,

            fuzzy_match: rule.fuzzy_match,

            rule_action:               rule.rule_action.clone(),
            mindful_intercept_running: rule.mindful_intercept_running,

            time_limit_enabled: tl_enabled,
            tl_hours, tl_mins, tl_reset_hour, tl_reset_min,
            tl_hard_block: tl_hard,
            tl_remind_10:  tl_r10,
            tl_remind_5:   tl_r5,
            tl_remind_1:   tl_r1,

            unavail_periods: rule.unavail_periods.clone(),
            adding_period:   false,
            new_period:      UnavailPeriod::new(),

            track_usage: rule.track_usage,
        }
    }

    pub fn build_rule(&self) -> Rule {
        let schedule = match self.sched_variant {
            SchedVariant::Always    => ScheduleType::Always,
            SchedVariant::TimeRange => ScheduleType::TimeRange(TimeRangeConfig {
                start_hour: self.tr_start_hour, start_min: self.tr_start_min,
                end_hour:   self.tr_end_hour,   end_min:   self.tr_end_min,
            }),
            SchedVariant::RestOfDay => ScheduleType::RestOfDay,
        };

        let resource_trigger = self.res_enabled.then(|| ResourceTrigger {
            cpu_percent:   self.res_cpu_on.then_some(self.res_cpu),
            ram_mb:        self.res_ram_on.then_some(self.res_ram_mb),
            duration_secs: self.res_dur_secs,
        });

        let time_limit = self.time_limit_enabled.then(|| {
            let mut thresholds = Vec::new();
            if self.tl_remind_10 { thresholds.push(600); }
            if self.tl_remind_5  { thresholds.push(300); }
            if self.tl_remind_1  { thresholds.push(60);  }
            TimeLimit {
                daily_limit_secs:  self.tl_hours * 3600 + self.tl_mins * 60,
                reset_hour:        self.tl_reset_hour,
                reset_min:         self.tl_reset_min,
                hard_block:        self.tl_hard_block,
                remind_thresholds: thresholds,
            }
        });

        Rule {
            id:         self.id.clone(),
            name:       self.name.clone(),
            executable: self.executable.clone(),
            blocking_method: self.method.clone(),
            enabled:    self.enabled,
            schedule,
            grace_period: GracePeriod {
                warn_before_block_secs:   self.grace_block_mins   * 60,
                warn_before_unblock_secs: self.grace_unblock_mins * 60,
            },
            startup_action:         self.startup.clone(),
            resource_trigger,
            persist_across_reboots: self.persist,
            blocked_until:          None,
            fuzzy_match:            self.fuzzy_match,
            time_limit,
            unavail_periods:        self.unavail_periods.clone(),
            rule_action:            self.rule_action.clone(),
            mindful_intercept_running: self.mindful_intercept_running,
            track_usage:            self.track_usage,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Rule> {
        let mut result  = None;
        let mut is_open = self.open;

        let title = if self.is_new { "Add Rule".to_owned() }
                    else { format!("Edit Rule: {}", self.name) };

        egui::Window::new(title)
            .open(&mut is_open)
            .resizable(true)
            .min_width(500.0)
            .show(ctx, |ui| {
                result = self.body(ui);
            });

        if !is_open { self.open = false; }
        result
    }

    fn body(&mut self, ui: &mut Ui) -> Option<Rule> {
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.section_app(ui);
            ui.separator();
            self.section_action(ui);
            ui.separator();
            self.section_blocking(ui);
            ui.separator();
            self.section_schedule(ui);
            ui.separator();
            self.section_unavail(ui);
            ui.separator();
            self.section_time_limit(ui);
            ui.separator();
            self.section_grace(ui);
            ui.separator();
            self.section_startup(ui);
            ui.separator();
            self.section_resource(ui);
            ui.separator();
            self.section_misc(ui);
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                self.open = false;
            }
            let can_save = !self.name.is_empty() && !self.executable.is_empty();
            ui.add_enabled_ui(can_save, |ui| {
                if ui.button(egui::RichText::new("  Save  ").strong()).clicked() {
                    self.open = false;
                    return Some(self.build_rule());
                }
                None::<Rule>
            }).inner
        }).inner
    }

    fn section_app(&mut self, ui: &mut Ui) {
        ui.heading("Application");
        egui::Grid::new("app_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Display name:");
            ui.text_edit_singleline(&mut self.name);
            ui.end_row();

            ui.label("Executable / name:");
            ui.text_edit_singleline(&mut self.executable);
            ui.end_row();

            ui.label("");
            ui.label(egui::RichText::new(
                "Full path (e.g. /usr/bin/firefox) or process name (e.g. steam). Case-insensitive."
            ).small().weak());
            ui.end_row();

            ui.label("");
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.fuzzy_match, "Lazy match");
                ui.label(egui::RichText::new(
                    "— match any process whose name contains the above"
                ).small().weak());
            });
            ui.end_row();
        });
    }

    fn section_action(&mut self, ui: &mut Ui) {
        ui.heading("Action Mode");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.rule_action, RuleAction::Block, "Block");
            ui.label(egui::RichText::new("— kill/block the process when the rule is active").small().weak());
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.rule_action, RuleAction::Mindful, "Mindful");
            ui.label(egui::RichText::new(
                "— never block; instead ask \"why?\" before each launch"
            ).small().weak());
        });

        if self.rule_action == RuleAction::Mindful {
            ui.indent("mindful_opts", |ui| {
                ui.checkbox(
                    &mut self.mindful_intercept_running,
                    "Also prompt when app is already running (every 30 min)",
                );
                if self.method != BlockingMethod::Wrapper {
                    ui.label(egui::RichText::new(
                        "⚠ Launch interception requires PATH Wrapper as blocking method."
                    ).small().color(egui::Color32::YELLOW));
                }
            });
        }
    }

    fn section_blocking(&mut self, ui: &mut Ui) {
        ui.heading("Blocking Method");
        for method in [
            BlockingMethod::Kill,
            BlockingMethod::ForceKill,
            BlockingMethod::Wrapper,
            BlockingMethod::Network,
        ] {
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.method, method.clone(), format!("{method}"));
                match method {
                    BlockingMethod::Kill      =>
                        { ui.label(egui::RichText::new("← recommended").small().weak()); }
                    BlockingMethod::ForceKill =>
                        { ui.label(egui::RichText::new("no chance to save").small().weak()); }
                    _ => {}
                }
            });
        }
        if self.method == BlockingMethod::Network {
            ui.horizontal(|ui| {
                ui.label("⚠");
                ui.label(egui::RichText::new(
                    "Network blocking requires root. AppBlocker will prompt via pkexec."
                ).small().color(egui::Color32::YELLOW));
            });
        }
    }

    fn section_schedule(&mut self, ui: &mut Ui) {
        ui.heading("Schedule");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.sched_variant, SchedVariant::Always,    "Always");
            ui.radio_value(&mut self.sched_variant, SchedVariant::TimeRange, "Time range");
            ui.radio_value(&mut self.sched_variant, SchedVariant::RestOfDay, "Rest of Day");
        });

        if self.sched_variant == SchedVariant::TimeRange {
            ui.horizontal(|ui| {
                ui.label("Block from");
                ui.add(egui::DragValue::new(&mut self.tr_start_hour).range(0..=23));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut self.tr_start_min).range(0..=59));
                ui.label("to");
                ui.add(egui::DragValue::new(&mut self.tr_end_hour).range(0..=23));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut self.tr_end_min).range(0..=59));
            });
        }
        if self.sched_variant == SchedVariant::RestOfDay {
            ui.label(egui::RichText::new("Active until midnight when triggered.").small().weak());
        }
    }

    fn section_unavail(&mut self, ui: &mut Ui) {
        ui.heading("Unavailability Periods");
        ui.label(egui::RichText::new(
            "Block the app during specific recurring windows (uses system local time)."
        ).small().weak());

        let mut remove_idx: Option<usize> = None;
        for (i, p) in self.unavail_periods.iter().enumerate() {
            ui.horizontal(|ui| {
                let days: String = p.days.iter()
                    .map(|&d| UnavailPeriod::day_name(d))
                    .collect::<Vec<_>>().join(" ");
                let label = if p.label.is_empty() {
                    format!("{}  {:02}:{:02}–{:02}:{:02}",
                        days, p.start_hour, p.start_min, p.end_hour, p.end_min)
                } else {
                    format!("{}  {} {:02}:{:02}–{:02}:{:02}",
                        p.label, days, p.start_hour, p.start_min, p.end_hour, p.end_min)
                };
                ui.label(egui::RichText::new(label).monospace().small());
                if ui.small_button("✕").clicked() { remove_idx = Some(i); }
            });
        }
        if let Some(i) = remove_idx { self.unavail_periods.remove(i); }

        if self.adding_period {
            ui.group(|ui| {
                ui.label("New period:");
                egui::Grid::new("new_period_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("Label (optional):");
                    ui.text_edit_singleline(&mut self.new_period.label);
                    ui.end_row();

                    ui.label("Days:");
                    ui.horizontal(|ui| {
                        for d in 0u8..7 {
                            let name = UnavailPeriod::day_name(d);
                            let checked = self.new_period.days.contains(&d);
                            let mut c = checked;
                            if ui.checkbox(&mut c, name).changed() {
                                if c { self.new_period.days.push(d); self.new_period.days.sort(); }
                                else { self.new_period.days.retain(|&x| x != d); }
                            }
                        }
                    });
                    ui.end_row();

                    ui.label("From:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.new_period.start_hour).range(0..=23));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut self.new_period.start_min).range(0..=59));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut self.new_period.end_hour).range(0..=23));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut self.new_period.end_min).range(0..=59));
                    });
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        self.unavail_periods.push(self.new_period.clone());
                        self.new_period    = UnavailPeriod::new();
                        self.adding_period = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.adding_period = false;
                    }
                });
            });
        } else if ui.button("+ Add Period").clicked() {
            self.adding_period = true;
            self.new_period    = UnavailPeriod::new();
        }
    }

    fn section_time_limit(&mut self, ui: &mut Ui) {
        ui.heading("Daily Time Limit");
        ui.checkbox(&mut self.time_limit_enabled, "Enable daily time limit");

        if self.time_limit_enabled {
            ui.indent("tl_indent", |ui| {
                egui::Grid::new("tl_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
                    ui.label("Daily limit:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.tl_hours).range(0..=23).suffix("h"));
                        ui.add(egui::DragValue::new(&mut self.tl_mins).range(0..=59).suffix("m"));
                    });
                    ui.end_row();

                    ui.label("Reset at:");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.tl_reset_hour).range(0..=23));
                        ui.label(":");
                        ui.add(egui::DragValue::new(&mut self.tl_reset_min).range(0..=59));
                        ui.label(egui::RichText::new("(local time)").small().weak());
                    });
                    ui.end_row();
                });

                ui.checkbox(&mut self.tl_hard_block, "Block app when limit is reached");
                if !self.tl_hard_block {
                    ui.label(egui::RichText::new("Without this, only a notification is sent.").small().weak());
                }

                ui.label("Remind me at:");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.tl_remind_10, "10 min");
                    ui.checkbox(&mut self.tl_remind_5,  "5 min");
                    ui.checkbox(&mut self.tl_remind_1,  "1 min");
                    ui.label(egui::RichText::new("remaining").small().weak());
                });
            });
        }
    }

    fn section_grace(&mut self, ui: &mut Ui) {
        ui.heading("Grace Period");
        egui::Grid::new("grace_grid").num_columns(3).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Warn before blocking:");
            ui.add(egui::DragValue::new(&mut self.grace_block_mins).range(0..=120));
            ui.label("minutes");
            ui.end_row();

            ui.label("Warn before unblocking:");
            ui.add(egui::DragValue::new(&mut self.grace_unblock_mins).range(0..=120));
            ui.label("minutes");
            ui.end_row();
        });
        ui.label(egui::RichText::new("Set to 0 for immediate blocking with no warning.").small().weak());
    }

    fn section_startup(&mut self, ui: &mut Ui) {
        ui.heading("Startup Action");
        for action in [StartupAction::None, StartupAction::LaunchOnStartup, StartupAction::BlockOnStartup] {
            ui.radio_value(&mut self.startup, action.clone(), format!("{action}"));
        }
    }

    fn section_resource(&mut self, ui: &mut Ui) {
        ui.heading("Resource Trigger");
        ui.checkbox(&mut self.res_enabled, "Block when resource usage exceeds threshold");

        if self.res_enabled {
            ui.indent("res_indent", |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.res_cpu_on, "CPU >");
                    ui.add_enabled(
                        self.res_cpu_on,
                        egui::DragValue::new(&mut self.res_cpu).range(1.0..=200.0).suffix("%"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.res_ram_on, "RAM >");
                    ui.add_enabled(
                        self.res_ram_on,
                        egui::DragValue::new(&mut self.res_ram_mb).range(1..=65536).suffix(" MB"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("For at least");
                    ui.add(egui::DragValue::new(&mut self.res_dur_secs).range(5..=3600));
                    ui.label("seconds before blocking");
                });
            });
        }
    }

    fn section_misc(&mut self, ui: &mut Ui) {
        ui.heading("Misc");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.persist, "Persist across reboots");
        });
        if !self.persist {
            ui.label(egui::RichText::new(
                "Session-only: rule is removed when AppBlocker exits."
            ).small().weak());
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.track_usage, "Track usage for this app");
            ui.label(egui::RichText::new("— see the Usage tab for stats").small().weak());
        });
    }
}
