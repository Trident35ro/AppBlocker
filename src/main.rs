mod config;
mod monitor;
mod blocker;
mod daemon;
mod startup;
mod notification;
mod tray;
mod usage_tracker;
mod network;
mod ui;

use std::sync::{Arc, RwLock};
use daemon::AppState;
use tray::TrayFlags;

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let daemon_mode = args.iter().any(|a| a == "--daemon");

    let state = Arc::new(RwLock::new(AppState::new()));

    daemon::start_daemon(state.clone());

    let tray_flags = Arc::new(TrayFlags::new());

    if state.read().unwrap().config.show_tray_icon {
        tray::spawn_tray(
            tray_flags.show_window.clone(),
            tray_flags.quit.clone(),
        );
    }

    let start_hidden = daemon_mode
        || state.read().unwrap().config.start_minimized;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("AppBlocker")
            .with_inner_size([1200.0, 740.0])
            .with_min_inner_size([800.0, 540.0])
            .with_visible(!start_hidden),
        ..Default::default()
    };

    eframe::run_native(
        "AppBlocker",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(ui::AppBlockerApp::new(cc, state, tray_flags, start_hidden)))
        }),
    )
    .expect("failed to start AppBlocker");
}
