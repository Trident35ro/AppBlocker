use egui::Ui;
use std::time::Instant;
use crate::config::Rule;
use crate::daemon::SharedState;
use crate::monitor::ProcessInfo;
use super::rule_editor::RuleEditor;

#[derive(Debug, Clone, PartialEq)]
enum SortCol { Name, Cpu, Ram, Pid }

pub struct MonitorTab {
    pub editor:         Option<RuleEditor>,
    sort_col:           SortCol,
    sort_asc:           bool,
    filter:             String,
    _last_refresh: Option<Instant>,
}

impl MonitorTab {
    pub fn new() -> Self {
        Self {
            editor:       None,
            sort_col:     SortCol::Cpu,
            sort_asc:     false,
            filter:       String::new(),
            _last_refresh: None,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &SharedState) {
        // ── Toolbar ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.filter);
            ui.separator();

            let age = state.read().unwrap().last_process_update
                .map(|t| t.elapsed().as_secs())
                .unwrap_or(0);
            ui.label(egui::RichText::new(format!("Updated {age}s ago")).weak().small());
        });

        ui.separator();

        // ── Column headers ─────────────────────────────────────────────────
        let mut procs = state.read().unwrap().processes.clone();

        // Filter
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            procs.retain(|p| p.name.to_lowercase().contains(&f)
                || p.exe_path.as_deref().unwrap_or("").to_lowercase().contains(&f));
        }

        // Sort
        match self.sort_col {
            SortCol::Name => procs.sort_by(|a, b| a.name.cmp(&b.name)),
            SortCol::Cpu  => procs.sort_by(|a, b|
                a.cpu_percent.partial_cmp(&b.cpu_percent).unwrap_or(std::cmp::Ordering::Equal)),
            SortCol::Ram  => procs.sort_by(|a, b|
                a.mem_mb.partial_cmp(&b.mem_mb).unwrap_or(std::cmp::Ordering::Equal)),
            SortCol::Pid  => procs.sort_by_key(|p| p.pid),
        }
        if !self.sort_asc { procs.reverse(); }

        egui::Grid::new("mon_hdr")
            .num_columns(5)
            .min_col_width(80.0)
            .show(ui, |ui| {
                self.sort_header(ui, "Name",    SortCol::Name);
                self.sort_header(ui, "CPU %",   SortCol::Cpu);
                self.sort_header(ui, "RAM (MB)", SortCol::Ram);
                self.sort_header(ui, "PID",     SortCol::Pid);
                ui.strong("Status");
                ui.end_row();
            });

        ui.separator();

        // ── Process rows ───────────────────────────────────────────────────
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("mon_grid")
                .num_columns(5)
                .min_col_width(80.0)
                .striped(true)
                .show(ui, |ui| {
                    let mut open_editor: Option<RuleEditor> = None;
                    let mut block_now:   Option<ProcessInfo> = None;

                    for proc in &procs {
                        let row = ui.label(&proc.name);

                        row.context_menu(|ui| {
                            ui.label(egui::RichText::new(
                                proc.exe_path.as_deref().unwrap_or(&proc.name)
                            ).small().weak());
                            ui.separator();

                            if ui.button("Create block rule…").clicked() {
                                let rule = Rule::new(
                                    &proc.name,
                                    proc.exe_path.as_deref().unwrap_or(&proc.name),
                                );
                                open_editor = Some(RuleEditor::from_rule(&rule));
                                ui.close_menu();
                            }
                            if ui.button("Block for rest of day").clicked() {
                                block_now = Some(proc.clone());
                                ui.close_menu();
                            }
                        });

                        let cpu_color = cpu_color(proc.cpu_percent);
                        ui.colored_label(cpu_color, format!("{:.1}%", proc.cpu_percent));
                        ui.label(format!("{:.1}", proc.mem_mb));
                        ui.label(proc.pid.to_string());
                        ui.label(&proc.status);
                        ui.end_row();
                    }

                    if let Some(editor) = open_editor {
                        self.editor = Some(editor);
                    }

                    // Quick "block rest of day" from context menu
                    if let Some(proc) = block_now {
                        let exe = proc.exe_path.as_deref().unwrap_or(&proc.name).to_owned();
                        // This will be handled by the outer show() after borrow ends
                        // Store intent for next frame — easiest via editor pre-flagged
                        let mut r = Rule::new(&proc.name, &exe);
                        r.block_rest_of_day();
                        // We can't mutate state here (Grid closure), so open editor instead
                        self.editor = Some(RuleEditor::from_rule(&r));
                    }
                });
        });

        // ── Editor (opened from context menu) ──────────────────────────────
        if let Some(editor) = &mut self.editor {
            if let Some(rule) = editor.show(ui.ctx()) {
                let mut s = state.write().unwrap();
                // Avoid duplicates by name+exe
                let exists = s.config.rules.iter()
                    .any(|r| r.executable == rule.executable);
                if !exists {
                    s.config.rules.push(rule);
                    s.save_config();
                }
            }
            if !editor.open {
                self.editor = None;
            }
        }
    }

    fn sort_header(&mut self, ui: &mut Ui, label: &str, col: SortCol) {
        let marker = if self.sort_col == col {
            if self.sort_asc { " ▲" } else { " ▼" }
        } else { "" };
        if ui.strong(format!("{label}{marker}")).clicked() {
            if self.sort_col == col { self.sort_asc = !self.sort_asc; }
            else { self.sort_col = col; self.sort_asc = false; }
        }
    }
}

fn cpu_color(pct: f32) -> egui::Color32 {
    if      pct >= 80.0 { egui::Color32::from_rgb(220, 60, 60) }
    else if pct >= 40.0 { egui::Color32::from_rgb(220, 160, 0) }
    else                { egui::Color32::GRAY }
}
