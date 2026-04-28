use egui::Ui;
use crate::config::{
    BlockingMethod, GracePeriod, ResourceTrigger, Rule, ScheduleType,
    StartupAction, TimeRangeConfig,
};

// ── Schedule variant (for the combo box) ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum SchedVariant { Always, TimeRange, RestOfDay }

// ── Editor state ──────────────────────────────────────────────────────────────

pub struct RuleEditor {
    pub open:    bool,
    pub is_new:  bool,

    // Working copy of all rule fields.
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

    startup: StartupAction,

    res_enabled:  bool,
    res_cpu:      f32,
    res_cpu_on:   bool,
    res_ram_mb:   u64,
    res_ram_on:   bool,
    res_dur_secs: u64,
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
        }
    }

    pub fn from_rule(rule: &Rule) -> Self {
        let (sched_variant, tr_sh, tr_sm, tr_eh, tr_em) = match &rule.schedule {
            ScheduleType::Always => (SchedVariant::Always, 9, 0, 17, 0),
            ScheduleType::TimeRange(r) =>
                (SchedVariant::TimeRange, r.start_hour, r.start_min, r.end_hour, r.end_min),
            ScheduleType::RestOfDay => (SchedVariant::RestOfDay, 0, 0, 0, 0),
        };

        let (res_enabled, res_cpu, res_cpu_on, res_ram_mb, res_ram_on, res_dur_secs) =
            if let Some(rt) = &rule.resource_trigger {
                (true,
                 rt.cpu_percent.unwrap_or(80.0),
                 rt.cpu_percent.is_some(),
                 rt.ram_mb.unwrap_or(512),
                 rt.ram_mb.is_some(),
                 rt.duration_secs)
            } else {
                (false, 80.0, true, 512, false, 300)
            };

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
        }
    }

    /// Build a Rule from the current editor state.
    pub fn build_rule(&self) -> Rule {
        let schedule = match self.sched_variant {
            SchedVariant::Always     => ScheduleType::Always,
            SchedVariant::TimeRange  => ScheduleType::TimeRange(TimeRangeConfig {
                start_hour: self.tr_start_hour, start_min: self.tr_start_min,
                end_hour:   self.tr_end_hour,   end_min:   self.tr_end_min,
            }),
            SchedVariant::RestOfDay  => ScheduleType::RestOfDay,
        };

        let resource_trigger = self.res_enabled.then(|| ResourceTrigger {
            cpu_percent:   self.res_cpu_on.then_some(self.res_cpu),
            ram_mb:        self.res_ram_on.then_some(self.res_ram_mb),
            duration_secs: self.res_dur_secs,
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
            startup_action: self.startup.clone(),
            resource_trigger,
            persist_across_reboots: self.persist,
            blocked_until: None,
        }
    }

    /// Returns Some(Rule) when the user clicks Save (and closes the window).
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Rule> {
        let mut result  = None;
        let mut is_open = self.open;

        let title = if self.is_new { "Add Rule".to_owned() }
                    else { format!("Edit Rule: {}", self.name) };

        egui::Window::new(title)
            .open(&mut is_open)
            .resizable(true)
            .min_width(460.0)
            .show(ctx, |ui| {
                result = self.body(ui);
            });

        // Propagate close (X button on the window).
        if !is_open { self.open = false; }

        result
    }

    fn body(&mut self, ui: &mut Ui) -> Option<Rule> {
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.section_app(ui);
            ui.separator();
            self.section_blocking(ui);
            ui.separator();
            self.section_schedule(ui);
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
                "Full path (e.g. /usr/bin/firefox) or just the process name \
                 (e.g. steam). Name matching is case-insensitive."
            ).small().weak());
            ui.end_row();
        });
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
                        { ui.label(egui::RichText::new("app gets no chance to save").small().weak()); }
                    _ => {}
                }
            });
        }
        if self.method == BlockingMethod::Network {
            ui.horizontal(|ui| {
                ui.label("⚠");
                ui.label(egui::RichText::new(
                    "Network blocking requires root. AppBlocker will prompt \
                     via pkexec when activating this rule."
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
            ui.label(egui::RichText::new(
                "The rule will be active until midnight when triggered."
            ).small().weak());
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
        ui.heading("Persistence");
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.persist, "Persist across reboots");
        });
        if !self.persist {
            ui.label(egui::RichText::new(
                "Session-only: rule will be removed when AppBlocker exits."
            ).small().weak());
        }
    }
}
