use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct TrayFlags {
    pub show_window: Arc<AtomicBool>,
    pub quit:        Arc<AtomicBool>,
}

impl TrayFlags {
    pub fn new() -> Self {
        Self {
            show_window: Arc::new(AtomicBool::new(false)),
            quit:        Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Spawns the KDE StatusNotifierItem tray icon in a background thread.
/// Keeps itself alive with an infinite sleep — returns immediately.
pub fn spawn_tray(show: Arc<AtomicBool>, quit: Arc<AtomicBool>) {
    use ksni::Tray;

    struct AppTray {
        show: Arc<AtomicBool>,
        quit: Arc<AtomicBool>,
    }

    impl Tray for AppTray {
        fn id(&self)        -> String { "appblocker".into() }
        fn title(&self)     -> String { "AppBlocker".into() }
        fn icon_name(&self) -> String { "security-high".into() }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::StandardItem;
            vec![
                StandardItem {
                    label: "Show AppBlocker".into(),
                    activate: Box::new(|this: &mut Self| {
                        this.show.store(true, Ordering::Relaxed);
                    }),
                    ..Default::default()
                }.into(),
                ksni::MenuItem::Separator,
                StandardItem {
                    label: "Quit".into(),
                    activate: Box::new(|this: &mut Self| {
                        this.quit.store(true, Ordering::Relaxed);
                        std::process::exit(0);
                    }),
                    ..Default::default()
                }.into(),
            ]
        }
    }

    std::thread::Builder::new()
        .name("appblocker-tray".into())
        .spawn(move || {
            let tray    = AppTray { show, quit };
            let service = ksni::TrayService::new(tray);
            let _handle = service.spawn(); // keep alive
            loop { std::thread::sleep(std::time::Duration::from_secs(60)); }
        })
        .expect("failed to spawn tray thread");
}
