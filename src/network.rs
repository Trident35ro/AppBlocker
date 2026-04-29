use anyhow::{Context, Result};
use crate::config::{NetworkBlockMethod, NetworkEntry, NetworkPreset, NetworkRule};

// ── Public API ────────────────────────────────────────────────────────────────

pub fn apply_network_rule(rule: &NetworkRule) -> Result<()> {
    match rule.method {
        NetworkBlockMethod::Dns      => apply_hosts(rule),
        NetworkBlockMethod::Nftables => apply_nft(rule),
    }
}

pub fn remove_network_rule(rule: &NetworkRule) -> Result<()> {
    match rule.method {
        NetworkBlockMethod::Dns      => remove_hosts(&rule.id),
        NetworkBlockMethod::Nftables => remove_nft(&rule.id),
    }
}

// ── DNS / /etc/hosts ──────────────────────────────────────────────────────────

fn apply_hosts(rule: &NetworkRule) -> Result<()> {
    let tag   = hosts_tag(&rule.id);
    let mut lines = vec![format!("# BEGIN {tag}")];

    for entry in rule.entries.iter().filter(|e| e.enabled && !e.value.trim().is_empty()) {
        let domain = entry.value.trim();
        lines.push(format!("0.0.0.0 {domain}"));
        if !domain.starts_with("www.") && domain.contains('.') {
            lines.push(format!("0.0.0.0 www.{domain}"));
        }
    }
    lines.push(format!("# END {tag}"));

    // Escape single quotes and use printf to append
    let block   = lines.join("\n").replace('\'', "'\\''");
    let script  = format!("printf '%s\\n' '{block}' >> /etc/hosts");
    run_pkexec_sh(&script)
}

fn remove_hosts(rule_id: &str) -> Result<()> {
    let tag    = hosts_tag(rule_id);
    let script = format!("sed -i '/# BEGIN {tag}/,/# END {tag}/d' /etc/hosts");
    run_pkexec_sh(&script)
}

fn hosts_tag(rule_id: &str) -> String {
    format!("AppBlocker-{rule_id}")
}

// ── nftables ──────────────────────────────────────────────────────────────────

fn apply_nft(rule: &NetworkRule) -> Result<()> {
    // Ensure table and chain exist.
    let init = "add table inet appblocker; \
                add chain inet appblocker output \
                { type filter hook output priority 0; policy accept; };";
    run_pkexec_nft(init)?;

    let verb = if rule.shutdown_on_connect { "drop" } else { "reject" };
    let tag  = nft_tag(&rule.id);

    for entry in rule.entries.iter().filter(|e| e.enabled && !e.value.trim().is_empty()) {
        let target = entry.value.trim();
        for ip in resolve_ips(target) {
            let cmd = format!(
                "add rule inet appblocker output ip daddr {ip} {verb} comment \"{tag}\""
            );
            let _ = run_pkexec_nft(&cmd); // ignore individual IP errors
        }
    }
    Ok(())
}

fn remove_nft(rule_id: &str) -> Result<()> {
    let tag    = nft_tag(rule_id);
    let script = format!(
        "nft -a list chain inet appblocker output 2>/dev/null \
         | grep '{tag}' \
         | awk '{{print $NF}}' \
         | xargs -r -I{{}} nft delete rule inet appblocker output handle {{}}"
    );
    let _ = std::process::Command::new("pkexec")
        .args(["sh", "-c", &script])
        .output();
    Ok(())
}

fn nft_tag(rule_id: &str) -> String {
    format!("appblocker-net-{rule_id}")
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn resolve_ips(target: &str) -> Vec<String> {
    if target.parse::<std::net::IpAddr>().is_ok() {
        return vec![target.to_owned()];
    }
    use std::net::ToSocketAddrs;
    match format!("{target}:80").to_socket_addrs() {
        Ok(addrs) => {
            let mut ips: Vec<String> = addrs
                .map(|a| a.ip().to_string())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            ips.sort();
            ips.truncate(8);
            ips
        }
        Err(e) => {
            log::warn!("DNS resolve for '{target}' failed: {e}");
            vec![]
        }
    }
}

fn run_pkexec_sh(cmd: &str) -> Result<()> {
    let out = std::process::Command::new("pkexec")
        .args(["sh", "-c", cmd])
        .output()
        .context("pkexec sh failed")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "command failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn run_pkexec_nft(cmd: &str) -> Result<()> {
    let out = std::process::Command::new("pkexec")
        .args(["nft", cmd])
        .output()
        .context("pkexec nft failed")?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "nft error: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

// ── Preset domain lists ───────────────────────────────────────────────────────

pub fn preset_entries(preset: &NetworkPreset) -> Vec<NetworkEntry> {
    match preset {
        NetworkPreset::BlockNsfw        => NSFW.iter().map(|&d| NetworkEntry::new(d)).collect(),
        NetworkPreset::BlockDistracting => DISTRACTING.iter().map(|&d| NetworkEntry::new(d)).collect(),
        NetworkPreset::BlockBoth => {
            let mut v: Vec<NetworkEntry> = NSFW.iter().chain(DISTRACTING.iter())
                .map(|&d| NetworkEntry::new(d))
                .collect();
            v.dedup_by(|a, b| a.value == b.value);
            v
        }
    }
}

const NSFW: &[&str] = &[
    "pornhub.com", "xvideos.com", "xnxx.com", "xhamster.com",
    "redtube.com", "youporn.com", "tube8.com", "spankbang.com",
    "eporner.com", "beeg.com", "porntrex.com", "tnaflix.com",
    "4tube.com", "drtuber.com", "hclips.com", "txxx.com",
];

const DISTRACTING: &[&str] = &[
    "reddit.com", "twitter.com", "x.com", "facebook.com",
    "instagram.com", "tiktok.com", "youtube.com", "twitch.tv",
    "9gag.com", "buzzfeed.com", "dailymail.co.uk", "tmz.com",
    "imgur.com", "tumblr.com", "news.ycombinator.com",
];
