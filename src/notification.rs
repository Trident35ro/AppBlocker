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
