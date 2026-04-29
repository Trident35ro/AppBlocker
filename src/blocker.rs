use anyhow::{Context, Result};
use std::path::PathBuf;

// ── Kill ──────────────────────────────────────────────────────────────────────

pub fn kill_process(pid: i32) -> Result<()> {
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 { Ok(()) }
    else { Err(std::io::Error::last_os_error().into()) }
}

pub fn force_kill_process(pid: i32) -> Result<()> {
    if unsafe { libc::kill(pid, libc::SIGKILL) } == 0 { Ok(()) }
    else { Err(std::io::Error::last_os_error().into()) }
}

// ── PATH Wrapper ──────────────────────────────────────────────────────────────

fn wrapper_state_path(exe_name: &str) -> PathBuf {
    dirs::runtime_dir()
        .or_else(|| dirs::cache_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("appblocker")
        .join(format!("{exe_name}.state"))
}

fn wrapper_bin_path(exe_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".local/bin")
        .join(exe_name)
}

/// Install a blocking wrapper (denies launch when state == "blocked").
pub fn install_wrapper(executable: &str) -> Result<()> {
    let exe_name  = exe_name_of(executable)?;
    let local_bin = dirs::home_dir().unwrap_or_default().join(".local/bin");
    std::fs::create_dir_all(&local_bin)?;

    let state = wrapper_state_path(&exe_name);
    let script = format!(
        r#"#!/bin/sh
# AppBlocker wrapper — do not remove
STATE="{state}"
if [ -f "$STATE" ] && [ "$(cat "$STATE" 2>/dev/null)" = "blocked" ]; then
    notify-send "AppBlocker" "{exe_name} is currently blocked" --icon=dialog-error 2>/dev/null || true
    exit 1
fi
exec "{executable}" "$@"
"#,
        state      = state.display(),
        exe_name   = exe_name,
        executable = executable,
    );
    write_executable(&wrapper_bin_path(&exe_name), &script)
}

/// Install a mindful wrapper: asks the user "why?" before launching and logs the reason.
pub fn install_mindful_wrapper(executable: &str) -> Result<()> {
    let exe_name  = exe_name_of(executable)?;
    let local_bin = dirs::home_dir().unwrap_or_default().join(".local/bin");
    std::fs::create_dir_all(&local_bin)?;

    let log_path = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("appblocker")
        .join("mindful_log.txt");

    let script = format!(
        r#"#!/bin/sh
# AppBlocker mindful wrapper — do not remove
APP="{exe_name}"
LOG="{log}"
REASON=$(kdialog --title "AppBlocker — Mindful Check" \
    --inputbox "Why do you want to open $APP?" 2>/dev/null)
RC=$?
if [ $RC -ne 0 ] || [ -z "$REASON" ]; then
    exit 0
fi
mkdir -p "$(dirname "$LOG")"
printf '[%s] %s: %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$APP" "$REASON" >> "$LOG"
exec "{executable}" "$@"
"#,
        exe_name   = exe_name,
        log        = log_path.display(),
        executable = executable,
    );
    write_executable(&wrapper_bin_path(&exe_name), &script)
}

fn write_executable(path: &PathBuf, script: &str) -> Result<()> {
    std::fs::write(path, script)?;
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

pub fn set_wrapper_state(exe_name: &str, blocked: bool) -> Result<()> {
    let path = wrapper_state_path(exe_name);
    if let Some(p) = path.parent() { std::fs::create_dir_all(p)?; }
    std::fs::write(path, if blocked { "blocked" } else { "allowed" })?;
    Ok(())
}

pub fn remove_wrapper(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
    let bin      = wrapper_bin_path(&exe_name);
    let stat     = wrapper_state_path(&exe_name);
    for p in [&bin, &stat] { if p.exists() { std::fs::remove_file(p)?; } }
    Ok(())
}

// ── Network block via nftables ────────────────────────────────────────────────

pub fn install_network_block(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
    let uid      = unsafe { libc::getuid() };

    let init = "add table inet appblocker; \
                add chain inet appblocker output \
                { type filter hook output priority 0; policy accept; };";
    run_pkexec_nft(init)?;

    let add = format!(
        "add rule inet appblocker output \
         skuid {uid} drop comment \"appblocker-{exe_name}\""
    );
    run_pkexec_nft(&add)
}

pub fn remove_network_block(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
    let cmd      = format!(
        r#"nft -a list chain inet appblocker output 2>/dev/null \
           | grep 'appblocker-{exe_name}' \
           | awk '{{print $NF}}' \
           | xargs -r -I{{}} nft delete rule inet appblocker output handle {{}}"#
    );
    let _ = std::process::Command::new("pkexec")
        .args(["sh", "-c", &cmd])
        .output();
    Ok(())
}

fn run_pkexec_nft(cmd: &str) -> Result<()> {
    let out = std::process::Command::new("pkexec")
        .args(["nft", cmd])
        .output()
        .context("failed to run pkexec nft")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "nft error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn exe_name_of(executable: &str) -> Result<String> {
    std::path::Path::new(executable)
        .file_name()
        .context("invalid executable path")?
        .to_str()
        .context("non-UTF-8 executable name")
        .map(|s| s.to_owned())
}
