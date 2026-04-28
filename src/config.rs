use chrono::{DateTime, Local, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

// ── Blocking method ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockingMethod {
    /// Send SIGTERM — lets the app clean up before exiting (recommended).
    Kill,
    /// Send SIGKILL — immediate termination, no cleanup (pkill -9 equivalent).
    ForceKill,
    /// Install a ~/.local/bin wrapper that checks a state file before exec.
    Wrapper,
    /// Add an nftables output rule for the current UID (requires pkexec/root).
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

// ── Schedule ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimeRangeConfig {
    pub start_hour: u8,
    pub start_min:  u8,
    pub end_hour:   u8,
    pub end_min:    u8,
}

impl TimeRangeConfig {
    pub fn is_active_now(&self) -> bool {
        let now  = Local::now().time();
        let h_s  = self.start_hour as u32;
        let m_s  = self.start_min  as u32;
        let h_e  = self.end_hour   as u32;
        let m_e  = self.end_min    as u32;
        let start = chrono::NaiveTime::from_hms_opt(h_s, m_s, 0).unwrap_or_default();
        let end   = chrono::NaiveTime::from_hms_opt(h_e, m_e, 0).unwrap_or_default();
        if start <= end { now >= start && now < end } else { now >= start || now < end }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScheduleType {
    Always,
    TimeRange(TimeRangeConfig),
    /// Active until midnight; `blocked_until` stores the exact timestamp.
    RestOfDay,
}

impl std::fmt::Display for ScheduleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Always => write!(f, "Always"),
            Self::TimeRange(r) => write!(
                f, "{:02}:{:02} – {:02}:{:02}",
                r.start_hour, r.start_min, r.end_hour, r.end_min,
            ),
            Self::RestOfDay => write!(f, "Rest of Day"),
        }
    }
}

// ── Resource trigger ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTrigger {
    pub cpu_percent:   Option<f32>,
    pub ram_mb:        Option<u64>,
    /// How long the threshold must be exceeded before blocking.
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

// ── Rule ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id:                    String,
    pub name:                  String,
    pub executable:            String,
    pub blocking_method:       BlockingMethod,
    pub enabled:               bool,
    pub schedule:              ScheduleType,
    pub grace_period:          GracePeriod,
    pub startup_action:        StartupAction,
    pub resource_trigger:      Option<ResourceTrigger>,
    /// false = rule is discarded on next launch (session-only).
    pub persist_across_reboots: bool,
    /// Populated when schedule == RestOfDay; midnight of today.
    #[serde(default)]
    pub blocked_until: Option<DateTime<Local>>,
}

impl Rule {
    pub fn new(name: impl Into<String>, executable: impl Into<String>) -> Self {
        Self {
            id:                    Uuid::new_v4().to_string(),
            name:                  name.into(),
            executable:            executable.into(),
            blocking_method:       BlockingMethod::default(),
            enabled:               true,
            schedule:              ScheduleType::Always,
            grace_period:          GracePeriod::default(),
            startup_action:        StartupAction::default(),
            resource_trigger:      None,
            persist_across_reboots: true,
            blocked_until:         None,
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

    pub fn block_rest_of_day(&mut self) {
        let now      = Local::now();
        let tomorrow = now.date_naive().succ_opt().unwrap_or(now.date_naive());
        let midnight = match Local.from_local_datetime(
            &tomorrow.and_hms_opt(0, 0, 0).unwrap()
        ) {
            chrono::LocalResult::Single(dt)      => dt,
            chrono::LocalResult::Ambiguous(dt, _) => dt,
            chrono::LocalResult::None             => now + chrono::Duration::hours(24),
        };
        self.schedule     = ScheduleType::RestOfDay;
        self.blocked_until = Some(midnight);
        self.enabled      = true;
    }

    /// Basename of the executable path.
    pub fn exe_name(&self) -> &str {
        std::path::Path::new(&self.executable)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.executable)
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
}

fn default_true()     -> bool { true }
fn default_interval() -> u64  { 5    }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            rules:               Vec::new(),
            show_tray_icon:      true,
            daemon_enabled:      true,
            start_minimized:     false,
            check_interval_secs: 5,
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
