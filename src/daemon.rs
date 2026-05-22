use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::Local;

use crate::config::{AppConfig, BlockingMethod, Rule, RuleAction, ScheduleType, TimeLimit};
use crate::config::{BreakConfig, SessionMode, SessionTemplate};
use crate::monitor::{ProcessInfo, ProcessMonitor};
use crate::usage_tracker::UsageData;
use crate::{blocker, notification};

// ── 1. New struct — place before `AppState` ───────────────────────────────────

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub template_id:   String,
    pub template_name: String,
    /// IDs of rules that are *additionally* force-enabled for this session.
    pub rule_ids:      Vec<String>,
    pub mode:          SessionMode,
    /// Wall-clock instant the session started.
    pub started_at:    Instant,
    /// Total duration of the session.
    pub duration:      Duration,
    /// Break configuration, if any.
    pub break_config:  Option<BreakConfig>,
    /// When the current break ends (None = not on a break).
    pub break_until:   Option<Instant>,
    /// When the next break starts (None = no breaks configured).
    pub next_break_at: Option<Instant>,
    // ── Cancel state (Flexible modes only) ────────────────────────────────
    /// When a cancel request was made (for FlexibleDelay).
    pub cancel_requested_at: Option<Instant>,
}

impl ActiveSession {
    pub fn new(tmpl: &SessionTemplate) -> Self {
        let next_break_at = tmpl.break_config.as_ref().map(|b| {
            Instant::now() + Duration::from_secs(b.every_secs)
        });
        Self {
            template_id:        tmpl.id.clone(),
            template_name:      tmpl.name.clone(),
            rule_ids:           tmpl.rule_ids.clone(),
            mode:               tmpl.mode.clone(),
            started_at:         Instant::now(),
            duration:           Duration::from_secs(tmpl.duration_secs),
            break_config:       tmpl.break_config.clone(),
            break_until:        None,
            next_break_at,
            cancel_requested_at: None,
        }
    }

    pub fn elapsed(&self) -> Duration { self.started_at.elapsed() }
    pub fn remaining(&self) -> Duration {
        self.duration.checked_sub(self.elapsed()).unwrap_or_default()
    }
    pub fn is_expired(&self) -> bool { self.elapsed() >= self.duration }
    pub fn on_break(&self) -> bool {
        self.break_until.map(|t| Instant::now() < t).unwrap_or(false)
    }
    pub fn break_remaining(&self) -> Option<Duration> {
        self.break_until.map(|t| t.checked_duration_since(Instant::now()).unwrap_or_default())
    }
    pub fn next_break_in(&self) -> Option<Duration> {
        if self.on_break() { return None; }
        self.next_break_at.map(|t| t.checked_duration_since(Instant::now()).unwrap_or_default())
    }
}

// ── Shared app state ──────────────────────────────────────────────────────────

pub struct AppState {
    pub config:               AppConfig,
    pub processes:            Vec<ProcessInfo>,
    pub last_process_update:  Option<Instant>,
    pub daemon_running:       bool,
    pub grace_timers:         HashMap<String, Instant>,
    pub grace_warned:         HashMap<String, bool>,
    pub resource_timers:      HashMap<String, Instant>,
    // Time limits (in-memory, not persisted)
    pub daily_usage_secs:     HashMap<String, u64>,
    pub time_limit_warned:    HashMap<String, Vec<u64>>,
    pub last_usage_reset:     HashMap<String, Instant>,
    // Mindful mode throttle
    pub mindful_last_prompt:  HashMap<String, Instant>,
    pub active_session: Option<ActiveSession>,
    /// Pending cancel for FlexibleDelay — counts down to zero then cancels.
    pub cancel_countdown: Option<Instant>,
}

impl AppState {
    pub fn new() -> Self {
        let config         = AppConfig::load();
        let daemon_running = config.daemon_enabled;
        Self {
            config,
            processes:           Vec::new(),
            last_process_update: None,
            daemon_running,
            grace_timers:        HashMap::new(),
            grace_warned:        HashMap::new(),
            resource_timers:     HashMap::new(),
            daily_usage_secs:    HashMap::new(),
            time_limit_warned:   HashMap::new(),
            last_usage_reset:    HashMap::new(),
            mindful_last_prompt: HashMap::new(),
            active_session:      None,
            cancel_countdown:    None,
        }
    }

