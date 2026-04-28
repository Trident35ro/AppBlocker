use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub pid:         i32,
    pub name:        String,
    pub exe_path:    Option<String>,
    pub cpu_percent: f32,
    pub mem_mb:      f64,
    pub status:      String,
    pub cmdline:     String,
}

fn ticks_per_second() -> u64 {
    let tps = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if tps > 0 { tps as u64 } else { 100 }
}

struct Prev {
    cpu_jiffies: u64,
    at: Instant,
}

pub struct ProcessMonitor {
    prev:     HashMap<i32, Prev>,
    num_cpus: u64,
}

impl ProcessMonitor {
    pub fn new() -> Self {
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as u64)
            .unwrap_or(1);
        Self { prev: HashMap::new(), num_cpus }
    }

    pub fn scan(&mut self) -> Vec<ProcessInfo> {
        let now = Instant::now();
        let tps = ticks_per_second();
        let mut out = Vec::new();

        let iter = match procfs::process::all_processes() {
            Ok(i) => i,
            Err(e) => { log::warn!("process scan failed: {e}"); return out; }
        };

        for entry in iter {
            let proc = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };
            let stat = match proc.stat() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let status = match proc.status() {
                Ok(s) => s,
                Err(_) => continue,
            };

            let pid          = proc.pid();
            let cpu_jiffies  = stat.utime + stat.stime;

            let cpu_percent = self.prev.get(&pid).map(|p| {
                let dj = cpu_jiffies.saturating_sub(p.cpu_jiffies) as f64;
                let ds = now.duration_since(p.at).as_secs_f64();
                if ds > 0.0 { (dj / tps as f64 / ds / self.num_cpus as f64 * 100.0) as f32 }
                else { 0.0 }
            }).unwrap_or(0.0);

            self.prev.insert(pid, Prev { cpu_jiffies, at: now });

            let mem_mb    = status.vmrss.unwrap_or(0) as f64 / 1024.0;
            let exe_path  = proc.exe().ok().map(|p| p.to_string_lossy().into_owned());
            let cmdline   = proc.cmdline().ok().map(|v| v.join(" ")).unwrap_or_default();
            let proc_stat = match stat.state {
                'R' => "Running",
                'S' => "Sleeping",
                'D' => "Waiting",
                'Z' => "Zombie",
                'T' => "Stopped",
                _   => "Other",
            };

            out.push(ProcessInfo {
                pid,
                name: stat.comm,
                exe_path,
                cpu_percent,
                mem_mb,
                status: proc_stat.to_owned(),
                cmdline,
            });
        }

        // Remove stale entries so prev map stays tidy.
        let alive: std::collections::HashSet<i32> = out.iter().map(|p| p.pid).collect();
        self.prev.retain(|pid, _| alive.contains(pid));

        out
    }
}
