use anyhow::{Context, Result};
use std::path::PathBuf;

// ── Kill ──────────────────────────────────────────────────────────────────────

/// Send SIGTERM — lets the process clean up gracefully.
pub fn kill_process(pid: i32) -> Result<()> {
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 { Ok(()) }
    else { Err(std::io::Error::last_os_error().into()) }
}

/// Send SIGKILL — immediate termination, equivalent to `pkill -9`.
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

pub fn install_wrapper(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
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

    let path = wrapper_bin_path(&exe_name);
    std::fs::write(&path, script)?;

    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(&path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms)?;
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
    let bin  = wrapper_bin_path(&exe_name);
    let stat = wrapper_state_path(&exe_name);
    for p in [&bin, &stat] { if p.exists() { std::fs::remove_file(p)?; } }
    Ok(())
}

// ── Network block via nftables ────────────────────────────────────────────────

pub fn install_network_block(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
    let uid = unsafe { libc::getuid() };

    // Ensure the appblocker table/chain exist.
    let init = "add table inet appblocker; \
                add chain inet appblocker output \
                { type filter hook output priority 0; policy accept; };";
    run_pkexec_nft(init)?;

    // Add a tagged drop rule.
    let add = format!(
        "add rule inet appblocker output \
         skuid {uid} drop comment \"appblocker-{exe_name}\"",
        uid      = uid,
        exe_name = exe_name,
    );
    run_pkexec_nft(&add)
}

pub fn remove_network_block(executable: &str) -> Result<()> {
    let exe_name = exe_name_of(executable)?;
    // List rules, find handle for our comment, delete it.
    let cmd = format!(
        r#"nft -a list chain inet appblocker output 2>/dev/null \
           | grep 'appblocker-{exe_name}' \
           | awk '{{print $NF}}' \
           | xargs -r -I{{}} nft delete rule inet appblocker output handle {{}}"#,
        exe_name = exe_name,
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