    pub fn save_config(&self) {
        if let Err(e) = self.config.save() {
            log::error!("config save failed: {e}");
        }
    }
}

pub type SharedState = Arc<RwLock<AppState>>;

// ── Background enforcement thread ─────────────────────────────────────────────

pub fn start_daemon(state: SharedState) {
    std::thread::Builder::new()
        .name("appblocker-daemon".into())
        .spawn(move || run(state))
        .expect("failed to spawn daemon thread");
}

// ── 5. Scheduled block firing — add inside `run()` loop ──────────────────────
//
// Add this call at the bottom of the `run()` loop, before the sleep:

fn fire_scheduled_blocks(state: &SharedState) {
    if state.read().unwrap().active_session.is_some() { return; }

    let now = chrono::Local::now();

    // Reset fired_today flags at midnight
    {
        let mut s = state.write().unwrap();
        for sb in &mut s.config.scheduled_blocks {
            if now.hour() == 0 && now.minute() == 0 {
                sb.fired_today = false;
            }
        }
    }

    let to_fire: Vec<String> = {
        let s = state.read().unwrap();
        s.config.scheduled_blocks.iter()
            .filter(|sb| sb.should_fire_now())
            .map(|sb| sb.template_id.clone())
            .collect()
    };

    for tmpl_id in to_fire {
        let tmpl = {
            let s = state.read().unwrap();
            s.config.session_templates.iter().find(|t| t.id == tmpl_id).cloned()
        };
        if let Some(tmpl) = tmpl {
            start_session(state, &tmpl);
            let mut s = state.write().unwrap();
            for sb in &mut s.config.scheduled_blocks {
                if sb.template_id == tmpl_id { sb.fired_today = true; }
            }
        }
    }
}

