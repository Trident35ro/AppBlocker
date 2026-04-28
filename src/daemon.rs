use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use chrono::Local;

use crate::config::{AppConfig, BlockingMethod, Rule, ScheduleType};
use crate::monitor::{ProcessInfo, ProcessMonitor};
use crate::{blocker, notification};

// ── Shared app state ──────────────────────────────────────────────────────────

pub struct AppState {
    pub config:               AppConfig,
    pub processes:            Vec<ProcessInfo>,
    pub last_process_update:  Option<Instant>,
    pub daemon_running:       bool,
    // rule_id → grace period expiry
    pub grace_timers:  HashMap<String, Instant>,
    // rule_id → whether warning notification was already sent
    pub grace_warned:  HashMap<String, bool>,
    // rule_id → when resource threshold was first exceeded
    pub resource_timers: HashMap<String, Instant>,
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

fn run(state: SharedState) {
    let mut monitor = ProcessMonitor::new();

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
            let mut s       = state.write().unwrap();
            s.processes          = procs.clone();
            s.last_process_update = Some(Instant::now());
        }

        enforce(&state, &procs);
        std::thread::sleep(Duration::from_secs(interval));
    }
}

// ── Rule enforcement ──────────────────────────────────────────────────────────

fn enforce(state: &SharedState, procs: &[ProcessInfo]) {
    let rules: Vec<Rule> = state.read().unwrap().config.rules.clone();

    for rule in &rules {
        if !rule.enabled {
            clear_all_timers(state, &rule.id);
            continue;
        }

        let matching: Vec<&ProcessInfo> = procs.iter()
            .filter(|p| rule.matches_process(&p.name, p.exe_path.as_deref()))
            .collect();

        if matching.is_empty() {
            clear_all_timers(state, &rule.id);
            continue;
        }

        log::debug!(
            "rule '{}' matched {} process(es): {}",
            rule.name,
            matching.len(),
            matching.iter().map(|p| format!("{}({})", p.name, p.pid)).collect::<Vec<_>>().join(", ")
        );

        let schedule_active  = rule.is_schedule_active();
        let resource_active  = check_resource(state, rule, &matching);

        if !schedule_active && !resource_active {
            clear_grace(state, &rule.id);
            continue;
        }

        if grace_expired(state, rule) {
            for p in &matching {
                do_block(rule, p);
            }
        }
    }

    expire_rest_of_day(state);
}

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

    let over_threshold = procs.iter().any(|p| {
        trigger.cpu_percent.map_or(false, |t| p.cpu_percent > t)
            || trigger.ram_mb.map_or(false, |t| p.mem_mb > t as f64)
    });

    if !over_threshold {
        state.write().unwrap().resource_timers.remove(&rule.id);
        return false;
    }

    let mut s = state.write().unwrap();
    let first_seen = s.resource_timers
        .entry(rule.id.clone())
        .or_insert_with(Instant::now);
    first_seen.elapsed().as_secs() >= trigger.duration_secs
}

/// Returns true once the grace window has elapsed (or if there is no grace).
fn grace_expired(state: &SharedState, rule: &Rule) -> bool {
    let secs = rule.grace_period.warn_before_block_secs;
    if secs == 0 { return true; }

    let now = Instant::now();
    let mut s = state.write().unwrap();

    if let Some(&end) = s.grace_timers.get(&rule.id) {
        return now >= end;
    }

    // First time we see this rule active — start the grace window.
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
                Ok(_)  => { log::info!("SIGTERM sent to PID {}", proc.pid); notification::send_blocked(&rule.name); }
                Err(e) => log::error!("kill PID {} failed: {e}", proc.pid),
            }
        }
        BlockingMethod::ForceKill => {
            match blocker::force_kill_process(proc.pid) {
                Ok(_)  => { log::info!("SIGKILL sent to PID {}", proc.pid); notification::send_blocked(&rule.name); }
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

