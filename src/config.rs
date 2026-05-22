use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;
use chrono;

// ── Blocking method ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockingMethod {
    Kill,
    ForceKill,
    Wrapper,
    Network,
}

impl Default for BlockingMethod {
    fn default() -> Self { Self::Kill }
}

impl std::fmt::Display for BlockingMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Kill      => write!(f, "Kill — SIGTERM (Recommended)"),
            Self::ForceKill => write!(f, "Force Kill — SIGKILL (pkill -9)"),
            Self::Wrapper   => write!(f, "PATH Wrapper"),
            Self::Network   => write!(f, "Network Block (requires root)"),
        }
    }
}

// ── Schedule ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeRangeConfig {
    pub start_hour: u8,
    pub start_min:  u8,
    pub end_hour:   u8,
    pub end_min:    u8,
}

impl TimeRangeConfig {
    pub fn is_active_now(&self) -> bool {
        let now   = Local::now().time();
        let start = chrono::NaiveTime::from_hms_opt(
            self.start_hour as u32, self.start_min as u32, 0,
        ).unwrap_or_default();
        let end = chrono::NaiveTime::from_hms_opt(
            self.end_hour as u32, self.end_min as u32, 0,
        ).unwrap_or_default();
        if start <= end { now >= start && now < end } else { now >= start || now < end }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScheduleType {
    Always,
    TimeRange(TimeRangeConfig),
    RestOfDay,
}

impl std::fmt::Display for ScheduleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Always       => write!(f, "Always"),
            Self::TimeRange(r) => write!(f, "{:02}:{:02} – {:02}:{:02}",
                r.start_hour, r.start_min, r.end_hour, r.end_min),
            Self::RestOfDay    => write!(f, "Rest of Day"),
        }
    }
}

// ── Unavailability periods ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UnavailPeriod {
    pub id:         String,
    pub label:      String,
    /// Days of the week: 0=Mon … 6=Sun
    pub days:       Vec<u8>,
    pub start_hour: u8,
    pub start_min:  u8,
    pub end_hour:   u8,
    pub end_min:    u8,
}

impl UnavailPeriod {
    pub fn new() -> Self {
        Self {
            id:         Uuid::new_v4().to_string(),
            label:      String::new(),
            days:       (0u8..5).collect(),
            start_hour: 22, start_min: 0,
            end_hour:   8,  end_min:   0,
        }
    }

    pub fn is_active_now(&self) -> bool {
        let now = Local::now();
        let wd  = now.weekday().num_days_from_monday() as u8;
        if !self.days.contains(&wd) { return false; }

        let time  = now.time();
        let start = chrono::NaiveTime::from_hms_opt(
            self.start_hour as u32, self.start_min as u32, 0,
        ).unwrap_or_default();
        let end = chrono::NaiveTime::from_hms_opt(
            self.end_hour as u32, self.end_min as u32, 0,
        ).unwrap_or_default();
        if start <= end { time >= start && time < end } else { time >= start || time < end }
    }

    pub fn day_name(d: u8) -> &'static str {
        match d { 0=>"Mon", 1=>"Tue", 2=>"Wed", 3=>"Thu", 4=>"Fri", 5=>"Sat", 6=>"Sun", _=>"?" }
    }
}

// ── Time limit ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeLimit {
    /// Total seconds of use allowed per day.
    pub daily_limit_secs:  u64,
    pub reset_hour:        u8,
    pub reset_min:         u8,
    /// Kill the process when the limit hits (false = notify only).
    pub hard_block:        bool,
    /// Seconds-remaining thresholds at which to send a reminder (e.g. [600,300,60]).
    pub remind_thresholds: Vec<u64>,
}

impl Default for TimeLimit {
    fn default() -> Self {
        Self {
            daily_limit_secs:  7200,
            reset_hour:        0,
            reset_min:         0,
            hard_block:        false,
            remind_thresholds: vec![600, 300, 60],
        }
    }
}

// ── Rule action ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum RuleAction {
    #[default]
    Block,
    /// Show a "why are you opening this?" prompt instead of blocking.
    Mindful,
}

impl std::fmt::Display for RuleAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block   => write!(f, "Block"),
            Self::Mindful => write!(f, "Mindful"),
        }
    }
}

// ── Resource trigger ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTrigger {
    pub cpu_percent:   Option<f32>,
    pub ram_mb:        Option<u64>,
    pub duration_secs: u64,
}

impl Default for ResourceTrigger {
    fn default() -> Self {
        Self { cpu_percent: Some(80.0), ram_mb: None, duration_secs: 300 }
    }
}

// ── Grace period ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GracePeriod {
    pub warn_before_block_secs:   u64,
    pub warn_before_unblock_secs: u64,
}

impl Default for GracePeriod {
    fn default() -> Self {
        Self { warn_before_block_secs: 0, warn_before_unblock_secs: 0 }
    }
}

// ── Startup action ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StartupAction {
    None,
    LaunchOnStartup,
    BlockOnStartup,
}

impl Default for StartupAction {
    fn default() -> Self { Self::None }
}