fn run(state: SharedState) {
    let mut monitor     = ProcessMonitor::new();
    let mut usage_cache: HashMap<String, UsageData> = HashMap::new();
    let mut last_flush  = Instant::now();

    loop {
        let (running, interval) = {
            let s = state.read().unwrap();
            (s.daemon_running, s.config.check_interval_secs)
        };

        if !running {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        let procs = monitor.scan();
        {
            let mut s = state.write().unwrap();
            s.processes           = procs.clone();
            s.last_process_update = Some(Instant::now());
        }

        // Session enforcement must come AFTER procs is defined
        enforce_session(&state, &procs);
        fire_scheduled_blocks(&state);

        enforce(&state, &procs, interval, &mut usage_cache);

        if last_flush.elapsed() >= Duration::from_secs(60) {
            for data in usage_cache.values() { data.save(); }
            last_flush = Instant::now();
        }

        std::thread::sleep(Duration::from_secs(interval));
    }
}

// ── 4. Session enforcement — add inside `enforce()` ──────────────────────────
//
// Call this at the TOP of `enforce()`, before the existing rule loop:

// Replace the entire enforce_session() function in daemon.rs with this:

fn enforce_session(state: &SharedState, procs: &[crate::monitor::ProcessInfo]) {

    // ── Step 1: tick the session state, decide what notification to send ──
    // We do everything under ONE write lock, extract what we need, then drop
    // before calling any notification (which may block).

    #[derive(Debug)]
    enum SessionTick {
        BreakEnded(String),
        BreakStarted(String, u64),   // (name, duration_mins)
        Expired(String),
        CancelDelayExpired(String),
        Ongoing { rule_ids: Vec<String>, on_break: bool },
        NoSession,
    }

    let tick = {
        let mut s = state.write().unwrap();

        let session = match &mut s.active_session {
            None    => { return; }
            Some(s) => s,
        };

        let now = Instant::now();

        // Session expired?
        if session.is_expired() {
            let name = session.template_name.clone();
            s.active_session = None;
            SessionTick::Expired(name)

        // Cancel delay expired?
        } else if let SessionMode::FlexibleDelay { delay_secs } = &session.mode.clone() {
            if let Some(req_at) = session.cancel_requested_at {
                if req_at.elapsed() >= Duration::from_secs(*delay_secs) {
                    let name = session.template_name.clone();
                    s.active_session = None;
                    SessionTick::CancelDelayExpired(name)
                } else {
                    // Still counting down — treat as ongoing
                    let ids    = session.rule_ids.clone();
                    let on_brk = session.on_break();
                    SessionTick::Ongoing { rule_ids: ids, on_break: on_brk }
                }
            } else {
                let ids    = session.rule_ids.clone();
                let on_brk = session.on_break();
                SessionTick::Ongoing { rule_ids: ids, on_break: on_brk }
            }

        // Break just ended?
        } else if let Some(until) = session.break_until {
            if now >= until {
                session.break_until = None;
                session.next_break_at = session.break_config.as_ref().map(|b| {
                    now + Duration::from_secs(b.every_secs)
                });
                let name = session.template_name.clone();
                SessionTick::BreakEnded(name)
            } else {
                // Still on break
                let ids = session.rule_ids.clone();
                SessionTick::Ongoing { rule_ids: ids, on_break: true }
            }

        // Time to start a break?
        } else if let Some(next) = session.next_break_at {
            if now >= next {
                let dur = session.break_config.as_ref()
                    .map(|b| b.duration_secs).unwrap_or(300);
                session.break_until   = Some(now + Duration::from_secs(dur));
                session.next_break_at = None;
                let name = session.template_name.clone();
                SessionTick::BreakStarted(name, dur / 60)
            } else {
                let ids    = session.rule_ids.clone();
                let on_brk = session.on_break();
                SessionTick::Ongoing { rule_ids: ids, on_break: on_brk }
            }

        } else {
            let ids    = session.rule_ids.clone();
            let on_brk = session.on_break();
            SessionTick::Ongoing { rule_ids: ids, on_break: on_brk }
        }
        // write lock is dropped here
    };

    // ── Step 2: send notifications (lock is fully released) ───────────────
    match tick {
        SessionTick::NoSession => {}

        SessionTick::Expired(name) => {
            crate::notification::send_session_ended(&name);
        }

        SessionTick::CancelDelayExpired(name) => {
            crate::notification::send_session_cancelled(&name);
        }

        SessionTick::BreakEnded(name) => {
            crate::notification::send_break_ended(&name);
            // Re-run next tick to immediately enforce focus blocking.
        }

        SessionTick::BreakStarted(name, mins) => {
            crate::notification::send_break_started(&name, mins);
        }

        SessionTick::Ongoing { rule_ids, on_break } => {
            if on_break { return; } // apps are free during break

            // ── Step 3: enforce blocking for session rules ─────────────
            let rules: Vec<crate::config::Rule> = {
                let s = state.read().unwrap();
                s.config.rules.iter()
                    .filter(|r| rule_ids.contains(&r.id))
                    .cloned()
                    .collect()
            };

            for rule in &rules {
                let matching: Vec<_> = procs.iter()
                    .filter(|p| rule.matches_process(&p.name, p.exe_path.as_deref()))
                    .collect();
                for p in matching {
                    do_block(rule, p);
                }
            }
        }
    }
}

// ── Rule enforcement ──────────────────────────────────────────────────────────

fn enforce(
    state:       &SharedState,
    procs:       &[ProcessInfo],
    interval:    u64,
    usage_cache: &mut HashMap<String, UsageData>,
) {
    let (rules, retention_days): (Vec<Rule>, Option<u64>) = {
        let s = state.read().unwrap();
        (s.config.rules.clone(), s.config.usage_retention_days)
    };

    let now       = Local::now();
    let now_date  = now.date_naive();
    let now_hour  = now.hour() as u8;

    for rule in &rules {
        if !rule.enabled {
            clear_all_timers(state, &rule.id);
            continue;
        }

        let matching: Vec<&ProcessInfo> = procs.iter()
            .filter(|p| rule.matches_process(&p.name, p.exe_path.as_deref()))
            .collect();

        // ── Usage tracking (always, even for mindful rules) ────────────────
        if (rule.track_usage || rule.time_limit.is_some()) && !matching.is_empty() {
            // In-memory daily counter
            add_daily_usage(state, &rule.id, interval);

            // Disk usage cache
            if rule.track_usage {
                let data = usage_cache.entry(rule.id.clone()).or_insert_with(|| {
                    let mut d = UsageData::load(&rule.id);
                    d.rule_name = rule.name.clone();
                    if let Some(days) = retention_days { d.cleanup(days); }
                    d
                });
                data.rule_name = rule.name.clone();
                data.add_secs(now_date, now_hour, interval);
            }
        }

        // ── Time limit ─────────────────────────────────────────────────────
        let time_limit_block = if let Some(limit) = &rule.time_limit {
            check_time_limit_reset(state, &rule.id, limit);
            if !matching.is_empty() {
                fire_time_limit_reminders(state, rule, limit, interval);
            }
            let usage = get_daily_usage(state, &rule.id);
            limit.hard_block && usage >= limit.daily_limit_secs
        } else { false };

        // ── Mindful mode ───────────────────────────────────────────────────
        if rule.rule_action == RuleAction::Mindful {
            if rule.mindful_intercept_running && !matching.is_empty() {
                maybe_mindful_prompt(state, &rule.id, &rule.name);
            }
            if matching.is_empty() { clear_all_timers(state, &rule.id); }
            continue;
        }

        // ── Block mode ─────────────────────────────────────────────────────
        if matching.is_empty() {
            clear_all_timers(state, &rule.id);
            continue;
        }

        let schedule_active = rule.is_schedule_active();
        let unavail_active  = rule.is_unavail_active();
        let resource_active = check_resource(state, rule, &matching);

        if !schedule_active && !unavail_active && !resource_active && !time_limit_block {
            clear_grace(state, &rule.id);
            continue;
        }

        log::debug!(
            "rule '{}' matched {} process(es)",
            rule.name,
            matching.len()
        );

        if grace_expired(state, rule) {
            for p in &matching {
                do_block(rule, p);
            }
        }
    }

    expire_rest_of_day(state);
}

// ── Usage helpers ─────────────────────────────────────────────────────────────

fn add_daily_usage(state: &SharedState, rule_id: &str, secs: u64) {
    let mut s = state.write().unwrap();
    *s.daily_usage_secs.entry(rule_id.to_owned()).or_insert(0) += secs;
}

fn get_daily_usage(state: &SharedState, rule_id: &str) -> u64 {
    *state.read().unwrap().daily_usage_secs.get(rule_id).unwrap_or(&0)
}

fn check_time_limit_reset(state: &SharedState, rule_id: &str, limit: &TimeLimit) {
    let reset_today = chrono::NaiveTime::from_hms_opt(
        limit.reset_hour as u32, limit.reset_min as u32, 0,
    )
    .and_then(|t| Local::now().date_naive().and_time(t).and_local_timezone(Local).single());

    let Some(reset_dt) = reset_today else { return };
    let now = Local::now();

    let should_reset = {
        let s = state.read().unwrap();
        match s.last_usage_reset.get(rule_id) {
            Some(&last_instant) => {
                // last_instant is Instant; compare via duration to wall clock
                // We track reset by "last reset was before today's reset time"
                // Use a simple approach: if daily_usage_secs is 0 and we'd reset, skip
                now >= reset_dt && last_instant.elapsed().as_secs() >= 23 * 3600
            }
            None => now >= reset_dt,
        }
    };

    if should_reset {
        let mut s = state.write().unwrap();
        s.daily_usage_secs.insert(rule_id.to_owned(), 0);
        s.last_usage_reset.insert(rule_id.to_owned(), Instant::now());
        s.time_limit_warned.remove(rule_id);
        log::debug!("daily usage reset for rule {rule_id}");
    }
}

fn fire_time_limit_reminders(state: &SharedState, rule: &Rule, limit: &TimeLimit, _interval: u64) {
    let usage   = get_daily_usage(state, &rule.id);
    let cap     = limit.daily_limit_secs;
    if usage >= cap {
        // Limit reached — fire once
        let already = state.read().unwrap()
            .time_limit_warned.get(&rule.id)
            .map(|v| v.contains(&0))
            .unwrap_or(false);
        if !already {
            state.write().unwrap()
                .time_limit_warned.entry(rule.id.clone())
                .or_default()
                .push(0);
            notification::send_time_limit_reached(&rule.name, limit.hard_block);
        }
        return;
    }
    let remaining = cap - usage;
    for &threshold in &limit.remind_thresholds {
        if remaining <= threshold {
            let already = state.read().unwrap()
                .time_limit_warned.get(&rule.id)
                .map(|v| v.contains(&threshold))
                .unwrap_or(false);
            if !already {
                state.write().unwrap()
                    .time_limit_warned.entry(rule.id.clone())
                    .or_default()
                    .push(threshold);
                notification::send_time_limit_warning(&rule.name, remaining / 60);
            }
        }
    }
}

// ── Mindful prompt ────────────────────────────────────────────────────────────

fn maybe_mindful_prompt(state: &SharedState, rule_id: &str, rule_name: &str) {
    let should_prompt = {
        let s = state.read().unwrap();
        match s.mindful_last_prompt.get(rule_id) {
            Some(&last) => last.elapsed() >= Duration::from_secs(1800),
            None        => true,
        }
    };
    if !should_prompt { return; }
    state.write().unwrap()
        .mindful_last_prompt.insert(rule_id.to_owned(), Instant::now());

    let name = rule_name.to_owned();
    std::thread::spawn(move || {
        let output = std::process::Command::new("kdialog")
            .args([
                "--title", "AppBlocker — Mindful Check",
                "--inputbox",
                &format!("You're using {name}.\n\nWhat are you using it for?"),
            ])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                let reason = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !reason.is_empty() {
                    log_mindful_reason(&name, &reason);
                }
            }
        }
    });
}

