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
