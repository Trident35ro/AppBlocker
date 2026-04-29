use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Per-day record ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DayRecord {
    /// ISO date "YYYY-MM-DD".
    pub date:       String,
    pub total_secs: u64,
    /// 24 slots, one per hour.
    pub hourly:     Vec<u64>,
}

impl DayRecord {
    fn new(date: NaiveDate) -> Self {
        Self { date: date.to_string(), total_secs: 0, hourly: vec![0u64; 24] }
    }

    fn naive_date(&self) -> Option<NaiveDate> {
        self.date.parse().ok()
    }
}

// ── Per-rule usage store ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageData {
    pub rule_id:   String,
    pub rule_name: String,
    pub records:   Vec<DayRecord>,
}

impl UsageData {
    pub fn storage_path(rule_id: &str) -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("appblocker")
            .join("usage")
            .join(format!("{rule_id}.enc"))
    }

    pub fn load(rule_id: &str) -> Self {
        let path = Self::storage_path(rule_id);
        if !path.exists() {
            return Self { rule_id: rule_id.into(), rule_name: String::new(), records: vec![] };
        }
        let bytes = match std::fs::read(&path) {
            Ok(b)  => b,
            Err(_) => return Self { rule_id: rule_id.into(), rule_name: String::new(), records: vec![] },
        };
        let key   = derive_key(&whoami());
        let plain = xor_cipher(&bytes, &key);
        let text  = String::from_utf8(plain).unwrap_or_default();
        toml::from_str(&text).unwrap_or_else(|_| Self {
            rule_id: rule_id.into(), rule_name: String::new(), records: vec![],
        })
    }

    pub fn save(&self) {
        let path = Self::storage_path(&self.rule_id);
        if let Some(p) = path.parent() { let _ = std::fs::create_dir_all(p); }
        let text = match toml::to_string_pretty(self) {
            Ok(s)  => s,
            Err(_) => return,
        };
        let key    = derive_key(&whoami());
        let cipher = xor_cipher(text.as_bytes(), &key);
        let _      = std::fs::write(&path, cipher);
    }

    pub fn add_secs(&mut self, date: NaiveDate, hour: u8, secs: u64) {
        let date_str = date.to_string();
        if let Some(rec) = self.records.iter_mut().find(|r| r.date == date_str) {
            rec.total_secs = rec.total_secs.saturating_add(secs);
            if rec.hourly.len() < 24 { rec.hourly.resize(24, 0); }
            rec.hourly[hour as usize] = rec.hourly[hour as usize].saturating_add(secs);
        } else {
            let mut rec = DayRecord::new(date);
            if rec.hourly.len() < 24 { rec.hourly.resize(24, 0); }
            rec.total_secs      = secs;
            rec.hourly[hour as usize] = secs;
            self.records.push(rec);
        }
    }

    pub fn today_total(&self) -> u64 {
        let today = Local::now().date_naive().to_string();
        self.records.iter().find(|r| r.date == today).map(|r| r.total_secs).unwrap_or(0)
    }

    pub fn avg_daily_secs(&self) -> f64 {
        if self.records.is_empty() { return 0.0; }
        let total: u64 = self.records.iter().map(|r| r.total_secs).sum();
        total as f64 / self.records.len() as f64
    }

    /// Returns a [weekday][hour] matrix of average seconds per slot.
    /// weekday: 0 = Monday … 6 = Sunday.
    pub fn heatmap(&self) -> [[f64; 24]; 7] {
        let mut totals: [[u64; 24]; 7] = [[0; 24]; 7];
        let mut counts: [[u64; 24]; 7] = [[0; 24]; 7];

        for rec in &self.records {
            let date = match rec.naive_date() { Some(d) => d, None => continue };
            let wd   = date.weekday().num_days_from_monday() as usize;
            for h in 0..24usize {
                let v = rec.hourly.get(h).copied().unwrap_or(0);
                if v > 0 {
                    totals[wd][h] += v;
                    counts[wd][h] += 1;
                }
            }
        }

        let mut result = [[0f64; 24]; 7];
        for wd in 0..7usize {
            for h in 0..24usize {
                result[wd][h] = if counts[wd][h] > 0 {
                    totals[wd][h] as f64 / counts[wd][h] as f64
                } else { 0.0 };
            }
        }
        result
    }

    pub fn cleanup(&mut self, retention_days: u64) {
        let cutoff = (Local::now().date_naive()
            - chrono::Duration::days(retention_days as i64))
            .to_string();
        self.records.retain(|r| r.date > cutoff);
    }
}

// ── XOR obfuscation ───────────────────────────────────────────────────────────

fn derive_key(username: &str) -> Vec<u8> {
    let bytes = username.as_bytes();
    if bytes.is_empty() { return vec![0x5A; 32]; }
    (0..32).map(|i| bytes[i % bytes.len()]).collect()
}

fn xor_cipher(data: &[u8], key: &[u8]) -> Vec<u8> {
    if key.is_empty() { return data.to_vec(); }
    data.iter().enumerate().map(|(i, &b)| b ^ key[i % key.len()]).collect()
}

pub fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".into())
}

// ── App-kind detection & recommendations ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum AppKind {
    Browser,
    Ide,
    Game,
    Social,
    Video,
    Music,
    Other,
}