fn log_mindful_reason(app_name: &str, reason: &str) {
    let path = mindful_log_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    let entry = format!(
        "[{}] {}: {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        app_name,
        reason,
    );
    let _ = std::fs::OpenOptions::new()
        .create(true).append(true)
        .open(&path)
        .map(|mut f| { use std::io::Write; f.write_all(entry.as_bytes()) });
}

pub fn mindful_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("appblocker")
        .join("mindful_log.txt")
}

// ── Timer helpers ─────────────────────────────────────────────────────────────

fn clear_all_timers(state: &SharedState, id: &str) {
    let mut s = state.write().unwrap();
    s.grace_timers.remove(id);
    s.grace_warned.remove(id);
    s.resource_timers.remove(id);
}

fn clear_grace(state: &SharedState, id: &str) {
    let mut s = state.write().unwrap();
    s.grace_timers.remove(id);
    s.grace_warned.remove(id);
}

fn check_resource(state: &SharedState, rule: &Rule, procs: &[&ProcessInfo]) -> bool {
    let trigger = match &rule.resource_trigger {
        Some(t) => t.clone(),
        None    => return false,
    };

    let over = procs.iter().any(|p| {
        trigger.cpu_percent.map_or(false, |t| p.cpu_percent > t)
            || trigger.ram_mb.map_or(false, |t| p.mem_mb > t as f64)
    });

    if !over {
        state.write().unwrap().resource_timers.remove(&rule.id);
        return false;
    }

    let mut s   = state.write().unwrap();
    let first   = s.resource_timers.entry(rule.id.clone()).or_insert_with(Instant::now);
    first.elapsed().as_secs() >= trigger.duration_secs
}

