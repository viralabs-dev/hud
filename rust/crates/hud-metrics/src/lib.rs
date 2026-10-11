//! Read-only Linux metrics. Missing/malformed/reset counters are unavailable,
//! never fabricated zero. No shell commands or environment mutation.
use std::{
    collections::VecDeque,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
const LIMIT: usize = 128 * 1024;
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub cpu_percent: Option<f64>,
    pub cores: Option<usize>,
    pub memory: Option<Memory>,
    pub load: Option<[f64; 3]>,
    pub processes: Option<u64>,
    pub network_bytes_per_second: Option<(f64, f64)>,
    pub uptime_seconds: Option<f64>,
    pub temperature_celsius: Option<f64>,
    pub battery: Option<(u8, String)>,
    /// Filesystem usage, deduplicated by native volume identifier.
    pub disks: Vec<Disk>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Memory {
    pub used: u64,
    pub total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Disk {
    pub label: String,
    pub used: u64,
    pub total: u64,
}
#[derive(Default)]
pub struct RawSample {
    pub cpu: Option<(u64, u64)>,
    pub network: Option<(u64, u64)>,
    pub sample: Sample,
}
pub struct Sampler {
    cpu: Option<(u64, u64)>,
    network: Option<(u64, u64, f64)>,
    last_rate: Option<(f64, f64)>,
    capacity: usize,
    pub cpu_history: VecDeque<Option<f64>>,
    pub network_history: VecDeque<Option<f64>>,
}
impl Sampler {
    pub fn new(history: usize) -> Self {
        Self {
            cpu: None,
            network: None,
            last_rate: None,
            capacity: history.min(3600),
            cpu_history: VecDeque::new(),
            network_history: VecDeque::new(),
        }
    }
    /// Timestamp must be monotonic seconds; injectable for deterministic tests.
    pub fn sample(&mut self, mut raw: RawSample, now: f64) -> Sample {
        raw.sample.cpu_percent = self.cpu.zip(raw.cpu).and_then(|((pi, pt), (i, t))| {
            let dt = t.checked_sub(pt)?;
            let di = i.checked_sub(pi)?;
            (dt > 0 && di <= dt).then(|| 100.0 * (1.0 - di as f64 / dt as f64))
        });
        self.cpu = raw.cpu;
        let net = raw.network.filter(|_| now.is_finite() && now >= 0.0);
        let rate = self.network.zip(net).and_then(|((pr, pt, at), (r, t))| {
            if now == at && r == pr && t == pt {
                return self.last_rate;
            }
            let dt = now - at;
            if dt <= 0.0 {
                return None;
            }
            Some((
                r.checked_sub(pr)? as f64 / dt,
                t.checked_sub(pt)? as f64 / dt,
            ))
        });
        self.network = net.map(|(r, t)| (r, t, now));
        self.last_rate = rate;
        raw.sample.network_bytes_per_second = rate;
        if self.capacity > 0 {
            if self.cpu_history.len() == self.capacity {
                self.cpu_history.pop_front();
                self.network_history.pop_front();
            }
            self.cpu_history.push_back(raw.sample.cpu_percent);
            self.network_history.push_back(rate.map(|(r, t)| r + t));
        }
        raw.sample
    }
}
pub struct LinuxBackend {
    proc_root: PathBuf,
    sys_root: PathBuf,
    disk_paths: Vec<(PathBuf, String)>,
}
impl Default for LinuxBackend {
    fn default() -> Self {
        Self::new("/proc", "/sys").with_disk_paths(vec![(PathBuf::from("/"), "/".into())])
    }
}
impl LinuxBackend {
    pub fn new(proc_root: impl Into<PathBuf>, sys_root: impl Into<PathBuf>) -> Self {
        Self {
            proc_root: proc_root.into(),
            sys_root: sys_root.into(),
            disk_paths: Vec::new(),
        }
    }
    /// Explicit paths only, matching Python root/home/extra ordering. Never scans mounts.
    pub fn with_disk_paths(mut self, paths: Vec<(PathBuf, String)>) -> Self {
        self.disk_paths = paths.into_iter().take(64).collect();
        self
    }
    pub fn read(&self) -> RawSample {
        let stat = read(&self.proc_root.join("stat"));
        let (load, processes) = read(&self.proc_root.join("loadavg"))
            .and_then(|s| parse_load(&s))
            .map_or((None, None), |(a, b)| (Some(a), Some(b)));
        RawSample {
            cpu: stat.as_deref().and_then(parse_cpu),
            network: read(&self.proc_root.join("net/dev"))
                .as_deref()
                .and_then(parse_network),
            sample: Sample {
                cores: stat
                    .as_deref()
                    .map(|s| {
                        s.lines()
                            .filter(|l| {
                                l.strip_prefix("cpu")
                                    .is_some_and(|v| v.starts_with(|c: char| c.is_ascii_digit()))
                            })
                            .count()
                    })
                    .filter(|n| *n > 0),
                memory: read(&self.proc_root.join("meminfo"))
                    .as_deref()
                    .and_then(parse_memory),
                load,
                processes,
                uptime_seconds: read(&self.proc_root.join("uptime"))
                    .and_then(|s| positive_float(s.split_whitespace().next()?)),
                temperature_celsius: self.temperature(),
                battery: self.battery(),
                disks: collect_disks(&self.disk_paths, native_disk),
                ..Sample::default()
            },
        }
    }
    fn temperature(&self) -> Option<f64> {
        let dirs = directories(&self.sys_root.join("class/hwmon"));
        for want in ["coretemp", "k10temp", "zenpower", "cpu_thermal", "acpitz"] {
            let mut best: Option<f64> = None;
            for d in &dirs {
                if read(&d.join("name")).as_deref().map(str::trim) != Some(want) {
                    continue;
                }
                let Ok(entries) = std::fs::read_dir(d) else {
                    continue;
                };
                for e in entries.take(128).flatten() {
                    let n = e.file_name();
                    let n = n.to_string_lossy();
                    if n.starts_with("temp") && n.ends_with("_input") {
                        if let Some(t) = read(&e.path())
                            .and_then(|s| s.trim().parse::<i64>().ok())
                            .map(|v| v as f64 / 1000.0)
                            .filter(|t| *t > 0.0 && *t < 150.0)
                        {
                            best = Some(best.map_or(t, |b| b.max(t)));
                        }
                    }
                }
            }
            if best.is_some() {
                return best;
            }
        }
        None
    }
    fn battery(&self) -> Option<(u8, String)> {
        for d in directories(&self.sys_root.join("class/power_supply")) {
            if !d.file_name()?.to_string_lossy().starts_with("BAT") {
                continue;
            }
            let Some(cap) = read(&d.join("capacity"))
                .and_then(|s| s.trim().parse::<u8>().ok())
                .filter(|v| *v <= 100)
            else {
                continue;
            };
            let Some(status) = read(&d.join("status")) else {
                continue;
            };
            let status = status.trim();
            if ["Charging", "Discharging", "Full", "Not charging", "Unknown"].contains(&status) {
                return Some((cap, status.to_owned()));
            }
        }
        None
    }
}
/// Device observed even if capacity fails: first path owns the volume label.
#[derive(Clone, Debug)]
pub struct DiskReading {
    pub device: u64,
    pub capacity: Option<(u64, u64)>,
}
pub fn collect_disks(
    paths: &[(PathBuf, String)],
    mut read: impl FnMut(&Path) -> Option<DiskReading>,
) -> Vec<Disk> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (path, label) in paths.iter().take(64) {
        let Some(reading) = read(path) else {
            continue;
        };
        if !seen.insert(reading.device) {
            continue;
        }
        let Some((used, total)) = reading.capacity else {
            continue;
        };
        if total > 0 && used <= total {
            out.push(Disk {
                label: label.clone(),
                used,
                total,
            });
        }
    }
    out
}
/// Matches Python: total=blocks*fragment size, used=total-available to user.
pub fn disk_capacity(blocks: u64, available: u64, fragment: u64) -> Option<(u64, u64)> {
    let total = blocks.checked_mul(fragment)?;
    let free = available.checked_mul(fragment)?;
    Some((total.checked_sub(free)?, total))
}
#[cfg(target_os = "linux")]
fn native_disk(path: &Path) -> Option<DiskReading> {
    use rustix::fs::{fstat, fstatvfs, open, Mode, OFlags};
    // Directory + NONBLOCK refuses FIFO/device reads. Final symlinks are refused;
    // ancestors may resolve normally because this adapter only queries metadata.
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    let stat = fstat(&fd).ok()?;
    let capacity = fstatvfs(&fd)
        .ok()
        .and_then(|v| disk_capacity(v.f_blocks, v.f_bavail, v.f_frsize));
    Some(DiskReading {
        device: stat.st_dev,
        capacity,
    })
}
#[cfg(not(target_os = "linux"))]
fn native_disk(_: &Path) -> Option<DiskReading> {
    None
}
fn directories(path: &Path) -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(path)
        .map(|it| it.take(128).flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}
fn read(path: &Path) -> Option<String> {
    // Proc/sys are trusted roots supplied by the caller. Bound file content;
    // sysfs class symlinks intentionally resolve to kernel devices.
    let mut data = Vec::new();
    File::open(path)
        .ok()?
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() > LIMIT {
        return None;
    }
    String::from_utf8(data).ok()
}
fn positive_float(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
}
pub fn parse_cpu(s: &str) -> Option<(u64, u64)> {
    let mut p = s.lines().next()?.split_whitespace();
    if p.next()? != "cpu" {
        return None;
    }
    let vals: Vec<u64> = p.take(8).map(str::parse).collect::<Result<_, _>>().ok()?;
    if vals.len() < 4 {
        return None;
    }
    let idle = vals[3].checked_add(*vals.get(4).unwrap_or(&0))?;
    let total = vals.iter().try_fold(0u64, |a, v| a.checked_add(*v))?;
    Some((idle, total))
}
pub fn parse_memory(s: &str) -> Option<Memory> {
    let mut vals = [None; 4];
    for line in s.lines() {
        let (key, rest) = line.split_once(':')?;
        let index = match key {
            "MemTotal" => 0,
            "MemAvailable" => 1,
            "SwapTotal" => 2,
            "SwapFree" => 3,
            _ => continue,
        };
        if vals[index].is_some() {
            return None;
        }
        let mut p = rest.split_whitespace();
        let n = p.next()?.parse::<u64>().ok()?.checked_mul(1024)?;
        if p.next() != Some("kB") || p.next().is_some() {
            return None;
        }
        vals[index] = Some(n);
    }
    let [Some(total), Some(available), Some(swap_total), Some(swap_free)] = vals else {
        return None;
    };
    if total == 0 {
        return None;
    }
    Some(Memory {
        used: total.checked_sub(available)?,
        total,
        swap_used: swap_total.checked_sub(swap_free)?,
        swap_total,
    })
}
pub fn parse_load(s: &str) -> Option<([f64; 3], u64)> {
    let mut p = s.split_whitespace();
    let load = [
        positive_float(p.next()?)?,
        positive_float(p.next()?)?,
        positive_float(p.next()?)?,
    ];
    let (running, total) = p.next()?.split_once('/')?;
    let running = running.parse::<u64>().ok()?;
    let total = total.parse::<u64>().ok()?;
    (running <= total).then_some((load, total))
}
pub fn parse_network(s: &str) -> Option<(u64, u64)> {
    let mut rx = 0u64;
    let mut tx = 0u64;
    let mut seen = false;
    for line in s.lines().skip(2) {
        let (name, rest) = line.split_once(':')?;
        let name = name.trim();
        if name == "lo"
            || ["veth", "docker", "br-", "virbr"]
                .iter()
                .any(|p| name.starts_with(p))
        {
            continue;
        }
        let vals: Vec<_> = rest.split_whitespace().collect();
        if vals.len() < 16 {
            return None;
        }
        rx = rx.checked_add(vals[0].parse::<u64>().ok()?)?;
        tx = tx.checked_add(vals[8].parse::<u64>().ok()?)?;
        seen = true;
    }
    seen.then_some((rx, tx))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn raw(cpu: Option<(u64, u64)>, network: Option<(u64, u64)>) -> RawSample {
        RawSample {
            cpu,
            network,
            sample: Sample::default(),
        }
    }
    #[test]
    fn deltas_and_bounded_history() {
        let mut s = Sampler::new(2);
        assert_eq!(
            s.sample(raw(Some((100, 1000)), Some((1000, 500))), 10.0)
                .cpu_percent,
            None
        );
        let next = s.sample(raw(Some((150, 1100)), Some((3000, 1500))), 12.0);
        assert_eq!(next.cpu_percent, Some(50.0));
        assert_eq!(next.network_bytes_per_second, Some((1000.0, 500.0)));
        assert_eq!(
            s.sample(raw(Some((160, 1200)), Some((3000, 1500))), 12.0)
                .network_bytes_per_second,
            Some((1000.0, 500.0))
        );
        assert_eq!(s.cpu_history.len(), 2);
        assert_eq!(s.network_history.len(), 2);
    }
    #[test]
    fn resets_missing_and_bad_time_are_unavailable() {
        let mut s = Sampler::new(0);
        s.sample(raw(Some((10, 100)), Some((100, 100))), 2.0);
        let x = s.sample(raw(Some((5, 50)), Some((5, 5))), 3.0);
        assert_eq!(x.cpu_percent, None);
        assert_eq!(x.network_bytes_per_second, None);
        s.sample(raw(None, None), 4.0);
        assert_eq!(
            s.sample(raw(Some((20, 200)), Some((10, 10))), 5.0)
                .cpu_percent,
            None
        );
        assert_eq!(
            s.sample(raw(None, Some((20, 20))), f64::NAN)
                .network_bytes_per_second,
            None
        );
        assert!(s.cpu_history.is_empty());
    }
    #[test]
    fn python_fixture_units_and_guest_exclusion() {
        assert_eq!(
            parse_cpu("cpu 100 0 50 800 50 0 0 0 999 999\ncpu0 1 2 3 4"),
            Some((850, 1000))
        );
        let m=parse_memory("MemTotal: 16000000 kB\nMemAvailable: 4000000 kB\nSwapTotal: 2000000 kB\nSwapFree: 1500000 kB\n").unwrap();
        assert_eq!(m.used, 12000000 * 1024);
        assert_eq!(m.swap_used, 500000 * 1024);
        assert_eq!(
            parse_load("0.50 0.75 1.00 2/345 6789"),
            Some(([0.5, 0.75, 1.0], 345))
        );
        let net="header\nheader\nlo: 999 0 0 0 0 0 0 0 999 0 0 0 0 0 0 0\neth0: 1000 0 0 0 0 0 0 0 400 0 0 0 0 0 0 0\nveth1: 5000 0 0 0 0 0 0 0 5000 0 0 0 0 0 0 0\n";
        assert_eq!(parse_network(net), Some((1000, 400)));
    }
    #[test]
    fn malformed_overflow_and_no_interface_fail_closed() {
        assert_eq!(parse_cpu("cpu 1 2"), None);
        assert_eq!(parse_cpu("cpu 18446744073709551615 1 0 0"), None);
        assert_eq!(parse_memory("MemTotal: 18446744073709551615 kB"), None);
        assert_eq!(
            parse_memory("MemTotal: 10 kB\nMemAvailable: 11 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB"),
            None
        );
        assert_eq!(parse_load("NaN 1 2 1/3"), None);
        assert_eq!(parse_network("a\nb\n"), None);
    }
    #[test]
    fn injected_paths_bounded_reads_sys_units() {
        let dir = std::env::temp_dir().join(format!("hud-metrics-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("proc/net")).unwrap();
        std::fs::create_dir_all(dir.join("sys/class/hwmon/hwmon0")).unwrap();
        std::fs::create_dir_all(dir.join("sys/class/power_supply/BAT0")).unwrap();
        let write = |p: &str, s: &str| std::fs::write(dir.join(p), s).unwrap();
        write("proc/stat", "cpu 100 0 50 800 50 0 0 0\ncpu0 1 2 3 4\n");
        write("proc/uptime", "12345.67 99999.00");
        write("sys/class/hwmon/hwmon0/name", "coretemp\n");
        write("sys/class/hwmon/hwmon0/temp1_input", "42500\n");
        write("sys/class/power_supply/BAT0/capacity", "85\n");
        write("sys/class/power_supply/BAT0/status", "Discharging\n");
        let b = LinuxBackend::new(dir.join("proc"), dir.join("sys"));
        let s = b.read().sample;
        assert_eq!(s.cores, Some(1));
        assert_eq!(s.uptime_seconds, Some(12345.67));
        assert_eq!(s.temperature_celsius, Some(42.5));
        assert_eq!(s.battery, Some((85, "Discharging".into())));
        assert_eq!(s.memory, None);
        write("proc/uptime", &"0".repeat(LIMIT + 1));
        assert_eq!(b.read().sample.uptime_seconds, None);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn disk_fixture_reserved_space_overflow_and_dedup() {
        assert_eq!(disk_capacity(100, 20, 1024), Some((80 * 1024, 100 * 1024)));
        assert_eq!(disk_capacity(u64::MAX, 1, 1024), None);
        assert_eq!(disk_capacity(10, 11, 1), None);
        let paths: Vec<_> = ["/", "/home", "/extra", "/broken", "/duplicate-broken"]
            .iter()
            .map(|p| (PathBuf::from(p), p.to_string()))
            .collect();
        let disks = collect_disks(&paths, |p| match p.to_str().unwrap() {
            "/" | "/home" => Some(DiskReading {
                device: 1,
                capacity: Some((30, 100)),
            }),
            "/extra" => Some(DiskReading {
                device: 2,
                capacity: Some((5, 10)),
            }),
            "/broken" => Some(DiskReading {
                device: 3,
                capacity: None,
            }),
            _ => Some(DiskReading {
                device: 3,
                capacity: Some((4, 10)),
            }),
        });
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].label, "/");
        assert_eq!(disks[1].label, "/extra");
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn native_disk_refuses_file_and_final_symlink() {
        let dir = std::env::temp_dir().join(format!("hud-disk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("file"), "unchanged").unwrap();
        std::os::unix::fs::symlink("/", dir.join("link")).unwrap();
        assert!(native_disk(&dir.join("file")).is_none());
        assert!(native_disk(&dir.join("link")).is_none());
        assert!(native_disk(&dir.join("missing")).is_none());
        assert!(native_disk(&dir).is_some());
        assert_eq!(
            std::fs::read_to_string(dir.join("file")).unwrap(),
            "unchanged"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn live_linux_read_smoke() {
        let s = LinuxBackend::default().read().sample;
        assert!(s.memory.is_some_and(|m| m.total > 0));
        assert!(s
            .disks
            .iter()
            .any(|d| d.label == "/" && d.total > 0 && d.used <= d.total));
        assert!(s.uptime_seconds.is_some_and(|v| v > 0.0));
    }
}

/// Platform fixture conversions; native availability is explicit.
pub mod platforms;

/// Nonblocking, coalescing background sampling controller.
pub mod worker;

/// Fixed-width native ABI boundaries and fixture contracts.
pub mod native;
