use egui::Ui;
use crate::daemon::SharedState;
use crate::usage_tracker::{AppKind, UsageData, show_heatmap};

pub struct UsageTab {
    selected_id: Option<String>,
    cache:       Vec<(String, String)>, // (rule_id, rule_name) of tracked rules
    loaded:      Option<UsageData>,
    last_load:   Option<std::time::Instant>,
}

impl UsageTab {
    pub fn new() -> Self {
        Self {
            selected_id: None,
            cache:       Vec::new(),
            loaded:      None,
            last_load:   None,
        }
    }

    pub fn show(&mut self, ui: &mut Ui, state: &SharedState) {
        // Refresh tracked-rule list on each frame (cheap — just reads config)
        self.cache = state.read().unwrap().config.rules.iter()
            .filter(|r| r.track_usage)
            .map(|r| (r.id.clone(), r.name.clone()))
            .collect();

        if self.cache.is_empty() {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(
                    "No apps are being tracked yet.\n\
                     Edit a rule and enable \"Track usage for this app\" to get started."
                ).weak());
            });
            return;
        }

        // Ensure selection is valid
        if self.selected_id.as_ref().map(|id| !self.cache.iter().any(|(rid, _)| rid == id)).unwrap_or(true) {
            self.selected_id = self.cache.first().map(|(id, _)| id.clone());
            self.loaded      = None;
        }

        // Lazy-load usage data (reload at most once per minute)
        let needs_reload = self.loaded.is_none()
            || self.last_load.map(|t| t.elapsed().as_secs() >= 60).unwrap_or(true);
        if needs_reload {
            if let Some(id) = &self.selected_id {
                self.loaded     = Some(UsageData::load(id));
                self.last_load  = Some(std::time::Instant::now());
            }
        }

        // Merge in-memory daily counter (adds to today's total)
        let in_mem_today: u64 = self.selected_id.as_ref()
            .and_then(|id| state.read().unwrap().daily_usage_secs.get(id).copied())
            .unwrap_or(0);

        egui::SidePanel::left("usage_list")
            .resizable(false)
            .min_width(180.0)
            .show_inside(ui, |ui| {
                ui.heading("Tracked Apps");
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (id, name) in &self.cache {
                        let sel = self.selected_id.as_deref() == Some(id);
                        let today = {
                            let data = UsageData::load(id);
                            let base = data.today_total();
                            if sel { base.max(in_mem_today) } else { base }
                        };
                        let label = format!("{name}\n{}", fmt_duration(today));
                        if ui.selectable_label(sel, label).clicked() && !sel {
                            self.selected_id = Some(id.clone());
                            self.loaded      = None;
                        }
                    }
                });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let Some(data) = &self.loaded else { return; };
            let Some(sel_name) = self.cache.iter()
                .find(|(id, _)| Some(id) == self.selected_id.as_ref())
                .map(|(_, n)| n.as_str()) else { return; };

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading(sel_name);
                ui.separator();

                // ── Stats row ─────────────────────────────────────────────
                let today_total = data.today_total().max(in_mem_today);
                let avg         = data.avg_daily_secs();
                let kind        = AppKind::detect(sel_name);

                egui::Grid::new("usage_stats").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.strong("Today:");
                    ui.label(fmt_duration(today_total));
                    ui.end_row();

                    ui.strong("Daily avg:");
                    ui.label(fmt_duration(avg as u64));
                    ui.end_row();

                    ui.strong("App type:");
                    ui.label(kind.label());
                    ui.end_row();

                    if let Some(sug) = kind.recommended_limit_secs(avg) {
                        ui.strong("Suggestion:");
                        ui.label(egui::RichText::new(format!(
                            "Consider a {}-per-day limit (avg usage is high)",
                            fmt_duration(sug)
                        )).color(egui::Color32::from_rgb(255, 180, 50)));
                        ui.end_row();
                    }
                });

                ui.add_space(12.0);
                ui.separator();

                // ── Heatmap ───────────────────────────────────────────────
                ui.label(egui::RichText::new("Average usage heatmap (per hour, by day of week)").strong());
                ui.label(egui::RichText::new(
                    "Colour: darker = less usage, brighter red = more. Hover for details."
                ).small().weak());
                ui.add_space(6.0);

                let heatmap = data.heatmap();
                show_heatmap(ui, &heatmap);

                ui.add_space(12.0);
                ui.separator();

                // ── Raw records summary ───────────────────────────────────
                ui.label(egui::RichText::new("Recent days").strong());
                let mut days: Vec<_> = data.records.iter().collect();
                days.sort_by(|a, b| b.date.cmp(&a.date));
                egui::ScrollArea::vertical().id_source("usage_days").max_height(200.0).show(ui, |ui| {
                    egui::Grid::new("usage_days_grid").num_columns(2).spacing([16.0, 2.0]).striped(true).show(ui, |ui| {
                        for rec in days.iter().take(30) {
                            ui.label(egui::RichText::new(&rec.date).monospace().small());
                            ui.label(fmt_duration(rec.total_secs));
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }
}

fn fmt_duration(secs: u64) -> String {
    if secs == 0 { return "0m".into(); }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0      { format!("{h}h {m}m") }
    else if m > 0 { format!("{m}m {s}s") }
    else          { format!("{s}s") }
}
