use notify_rust::{Notification, Urgency};

pub fn send_blocked(app_name: &str) {
    let _ = Notification::new()
        .summary("AppBlocker")
        .body(&format!("{app_name} has been blocked."))
        .icon("security-high")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_block_warning(app_name: &str, in_minutes: u64) {
    let body = if in_minutes == 0 {
        format!("{app_name} will be blocked momentarily.")
    } else {
        let s = if in_minutes == 1 { "" } else { "s" };
        format!("{app_name} will be blocked in {in_minutes} minute{s}.")
    };
    let _ = Notification::new()
        .summary("AppBlocker — Upcoming Block")
        .body(&body)
        .icon("dialog-warning")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_unblock_warning(app_name: &str, in_minutes: u64) {
    let body = if in_minutes == 0 {
        format!("{app_name} will be unblocked momentarily.")
    } else {
        let s = if in_minutes == 1 { "" } else { "s" };
        format!("{app_name} will be unblocked in {in_minutes} minute{s}.")
    };
    let _ = Notification::new()
        .summary("AppBlocker")
        .body(&body)
        .icon("security-high")
        .urgency(Urgency::Low)
        .show();
}

pub fn send_time_limit_warning(app_name: &str, remaining_mins: u64) {
    let body = if remaining_mins == 0 {
        format!("You've reached your daily limit for {app_name}.")
    } else {
        let s = if remaining_mins == 1 { "" } else { "s" };
        format!("You have {remaining_mins} minute{s} left on {app_name} today.")
    };
    let _ = Notification::new()
        .summary("AppBlocker — Time Limit")
        .body(&body)
        .icon("appointment-soon")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_time_limit_reached(app_name: &str, hard_block: bool) {
    let body = if hard_block {
        format!("Daily limit reached — {app_name} has been blocked.")
    } else {
        format!("Daily limit reached for {app_name}. Consider closing it.")
    };
    let _ = Notification::new()
        .summary("AppBlocker — Daily Limit Reached")
        .body(&body)
        .icon("dialog-warning")
        .urgency(Urgency::Critical)
        .show();
}

// ── Session notifications — append to notification.rs ────────────────────────

pub fn send_session_started(name: &str, duration_mins: u64) {
    let s = if duration_mins == 1 { "" } else { "s" };
    let _ = Notification::new()
        .summary("AppBlocker — Focus Session Started")
        .body(&format!("🎯 \"{name}\" session started ({duration_mins} minute{s})."))
        .icon("appointment-new")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_session_ended(name: &str) {
    let _ = Notification::new()
        .summary("AppBlocker — Session Complete")
        .body(&format!("✅ \"{name}\" focus session finished. Great work!"))
        .icon("appointment-missed")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_session_cancelled(name: &str) {
    let _ = Notification::new()
        .summary("AppBlocker — Session Cancelled")
        .body(&format!("\"{}\" session was cancelled.", name))
        .icon("appointment-missed")
        .urgency(Urgency::Low)
        .show();
}

pub fn send_break_started(session_name: &str, duration_mins: u64) {
    let s = if duration_mins == 1 { "" } else { "s" };
    let _ = Notification::new()
        .summary("AppBlocker — Break Time!")
        .body(&format!("☕ Break started for \"{session_name}\" ({duration_mins} minute{s}). Blocked apps are temporarily allowed."))
        .icon("appointment-soon")
        .urgency(Urgency::Normal)
        .show();
}

pub fn send_break_ended(session_name: &str) {
    let _ = Notification::new()
        .summary("AppBlocker — Back to Focus")
        .body(&format!("🎯 Break over — \"{session_name}\" focus mode resumed."))
        .icon("appointment-new")
        .urgency(Urgency::Normal)
        .show();
}