fn grace_expired(state: &SharedState, rule: &Rule) -> bool {
    let secs = rule.grace_period.warn_before_block_secs;
    if secs == 0 { return true; }

    let now = Instant::now();
    let mut s = state.write().unwrap();

    if let Some(&end) = s.grace_timers.get(&rule.id) {
        return now >= end;
    }

    s.grace_timers.insert(rule.id.clone(), now + Duration::from_secs(secs));

    if !*s.grace_warned.entry(rule.id.clone()).or_insert(false) {
        s.grace_warned.insert(rule.id.clone(), true);
        let (name, mins) = (rule.name.clone(), secs / 60);
        drop(s);
        notification::send_block_warning(&name, mins);
    }

    false
}

fn do_block(rule: &Rule, proc: &ProcessInfo) {
    log::info!("blocking '{}' — PID {} ({})", rule.name, proc.pid, proc.name);
    match &rule.blocking_method {
        BlockingMethod::Kill => {
            match blocker::kill_process(proc.pid) {
                Ok(_)  => { log::info!("SIGTERM → PID {}", proc.pid); notification::send_blocked(&rule.name); }
                Err(e) => log::error!("kill PID {} failed: {e}", proc.pid),
            }
        }
        BlockingMethod::ForceKill => {
            match blocker::force_kill_process(proc.pid) {
                Ok(_)  => { log::info!("SIGKILL → PID {}", proc.pid); notification::send_blocked(&rule.name); }
                Err(e) => log::error!("force-kill PID {} failed: {e}", proc.pid),
            }
        }
        BlockingMethod::Wrapper => {
            let _ = blocker::set_wrapper_state(rule.exe_name(), true);
            let _ = blocker::force_kill_process(proc.pid);
            notification::send_blocked(&rule.name);
        }
        BlockingMethod::Network => {
            if let Err(e) = blocker::install_network_block(&rule.executable) {
                log::error!("network block for '{}' failed: {e}", rule.name);
            }
        }
    }
}

