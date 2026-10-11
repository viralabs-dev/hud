//! Fixture-tested conversions. Native macOS source is not compiled on Linux;
//! Native OS calls are cfg-gated and still require validation on their host.
use crate::{Memory, RawSample, Sample, LIMIT};
use std::{collections::BTreeMap, io};
fn bounded(s: &str) -> Option<&str> {
    (s.len() <= LIMIT && !s.contains('\0')).then_some(s)
}
fn integer(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
        None
    } else {
        s.parse().ok()
    }
}
fn fields(s: &str) -> Option<BTreeMap<&str, &str>> {
    let mut result = BTreeMap::new();
    for chunk in bounded(s)?
        .trim()
        .trim_matches(|c| c == '{' || c == '}')
        .split(',')
    {
        let (key, value) = chunk.trim().split_once('=')?;
        if result.insert(key.trim(), value.trim()).is_some() {
            return None;
        }
    }
    Some(result)
}
pub mod macos {
    use super::*;
    pub fn vm_used(text: &str, total: u64) -> Option<u64> {
        let mut lines = bounded(text)?.lines();
        let header = lines.next()?;
        let page = integer(
            header
                .split_once("page size of ")?
                .1
                .split_once(" bytes")?
                .0,
        )?;
        if page == 0 || total == 0 {
            return None;
        }
        let mut pages = BTreeMap::new();
        for line in lines {
            let (key, value) = line.split_once(':')?;
            let value = integer(value.trim().trim_end_matches('.'))?;
            if pages.insert(key.trim().trim_matches('"'), value).is_some() {
                return None;
            }
        }
        let app = if let Some(anonymous) = pages.get("Anonymous pages") {
            anonymous.saturating_sub(*pages.get("Pages purgeable")?)
        } else {
            *pages.get("Pages active")?
        };
        Some(
            app.checked_add(*pages.get("Pages wired down")?)?
                .checked_add(*pages.get("Pages occupied by compressor")?)?
                .checked_mul(page)?
                .min(total),
        )
    }
    pub fn swap(text: &str) -> Option<(u64, u64)> {
        let text = bounded(text)?.strip_prefix("vm.swapusage:").unwrap_or(text);
        let text = text
            .trim()
            .strip_suffix("(encrypted)")
            .unwrap_or(text)
            .trim();
        let parts: Vec<_> = text.split_whitespace().collect();
        let mut map = BTreeMap::new();
        for p in parts.chunks(3) {
            if p.len() != 3 || p[1] != "=" || map.insert(p[0], p[2]).is_some() {
                return None;
            }
        }
        fn bytes(s: &str) -> Option<u64> {
            let unit = s.chars().last()?;
            let scale = match unit {
                'B' => 1u64,
                'K' => 1024,
                'M' => 1024 * 1024,
                'G' => 1024 * 1024 * 1024,
                'T' => 1024u64.pow(4),
                _ => return None,
            };
            let number = &s[..s.len() - 1];
            let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
            if fraction.len() > 6 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let base = integer(whole)?.checked_mul(scale)?;
            if fraction.is_empty() {
                return Some(base);
            }
            base.checked_add(
                integer(fraction)?.checked_mul(scale)? / 10u64.pow(fraction.len() as u32),
            )
        }
        let total = bytes(map.get("total")?)?;
        let used = bytes(map.get("used")?)?;
        (used <= total).then_some((used, total))
    }
    pub fn uptime(text: &str, epoch_seconds: f64) -> Option<f64> {
        let content = bounded(text)?.split_once('{')?.1.split_once('}')?.0;
        let map = fields(content)?;
        let sec = integer(map.get("sec")?)?;
        let usec = integer(map.get("usec")?)?;
        if usec >= 1_000_000 || !epoch_seconds.is_finite() {
            return None;
        }
        let age = epoch_seconds - sec as f64 - usec as f64 / 1_000_000.;
        (age >= 0.).then_some(age)
    }
    pub fn load(text: &str) -> Option<[f64; 3]> {
        let raw: Vec<_> = bounded(text)?
            .trim()
            .trim_matches(|c| c == '{' || c == '}')
            .split_whitespace()
            .collect();
        if raw.len() != 3 {
            return None;
        }
        let mut out = [0.; 3];
        for (i, s) in raw.iter().enumerate() {
            out[i] = crate::positive_float(s)?;
        }
        Some(out)
    }
    pub fn battery(text: &str) -> Option<(u8, String)> {
        if let Some(line) = bounded(text)?
            .lines()
            .find(|l| l.contains("InternalBattery"))
        {
            let (prefix, after) = line.split_once("%;")?;
            let percent = prefix.split_whitespace().last()?.parse::<u8>().ok()?;
            if percent > 100 {
                return None;
            }
            let state = match after.split(';').next()?.trim() {
                "charging" | "finishing charge" => "Charging",
                "discharging" => "Discharging",
                "charged" => "Full",
                "AC attached" => "Not charging",
                _ => return None,
            };
            return Some((percent, state.into()));
        }
        None
    }
    pub fn network(text: &str) -> Option<(u64, u64)> {
        let mut seen = std::collections::HashSet::new();
        let mut rx = 0u64;
        let mut tx = 0u64;
        for line in bounded(text)?.lines().skip(1) {
            let f: Vec<_> = line.split_whitespace().collect();
            if f.len() < 10 || !f[2].starts_with("<Link#") {
                continue;
            }
            let name = f[0].trim_end_matches('*');
            if ["lo", "gif", "stf", "utun", "awdl", "llw", "bridge", "anpi"]
                .iter()
                .any(|p| name.starts_with(p))
            {
                continue;
            }
            if !seen.insert(name) {
                return None;
            }
            rx = rx.checked_add(integer(f[f.len() - 5])?)?;
            tx = tx.checked_add(integer(f[f.len() - 2])?)?;
        }
        (!seen.is_empty()).then_some((rx, tx))
    }
    /// Fixed executable and argv only; no user paths or shell fragments.
    pub const COMMANDS: &[(&str, &[&str])] = &[
        ("/usr/sbin/sysctl", &["-n", "hw.memsize"]),
        ("/usr/bin/vm_stat", &[]),
        ("/usr/sbin/sysctl", &["vm.swapusage"]),
        ("/usr/sbin/sysctl", &["-n", "vm.loadavg"]),
        ("/usr/sbin/sysctl", &["kern.boottime"]),
        ("/usr/sbin/netstat", &["-ib"]),
        ("/usr/bin/pmset", &["-g", "batt"]),
        ("/bin/ps", &["-axo", "pid="]),
        ("/usr/sbin/sysctl", &["-n", "hw.logicalcpu"]),
    ];
    #[cfg(target_os = "macos")]
    pub struct Backend {
        paths: Vec<(std::path::PathBuf, String)>,
    }
    #[cfg(target_os = "macos")]
    impl Backend {
        pub fn new(paths: Vec<(std::path::PathBuf, String)>) -> Self {
            Self {
                paths: paths.into_iter().take(64).collect(),
            }
        }
    }
    #[cfg(target_os = "macos")]
    impl crate::worker::MetricsSource for Backend {
        fn shutdown_kind(&self) -> crate::worker::Shutdown {
            crate::worker::Shutdown::BoundedProcessCleanup
        }
        fn read(&mut self, cancel: &crate::worker::Cancellation) -> io::Result<RawSample> {
            use hud_runner::{ConfiguredCommand, Runner};
            use std::{
                path::Path,
                time::{Duration, SystemTime, UNIX_EPOCH},
            };
            let mut outputs = Vec::new();
            let runner = Runner::default();
            for (exe, args) in COMMANDS {
                if cancel.is_cancelled() {
                    return Err(io::ErrorKind::Interrupted.into());
                }
                let argv: Vec<String> = std::iter::once(*exe)
                    .chain(args.iter().copied())
                    .map(str::to_owned)
                    .collect();
                let value = ConfiguredCommand::new(
                    &argv,
                    Path::new("/"),
                    Duration::from_millis(1500),
                    4096,
                )
                .ok()
                .and_then(|c| {
                    runner
                        .start("metrics", c, Path::new("/").into())
                        .ok()
                        .flatten()
                })
                .and_then(|running| loop {
                    if cancel.is_cancelled() {
                        running.cancellation.cancel();
                        // Await the result so child cleanup completes before source exits.
                    }
                    match running.try_result() {
                        Ok(out) => break out.ok(),
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break None,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                    }
                })
                .filter(|o| o.code == Some(0) && !o.timed_out && !o.cancelled && !o.truncated)
                .map(|o| o.stdout.join("\n"))
                .filter(|s| s.len() <= LIMIT);
                outputs.push(value);
            }
            if cancel.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let mut raw = from_outputs(
                &outputs,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|v| v.as_secs_f64()),
            );
            raw.cpu = crate::native::macos::cpu();
            raw.sample.disks = crate::native::macos::disks(&self.paths);
            Ok(raw)
        }
    }
    pub fn from_outputs(outputs: &[Option<String>], epoch: Option<f64>) -> RawSample {
        let get = |i: usize| outputs.get(i).and_then(|v| v.as_deref());
        let memory = get(0)
            .and_then(|s| integer(s.trim()))
            .zip(get(1))
            .and_then(|(total, vm)| {
                let used = vm_used(vm, total)?;
                let (swap_used, swap_total) = swap(get(2)?)?;
                Some(Memory {
                    used,
                    total,
                    swap_used,
                    swap_total,
                })
            });
        let processes = get(7).and_then(|s| {
            bounded(s)?;
            let mut n = 0u64;
            for l in s.lines().filter(|l| !l.trim().is_empty()) {
                integer(l.trim())?;
                n = n.checked_add(1)?;
            }
            (n > 0).then_some(n)
        });
        RawSample {
            cpu: None,
            network: get(5).and_then(network),
            sample: Sample {
                cores: get(8)
                    .and_then(|s| integer(s.trim()))
                    .and_then(|n| u32::try_from(n).ok())
                    .and_then(crate::native::core_count),
                memory,
                load: get(3).and_then(load),
                uptime_seconds: get(4).zip(epoch).and_then(|(s, e)| uptime(s, e)),
                battery: get(6).and_then(battery),
                processes,
                ..Sample::default()
            },
        }
    }
}
pub mod windows {
    use super::*;
    #[derive(Clone, Copy, Debug)]
    pub struct MemoryStatus {
        pub total_phys: u64,
        pub avail_phys: u64,
        pub total_page_file: u64,
        pub avail_page_file: u64,
    }
    impl MemoryStatus {
        pub fn memory(self) -> Option<Memory> {
            if self.total_phys == 0 {
                return None;
            }
            let used = self.total_phys.checked_sub(self.avail_phys)?;
            let commit_used = self.total_page_file.checked_sub(self.avail_page_file)?;
            let swap_total = self.total_page_file.saturating_sub(self.total_phys);
            Some(Memory {
                used,
                total: self.total_phys,
                swap_used: commit_used.saturating_sub(used).min(swap_total),
                swap_total,
            })
        }
    }
    pub fn cpu(idle: u64, kernel: u64, user: u64) -> Option<(u64, u64)> {
        let total = kernel.checked_add(user)?;
        (idle <= kernel).then_some((idle, total))
    }
    pub fn battery(ac: u8, flags: u8, percent: u8) -> Option<(u8, String)> {
        if flags == 255 || flags & 128 != 0 || percent > 100 || ac > 1 {
            return None;
        }
        let state = if flags & 8 != 0 {
            "Charging"
        } else if ac == 1 && percent == 100 {
            "Full"
        } else if ac == 1 {
            "Not charging"
        } else {
            "Discharging"
        };
        Some((percent, state.into()))
    }
    #[derive(Clone, Copy, Debug)]
    pub struct Interface {
        pub hardware: bool,
        pub filter: bool,
        pub up: bool,
        pub kind: u32,
        pub rx: u64,
        pub tx: u64,
    }
    pub fn network(rows: &[Interface]) -> Option<(u64, u64)> {
        if rows.len() > 4096 {
            return None;
        }
        let mut result = (0u64, 0u64);
        let mut found = false;
        for row in rows
            .iter()
            .filter(|r| r.hardware && !r.filter && r.up && r.kind != 24)
        {
            result.0 = result.0.checked_add(row.rx)?;
            result.1 = result.1.checked_add(row.tx)?;
            found = true;
        }
        found.then_some(result)
    }
    pub struct Backend {
        paths: Vec<(std::path::PathBuf, String)>,
    }
    impl Backend {
        pub fn new(paths: Vec<(std::path::PathBuf, String)>) -> Self {
            Self {
                paths: paths.into_iter().take(64).collect(),
            }
        }
        pub fn read(&self) -> io::Result<RawSample> {
            #[cfg(target_os = "windows")]
            {
                crate::native::windows::read(&self.paths, &crate::worker::Cancellation::default())
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = &self.paths;
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Windows metrics only run on the native host",
                ))
            }
        }
    }
    #[cfg(target_os = "windows")]
    impl crate::worker::MetricsSource for Backend {
        fn read(&mut self, cancel: &crate::worker::Cancellation) -> io::Result<RawSample> {
            crate::native::windows::read(&self.paths, cancel)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const VM:&str="Mach Virtual Memory Statistics: (page size of 16384 bytes)\nAnonymous pages: 10.\nPages purgeable: 2.\nPages wired down: 3.\nPages occupied by compressor: 4.\n";
    #[test]
    fn mac_memory_swap_checked_units() {
        assert_eq!(macos::vm_used(VM, 1_000_000), Some(15 * 16384));
        assert_eq!(
            macos::swap("vm.swapusage: total = 1.5G used = 512.25M free = 1023.75M"),
            Some((537133056, 1610612736))
        );
        assert!(macos::vm_used("Pages active: 1.", 100).is_none());
        assert!(macos::vm_used(&VM.replace("10.", "18446744073709551615."), u64::MAX).is_none());
        assert_eq!(
            macos::swap("vm.swapusage: total = 1G used = 512M free = 512M (encrypted)"),
            Some((536870912, 1073741824))
        );
        assert!(macos::swap("total = 1G used = 2G").is_none());
        assert!(macos::swap("total = NaNG used = 1G").is_none());
    }
    #[test]
    fn mac_boot_load_battery_errors() {
        assert_eq!(
            macos::uptime("kern.boottime: { sec = 100, usec = 500000 } Sat", 102.),
            Some(1.5)
        );
        assert!(macos::uptime("{ sec = 100, usec = 1000000 }", 102.).is_none());
        assert_eq!(macos::load("{ 0.5 1.0 2.0 }"), Some([0.5, 1., 2.]));
        assert!(macos::load("{ NaN 1 2 }").is_none());
        assert_eq!(macos::battery("Now drawing from 'Battery Power'\n-InternalBattery-0 80%; discharging; 2:00 remaining"),Some((80,"Discharging".into())));
        assert!(macos::battery("InternalBattery 101%; charged;").is_none());
        assert!(macos::battery("InternalBattery 80%; état inconnu;").is_none());
    }
    #[test]
    fn mac_network_duplicate_missing_overflow() {
        let fixture="Name Mtu Network Address Ipkts Ierrs Ibytes Opkts Oerrs Obytes Coll\nen0 1500 <Link#1> aa 1 0 100 2 0 50 0\nutun0 1500 <Link#2> aa 1 0 999 2 0 999 0";
        assert_eq!(macos::network(fixture), Some((100, 50)));
        assert!(
            macos::network(&(fixture.to_owned() + "\nen0 1500 <Link#1> aa 1 0 100 2 0 50 0"))
                .is_none()
        );
        assert!(macos::network("Name\n").is_none());
    }
    #[test]
    fn source_contract_limits_missing_unicode_and_fixed_commands() {
        assert!(macos::from_outputs(&[], Some(1.)).sample.memory.is_none());
        let mut outputs = vec![None; 9];
        outputs[8] = Some("8\n".into());
        assert_eq!(
            macos::from_outputs(&outputs, Some(1.)).sample.cores,
            Some(8)
        );
        outputs[8] = Some("0".into());
        assert_eq!(macos::from_outputs(&outputs, Some(1.)).sample.cores, None);
        assert!(macos::battery(&"x".repeat(LIMIT + 1)).is_none());
        for (exe, argv) in macos::COMMANDS {
            assert!(exe.starts_with('/'));
            assert!(!argv
                .iter()
                .any(|a| a.contains("ação") || a.contains("日本")));
        }
        // Unicode paths are labels/caller state, never interpolated into fixed argv.
        let outputs = vec![Some("caminho /Users/ação/日本".into())];
        assert!(macos::from_outputs(&outputs, Some(0.))
            .sample
            .memory
            .is_none());
        assert!(windows::Backend::new(Vec::new()).read().is_err());
    }
    #[test]
    fn windows_conversions_no_fabricated_zero() {
        let m = windows::MemoryStatus {
            total_phys: 100,
            avail_phys: 40,
            total_page_file: 150,
            avail_page_file: 20,
        }
        .memory()
        .unwrap();
        assert_eq!(m.used, 60);
        assert_eq!(m.swap_used, 50);
        assert_eq!(m.swap_total, 50);
        assert!(windows::MemoryStatus {
            total_phys: 100,
            avail_phys: 101,
            total_page_file: 0,
            avail_page_file: 0
        }
        .memory()
        .is_none());
        assert_eq!(windows::cpu(10, 30, 20), Some((10, 50)));
        assert!(windows::cpu(31, 30, 20).is_none());
        assert_eq!(windows::battery(0, 0, 80), Some((80, "Discharging".into())));
        assert!(windows::battery(1, 128, 255).is_none());
        let row = windows::Interface {
            hardware: true,
            filter: false,
            up: true,
            kind: 6,
            rx: 100,
            tx: 50,
        };
        assert_eq!(
            windows::network(&[
                row,
                windows::Interface {
                    filter: true,
                    ..row
                }
            ]),
            Some((100, 50))
        );
        assert!(windows::network(&[]).is_none());
        assert!(windows::network(&[
            windows::Interface {
                rx: u64::MAX,
                ..row
            },
            row
        ])
        .is_none());
    }
}