impl AppKind {
    pub fn detect(exe_name: &str) -> Self {
        let n = exe_name.to_lowercase();
        const BROWSERS: &[&str] = &["firefox","chrome","chromium","brave","opera","vivaldi",
                                     "falkon","midori","epiphany","konqueror","qutebrowser"];
        const IDES:     &[&str] = &["code","cursor","idea","pycharm","goland","clion",
                                     "webstorm","rider","vim","nvim","emacs","kate",
                                     "sublime","atom","eclipse","netbeans","android-studio"];
        const GAMES:    &[&str] = &["steam","lutris","wine","heroic","bottles",
                                     "mangohud","gamemode","minecraft"];
        const SOCIAL:   &[&str] = &["discord","slack","telegram","signal","element",
                                     "hexchat","thunderbird","evolution","teams","zoom","skype"];
        const VIDEO:    &[&str] = &["vlc","mpv","celluloid","totem","kodi","smplayer","mplayer"];
        const MUSIC:    &[&str] = &["spotify","rhythmbox","clementine","strawberry","lollypop","cantata","elisa"];

        if BROWSERS.iter().any(|&b| n.contains(b)) { return Self::Browser; }
        if IDES.iter().any(|&i| n.contains(i))     { return Self::Ide; }
        if GAMES.iter().any(|&g| n.contains(g))    { return Self::Game; }
        if SOCIAL.iter().any(|&s| n.contains(s))   { return Self::Social; }
        if VIDEO.iter().any(|&v| n.contains(v))    { return Self::Video; }
        if MUSIC.iter().any(|&m| n.contains(m))    { return Self::Music; }
        Self::Other
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Browser => "Browser",
            Self::Ide     => "IDE / Editor",
            Self::Game    => "Game / Launcher",
            Self::Social  => "Social / Chat",
            Self::Video   => "Video Player",
            Self::Music   => "Music Player",
            Self::Other   => "Other",
        }
    }

    /// Returns a suggested daily limit in seconds if the average exceeds the threshold.
    pub fn recommended_limit_secs(&self, avg_daily_secs: f64) -> Option<u64> {
        let (threshold, suggestion): (f64, u64) = match self {
            Self::Browser => (2.0 * 3600.0, 2 * 3600),
            Self::Social  => (1.0 * 3600.0, 45 * 60),
            Self::Game    => (2.0 * 3600.0, 90 * 60),
            Self::Video   => (3.0 * 3600.0, 150 * 60),
            Self::Other   => (3.0 * 3600.0, 150 * 60),
            Self::Ide | Self::Music => return None,
        };
        if avg_daily_secs > threshold { Some(suggestion) } else { None }
    }
}

// ── Heatmap rendering ─────────────────────────────────────────────────────────

pub fn show_heatmap(ui: &mut egui::Ui, data: &[[f64; 24]; 7]) {
    const CELL_W: f32  = 24.0;
    const CELL_H: f32  = 22.0;
    const LABEL_W: f32 = 38.0;
    const HDR_H:   f32 = 18.0;

    let total_w = LABEL_W + 24.0 * CELL_W;
    let total_h = HDR_H + 7.0 * CELL_H;

    let (resp, painter) = ui.allocate_painter(
        egui::vec2(total_w, total_h),
        egui::Sense::hover(),
    );
    let origin  = resp.rect.min;
    let max_val = data.iter().flat_map(|r| r.iter()).cloned().fold(1.0f64, f64::max);
    let day_names = ["Mon","Tue","Wed","Thu","Fri","Sat","Sun"];

    // Hour labels (every 3 h)
    for h in (0..24usize).step_by(3) {
        let x = origin.x + LABEL_W + h as f32 * CELL_W + CELL_W / 2.0;
        painter.text(
            egui::pos2(x, origin.y + HDR_H / 2.0),
            egui::Align2::CENTER_CENTER,
            &format!("{h:02}"),
            egui::FontId::proportional(10.0),
            ui.visuals().weak_text_color(),
        );
    }

    // Day rows
    for row in 0..7usize {
        let y_top = origin.y + HDR_H + row as f32 * CELL_H;

        painter.text(
            egui::pos2(origin.x + LABEL_W - 4.0, y_top + CELL_H / 2.0),
            egui::Align2::RIGHT_CENTER,
            day_names[row],
            egui::FontId::proportional(11.0),
            ui.visuals().text_color(),
        );

        for col in 0..24usize {
            let val  = data[row][col];
            let norm = (val / max_val).min(1.0) as f32;
            let cell = egui::Rect::from_min_size(
                egui::pos2(origin.x + LABEL_W + col as f32 * CELL_W + 1.0, y_top + 1.0),
                egui::vec2(CELL_W - 2.0, CELL_H - 2.0),
            );
            painter.rect_filled(cell, 3.0, heat_color(norm));
        }
    }

    // Hover tooltip
    if let Some(pos) = resp.hover_pos() {
        let rel_x = pos.x - origin.x - LABEL_W;
        let rel_y = pos.y - origin.y - HDR_H;
        if rel_x >= 0.0 && rel_y >= 0.0 {
            let col = (rel_x / CELL_W) as usize;
            let row = (rel_y / CELL_H) as usize;
            if col < 24 && row < 7 {
                let secs = data[row][col] as u64;
                let mins = secs / 60;
                let s    = secs % 60;
                resp.on_hover_text(format!(
                    "{} {:02}:00 — avg {}m {}s", day_names[row], col, mins, s,
                ));
            }
        }
    }
}

fn heat_color(norm: f32) -> egui::Color32 {
    if norm < 0.01 { return egui::Color32::from_rgb(35, 35, 45); }
    let r = ((norm * 2.0).min(1.0) * 220.0) as u8;
    let g = (((norm * 2.0 - 1.0).max(0.0).min(1.0).powf(0.5)) * 190.0) as u8;
    let b = ((1.0 - norm) * 180.0) as u8;
    egui::Color32::from_rgb(r, g, b)
}