fn expire_rest_of_day(state: &SharedState) {
    let now = Local::now();
    let mut s = state.write().unwrap();
    for rule in &mut s.config.rules {
        if matches!(rule.schedule, ScheduleType::RestOfDay) {
            if let Some(until) = rule.blocked_until {
                if now >= until {
                    rule.blocked_until = None;
                    rule.enabled       = false;
                }
            }
        }
    }
}

// chrono import needed for hour()
use chrono::Timelike;

// ── 3. Public session control API ─────────────────────────────────────────────
//
// Add these free functions (they take SharedState so they're callable from UI).

/// Start a session from a saved template. Fails silently if one is already running.
pub fn start_session(state: &SharedState, template: &SessionTemplate) {
    let mut s = state.write().unwrap();
    if s.active_session.is_some() { return; }
    log::info!("starting focus session '{}'", template.name);
    s.active_session = Some(ActiveSession::new(template));
    // Force-enable all rules in the session.
    for id in &template.rule_ids {
        if let Some(r) = s.config.rules.iter_mut().find(|r| &r.id == id) {
            r.enabled = true;
        }
    }
    crate::notification::send_session_started(&template.name, template.duration_secs / 60);
}

/// Request cancellation of the running session.
/// - Strict: always fails (returns false).
/// - FlexibleDelay: arms a countdown; returns true once countdown expires.
/// - FlexiblePassword: verifies password; returns true immediately if correct.
pub fn request_cancel(state: &SharedState, password_attempt: Option<&str>) -> CancelResult {
    let mut s = state.write().unwrap();
    let session = match &mut s.active_session {
        Some(s) => s,
        None    => return CancelResult::NoSession,
    };

    match &session.mode.clone() {
        SessionMode::Strict => CancelResult::Denied,

        SessionMode::FlexiblePassword { password_hash } => {
            let attempt = password_attempt.unwrap_or("");
            if crate::config::verify_password(attempt, password_hash) {
                log::info!("session cancelled via password");
                s.active_session = None;
                CancelResult::Cancelled
            } else {
                CancelResult::WrongPassword
            }
        }

        SessionMode::FlexibleDelay { delay_secs } => {
            let delay = Duration::from_secs(*delay_secs);
            if let Some(req_at) = session.cancel_requested_at {
                if req_at.elapsed() >= delay {
                    log::info!("session cancel delay expired — cancelling");
                    s.active_session = None;
                    return CancelResult::Cancelled;
                }
                let remaining = delay.checked_sub(req_at.elapsed()).unwrap_or_default();
                CancelResult::PendingDelay { remaining_secs: remaining.as_secs() }
            } else {
                session.cancel_requested_at = Some(Instant::now());
                CancelResult::PendingDelay { remaining_secs: delay.as_secs() }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CancelResult {
    NoSession,
    Cancelled,
    Denied,
    WrongPassword,
    PendingDelay { remaining_secs: u64 },
}

