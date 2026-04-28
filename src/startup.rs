use anyhow::Result;
use std::path::PathBuf;

// ── Systemd user service for the AppBlocker daemon ───────────────────────────

fn service_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("systemd/user")
}

fn service_path() -> PathBuf {
    service_dir().join("appblocker.service")
}

pub fn install_daemon_service() -> Result<()> {
    let exe = std::env::current_exe()?;
    std::fs::create_dir_all(service_dir())?;

    let unit = format!(
        "[Unit]\n\
         Description=AppBlocker Daemon\n\
         After=graphical-session.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} --daemon\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=graphical-session.target\n",
        exe = exe.display(),
    );

    std::fs::write(service_path(), unit)?;
    systemctl(&["--user", "daemon-reload"])?;
    systemctl(&["--user", "enable", "appblocker.service"])?;
    Ok(())
}

pub fn remove_daemon_service() -> Result<()> {
    let _ = systemctl(&["--user", "disable", "--now", "appblocker.service"]);
    let p = service_path();
    if p.exists() { std::fs::remove_file(p)?; }
    let _ = systemctl(&["--user", "daemon-reload"]);
    Ok(())
}

pub fn is_service_installed() -> bool { service_path().exists() }

pub fn is_service_running() -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", "appblocker.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl(args: &[&str]) -> Result<()> {
    let out = std::process::Command::new("systemctl").args(args).output()?;
    if !out.status.success() {
        log::warn!("systemctl {}: {}", args.join(" "),
                   String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

// ── XDG autostart for individual applications ─────────────────────────────────

fn autostart_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("autostart")
}

fn autostart_path(rule_name: &str) -> PathBuf {
    autostart_dir().join(format!("appblocker-{}.desktop", rule_name.to_lowercase()))
}

pub fn install_app_autostart(rule_name: &str, executable: &str) -> Result<()> {
    std::fs::create_dir_all(autostart_dir())?;
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Exec={exe}\n\
         Hidden=false\n\
         NoDisplay=false\n\
         X-GNOME-Autostart-enabled=true\n",
        name = rule_name,
        exe  = executable,
    );
    std::fs::write(autostart_path(rule_name), content)?;
    Ok(())
}

pub fn remove_app_autostart(rule_name: &str) -> Result<()> {
    let p = autostart_path(rule_name);
    if p.exists() { std::fs::remove_file(p)?; }
    Ok(())
}

pub fn is_app_autostartd(rule_name: &str) -> bool {
    autostart_path(rule_name).exists()
}