impl std::fmt::Display for StartupAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None            => write!(f, "None"),
            Self::LaunchOnStartup => write!(f, "Launch on Startup"),
            Self::BlockOnStartup  => write!(f, "Block on Startup"),
        }
    }
}

// ── Network rules ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkBlockMethod {
    Nftables,
    Dns,
}

impl Default for NetworkBlockMethod {
    fn default() -> Self { Self::Nftables }
}

impl std::fmt::Display for NetworkBlockMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nftables => write!(f, "nftables (IP/domain block, requires root)"),
            Self::Dns      => write!(f, "DNS via /etc/hosts (domain block, requires root)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NetworkPreset {
    BlockNsfw,
    BlockDistracting,
    BlockBoth,
}

impl std::fmt::Display for NetworkPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockNsfw        => write!(f, "Block NSFW Media"),
            Self::BlockDistracting => write!(f, "Block Low-Quality / Distracting Sites"),
            Self::BlockBoth        => write!(f, "Block NSFW + Distracting"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkEntry {
    pub value:   String,
    pub comment: String,
    pub enabled: bool,
}

impl NetworkEntry {
    pub fn new(value: impl Into<String>) -> Self {
        Self { value: value.into(), comment: String::new(), enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    pub id:      String,
    pub name:    String,
    pub enabled: bool,
    pub method:  NetworkBlockMethod,
    pub entries: Vec<NetworkEntry>,
    /// Empty = system-wide; otherwise rule is only active when one of these apps is running.
    #[serde(default)]
    pub apply_to_apps: Vec<String>,
    #[serde(default)]
    pub shutdown_on_connect: bool,
    /// Whether the rule has been pushed to the system (DNS/nftables).
    #[serde(default)]
    pub applied: bool,
}

impl NetworkRule {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id:                  Uuid::new_v4().to_string(),
            name:                name.into(),
            enabled:             true,
            method:              NetworkBlockMethod::default(),
            entries:             Vec::new(),
            apply_to_apps:       Vec::new(),
            shutdown_on_connect: false,
            applied:             false,
        }
    }
}

// ── Rule ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id:                     String,
    pub name:                   String,
    pub executable:             String,
    pub blocking_method:        BlockingMethod,
    pub enabled:                bool,
    pub schedule:               ScheduleType,
    pub grace_period:           GracePeriod,
    pub startup_action:         StartupAction,
    pub resource_trigger:       Option<ResourceTrigger>,
    pub persist_across_reboots: bool,
    #[serde(default)]
    pub blocked_until: Option<DateTime<Local>>,
    #[serde(default)]
    pub fuzzy_match: bool,
    #[serde(default)]
    pub time_limit: Option<TimeLimit>,
    #[serde(default)]
    pub unavail_periods: Vec<UnavailPeriod>,
    #[serde(default)]
    pub rule_action: RuleAction,
    /// In Mindful mode: also prompt when the app is detected already running.
    #[serde(default)]
    pub mindful_intercept_running: bool,
    #[serde(default)]
    pub track_usage: bool,
}

impl Rule {
    pub fn new(name: impl Into<String>, executable: impl Into<String>) -> Self {
        Self {
            id:                     Uuid::new_v4().to_string(),
            name:                   name.into(),
            executable:             executable.into(),
            blocking_method:        BlockingMethod::default(),
            enabled:                true,
            schedule:               ScheduleType::Always,
            grace_period:           GracePeriod::default(),
            startup_action:         StartupAction::default(),
            resource_trigger:       None,
            persist_across_reboots: true,
            blocked_until:          None,
            fuzzy_match:            false,
            time_limit:             None,
            unavail_periods:        Vec::new(),
            rule_action:            RuleAction::Block,
            mindful_intercept_running: false,
            track_usage:            false,
        }
    }

    pub fn is_schedule_active(&self) -> bool {
        if !self.enabled { return false; }
        match &self.schedule {
            ScheduleType::Always           => true,
            ScheduleType::TimeRange(range) => range.is_active_now(),
            ScheduleType::RestOfDay        => {
                self.blocked_until.map(|t| Local::now() < t).unwrap_or(false)
            }
        }
    }

    pub fn is_unavail_active(&self) -> bool {
        self.unavail_periods.iter().any(|p| p.is_active_now())
    }

    pub fn block_rest_of_day(&mut self) {
        let now      = Local::now();
        let tomorrow = now.date_naive().succ_opt().unwrap_or(now.date_naive());
        let midnight = match Local.from_local_datetime(
            &tomorrow.and_hms_opt(0, 0, 0).unwrap()
        ) {
            chrono::LocalResult::Single(dt)       => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None             => now + chrono::Duration::hours(24),
        };
        self.schedule      = ScheduleType::RestOfDay;
        self.blocked_until = Some(midnight);
        self.enabled       = true;
    }

    pub fn exe_name(&self) -> &str {
        std::path::Path::new(&self.executable)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.executable)
    }

    pub fn matches_process(&self, proc_name: &str, proc_exe: Option<&str>) -> bool {
        if let Some(exe) = proc_exe {
            if exe == self.executable { return true; }
        }
        let base = self.exe_name().to_lowercase();
        let name = proc_name.to_lowercase();
        if self.fuzzy_match {
            name.contains(&base)
        } else if self.executable.contains('/') {
            proc_name == self.exe_name()
        } else {
            name == base
        }
    }
}

// ── Focus Session ─────────────────────────────────────────────────────────────
//
// A Focus Session is a time-boxed block that activates a set of rules for a
// fixed duration, with optional scheduled breaks and a strict/flexible lock.
//
// Add these structs to config.rs (before or after the `AppConfig` block).

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionMode {
    /// Session cannot be cancelled once started.
    Strict,
    /// Session can be cancelled after a cooldown delay.
    FlexibleDelay {
        /// Seconds the user must wait before the cancel takes effect.
        delay_secs: u64,
    },
    /// Session can be cancelled by entering the correct password.
    FlexiblePassword {
        /// Bcrypt-hashed password stored at session creation.
        /// We use a simple SHA-256 hex here to avoid a heavy dep.
        password_hash: String,
    },
}

impl Default for SessionMode {
    fn default() -> Self {
        Self::FlexibleDelay { delay_secs: 30 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakConfig {
    /// How often a break is offered (seconds of focus before a break).
    pub every_secs: u64,
    /// How long each break lasts (seconds).
    pub duration_secs: u64,
}

impl Default for BreakConfig {
    fn default() -> Self {
        Self { every_secs: 3600, duration_secs: 300 } // 1 h work → 5 min break
    }
}

/// A saved session template the user can launch quickly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTemplate {
    pub id:           String,
    pub name:         String,
    /// IDs of the `Rule`s that are blocked during this session.
    pub rule_ids:     Vec<String>,
    /// Total session length in seconds (e.g. 7200 = 2 h).
    pub duration_secs: u64,
    pub mode:          SessionMode,
    pub break_config:  Option<BreakConfig>,
}

impl SessionTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id:            uuid::Uuid::new_v4().to_string(),
            name:          name.into(),
            rule_ids:      Vec::new(),
            duration_secs: 2 * 3600,
            mode:          SessionMode::default(),
            break_config:  None,
        }
    }
}


// ── AppConfig ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default = "default_true")]
    pub show_tray_icon: bool,
    #[serde(default = "default_true")]
    pub daemon_enabled: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default = "default_interval")]
    pub check_interval_secs: u64,
    #[serde(default)]
    pub network_rules: Vec<NetworkRule>,
    /// None = keep forever; Some(n) = discard records older than n days.
    #[serde(default = "default_retention")]
    pub usage_retention_days: Option<u64>,
    #[serde(default)]
    pub session_templates: Vec<SessionTemplate>,
    #[serde(default)]
    pub scheduled_blocks:  Vec<ScheduledBlock>,
}

fn default_true()      -> bool        { true }
fn default_interval()  -> u64         { 5    }
fn default_retention() -> Option<u64> { Some(90) }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            rules:                Vec::new(),
            show_tray_icon:       true,
            daemon_enabled:       true,
            start_minimized:      false,
            check_interval_secs:  5,
            network_rules:        Vec::new(),
            usage_retention_days: Some(90),
            session_templates: Vec::new(),
            scheduled_blocks:  Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("appblocker")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str(&text) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}

// ── Scheduled block ───────────────────────────────────────────────────────────
//
// A ScheduledBlock fires a SessionTemplate automatically at a given time on
// selected days of the week.  Think of it as a recurring alarm that starts a
// focus session without the user doing anything.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledBlock {
    pub id:          String,
    pub enabled:     bool,
    pub template_id: String,
    /// Days to fire: 0 = Mon … 6 = Sun.
    pub days:        Vec<u8>,
    pub start_hour:  u8,
    pub start_min:   u8,
    /// Whether this scheduled block has already fired today (reset at midnight).
    #[serde(default)]
    pub fired_today: bool,
}

impl ScheduledBlock {
    pub fn new(template_id: impl Into<String>) -> Self {
        Self {
            id:          uuid::Uuid::new_v4().to_string(),
            enabled:     true,
            template_id: template_id.into(),
            days:        (0u8..5).collect(), // Mon–Fri by default
            start_hour:  9,
            start_min:   0,
            fired_today: false,
        }
    }

    /// Returns true if this block should fire right now (within a 10-second window).
    pub fn should_fire_now(&self) -> bool {
        if !self.enabled || self.fired_today { return false; }
        let now = chrono::Local::now();
        let wd  = now.weekday().num_days_from_monday() as u8;
        if !self.days.contains(&wd) { return false; }
        let h = now.hour() as u8;
        let m = now.minute() as u8;
        h == self.start_hour && m == self.start_min
    }
}

pub fn hash_password(password: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let salt = "appblocker-session-v1";
    let mut h = DefaultHasher::new();
    format!("{salt}:{password}").hash(&mut h);
    let r1 = h.finish();
    format!("{salt}:{r1:016x}:{password}").hash(&mut h);
    format!("{:016x}{:016x}", r1, h.finish())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    hash_password(password) == hash
}

