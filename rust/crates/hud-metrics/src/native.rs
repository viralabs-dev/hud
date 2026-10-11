//! Audited fixed-width ABI boundaries. OS-specific calls remain unvalidated natively.
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::Disk;
#[cfg(target_os = "windows")]
use crate::{RawSample, Sample};
#[cfg(target_os = "windows")]
use std::io;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct FileTime {
    pub low: u32,
    pub high: u32,
}
impl FileTime {
    pub fn ticks(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}
#[repr(C)]
#[derive(Default)]
pub struct MemoryStatusEx {
    pub length: u32,
    pub load: u32,
    pub total_phys: u64,
    pub avail_phys: u64,
    pub total_page_file: u64,
    pub avail_page_file: u64,
    pub total_virtual: u64,
    pub avail_virtual: u64,
    pub avail_extended_virtual: u64,
}
#[repr(C)]
#[derive(Default)]
pub struct SystemPowerStatus {
    pub ac: u8,
    pub flags: u8,
    pub percent: u8,
    pub system: u8,
    pub life: u32,
    pub full_life: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub struct MibIfRow2 {
    pub luid: u64,
    pub index: u32,
    pub guid: Guid,
    pub alias: [u16; 257],
    pub description: [u16; 257],
    pub physical_length: u32,
    pub physical: [u8; 32],
    pub permanent: [u8; 32],
    pub mtu: u32,
    pub kind: u32,
    pub tunnel: i32,
    pub medium: i32,
    pub physical_medium: i32,
    pub access: i32,
    pub direction: i32,
    pub flags: u8,
    pub oper_status: i32,
    pub admin_status: i32,
    pub media_state: i32,
    pub network_guid: Guid,
    pub connection: i32,
    pub transmit_speed: u64,
    pub receive_speed: u64,
    pub counters: [u64; 18],
}
impl Default for MibIfRow2 {
    fn default() -> Self {
        Self {
            luid: 0,
            index: 0,
            guid: Guid::default(),
            alias: [0; 257],
            description: [0; 257],
            physical_length: 0,
            physical: [0; 32],
            permanent: [0; 32],
            mtu: 0,
            kind: 0,
            tunnel: 0,
            medium: 0,
            physical_medium: 0,
            access: 0,
            direction: 0,
            flags: 0,
            oper_status: 0,
            admin_status: 0,
            media_state: 0,
            network_guid: Guid::default(),
            connection: 0,
            transmit_speed: 0,
            receive_speed: 0,
            counters: [0; 18],
        }
    }
}
#[repr(C)]
pub struct MibIfTable2Header {
    pub count: u32,
    pub rows: [MibIfRow2; 0],
}
pub fn windows_network_rows(rows: &[MibIfRow2]) -> Option<(u64, u64)> {
    if rows.len() > 4096 {
        return None;
    }
    let rows: Vec<_> = rows
        .iter()
        .map(|r| crate::platforms::windows::Interface {
            hardware: r.flags & 1 != 0,
            filter: r.flags & 2 != 0,
            up: r.oper_status == 1,
            kind: r.kind,
            rx: r.counters[0],
            tx: r.counters[9],
        })
        .collect();
    crate::platforms::windows::network(&rows)
}
pub fn process_count_bytes(returned: u32, capacity: u32) -> Option<u64> {
    if returned >= capacity || !returned.is_multiple_of(4) || !capacity.is_multiple_of(4) {
        return None;
    }
    Some(u64::from(returned / 4))
}
pub fn core_count(count: u32) -> Option<usize> {
    (count > 0 && count <= 65535).then_some(count as usize)
}
pub fn mac_cpu_ticks(ticks: [u32; 4]) -> Option<(u64, u64)> {
    let total = ticks
        .iter()
        .try_fold(0u64, |a, v| a.checked_add(u64::from(*v)))?;
    Some((u64::from(ticks[2]), total))
}
/// Accepted native Windows paths are explicit absolute drive paths only. UNC,
/// device namespaces, relative and NUL-containing paths are rejected before FFI.
pub fn windows_drive(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !b"\\/".contains(&bytes[2])
        || path.contains('\0')
        || path.len() > 32760
    {
        return None;
    }
    if path[3..].split(['\\', '/']).any(|s| s == ".." || s == ".") {
        return None;
    }
    Some(format!("{}:", char::from(bytes[0]).to_ascii_uppercase()))
}
/// Pure UTF-16 encoding contract, fixture-tested without loading Windows DLLs.
pub fn windows_wide_path(path: &str) -> Option<Vec<u16>> {
    windows_drive(path)?;
    let mut wide: Vec<u16> = path.encode_utf16().collect();
    if wide.len() > 32760 {
        return None;
    }
    wide.push(0);
    Some(wide)
}
#[cfg(target_os = "macos")]
pub mod macos {
    use super::*;
    #[link(name = "System")]
    unsafe extern "C" {
        fn mach_host_self() -> u32;
        static mach_task_self_: u32;
        fn host_statistics(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
        fn mach_port_deallocate(task: u32, name: u32) -> i32;
    }
    pub fn cpu() -> Option<(u64, u64)> {
        // HOST_CPU_LOAD_INFO=3; natural_t cpu_ticks[4], count measured in integer_t.
        let mut ticks = [0i32; 4];
        let mut count = 4u32;
        // SAFETY: host right is OS-issued; aligned owned 4-word output and count
        // live across call. Release acquired send right on every status path.
        let (status, count) = unsafe {
            let host = mach_host_self();
            if host == 0 {
                return None;
            }
            let status = host_statistics(host, 3, ticks.as_mut_ptr(), &mut count);
            let _ = mach_port_deallocate(mach_task_self_, host);
            (status, count)
        };
        if status != 0 || count != 4 {
            return None;
        }
        mac_cpu_ticks(ticks.map(|v| v as u32))
    }
    pub fn disks(paths: &[(PathBuf, String)]) -> Vec<Disk> {
        use rustix::fs::{fstatvfs, open, Mode, OFlags};
        crate::collect_disks(paths, |path| {
            let fd = open(
                path,
                OFlags::RDONLY
                    | OFlags::DIRECTORY
                    | OFlags::NOFOLLOW
                    | OFlags::NONBLOCK
                    | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .ok()?;
            let v = fstatvfs(&fd).ok()?;
            let total = v.f_blocks.checked_mul(v.f_frsize)?;
            // Match Python APFS policy: system/Data share capacity, one label.
            Some(crate::DiskReading {
                device: total,
                capacity: crate::disk_capacity(v.f_blocks, v.f_bavail, v.f_frsize),
            })
        })
    }
}
#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
        fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
        fn GetTickCount64() -> u64;
        fn GetDriveTypeW(root: *const u16) -> u32;
        fn GetActiveProcessorCount(group: u16) -> u32;
        fn K32EnumProcesses(pids: *mut u32, bytes: u32, needed: *mut u32) -> i32;
        fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        fn GetIfTable2(table: *mut *mut MibIfTable2Header) -> u32;
        fn FreeMibTable(table: *mut std::ffi::c_void);
    }
    fn network(cancel: &crate::worker::Cancellation) -> Option<(u64, u64)> {
        if cancel.is_cancelled() {
            return None;
        }
        let mut table = std::ptr::null_mut();
        // SAFETY: owned null-initialized out-pointer; successful OS call owns allocation.
        if unsafe { GetIfTable2(&mut table) } != 0 {
            return None;
        }
        let table = std::ptr::NonNull::new(table)?;
        struct Guard(std::ptr::NonNull<MibIfTable2Header>);
        impl Drop for Guard {
            fn drop(&mut self) {
                // SAFETY: exactly once for the allocation returned by successful GetIfTable2.
                unsafe { FreeMibTable(self.0.as_ptr().cast()) };
            }
        }
        let guard = Guard(table);
        // SAFETY: successful GetIfTable2 returns its native aligned header and row
        // allocation. repr(C) offsets/stride tested; native x64/ARM validation pending.
        let count = unsafe { (*guard.0.as_ptr()).count } as usize;
        if count > 4096 || cancel.is_cancelled() {
            return None;
        }
        let size = count.checked_mul(std::mem::size_of::<MibIfRow2>())?;
        if size > isize::MAX as usize {
            return None;
        }
        // SAFETY: documented array has NumEntries rows; no retained references;
        // integer-only repr(C) rows copied before guard deallocates table.
        let rows = unsafe {
            let pointer = guard
                .0
                .as_ptr()
                .cast::<u8>()
                .add(std::mem::offset_of!(MibIfTable2Header, rows))
                .cast::<MibIfRow2>();
            std::slice::from_raw_parts(pointer, count)
        };
        windows_network_rows(rows)
    }
    fn processes(cancel: &crate::worker::Cancellation) -> Option<u64> {
        let mut capacity = 1024usize;
        while capacity <= 1 << 20 {
            if cancel.is_cancelled() {
                return None;
            }
            let mut pids = vec![0u32; capacity];
            let bytes = u32::try_from(capacity.checked_mul(4)?).ok()?;
            let mut needed = 0u32;
            // SAFETY: writable u32 PID array contains capacity elements and byte
            // count matches allocation. needed is separate initialized owned DWORD.
            if unsafe { K32EnumProcesses(pids.as_mut_ptr(), bytes, &mut needed) } == 0 {
                return None;
            }
            if let Some(count) = process_count_bytes(needed, bytes) {
                return Some(count);
            }
            if needed != bytes {
                return None;
            }
            capacity = capacity.checked_mul(2)?;
        }
        None
    }
    pub fn read(
        paths: &[(PathBuf, String)],
        cancel: &crate::worker::Cancellation,
    ) -> io::Result<RawSample> {
        if !cfg!(target_pointer_width = "64") {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows native metric ABI requires x64/ARM64 validation",
            ));
        }
        if cancel.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let mut idle = FileTime::default();
        let mut kernel = FileTime::default();
        let mut user = FileTime::default();
        // SAFETY: all three FILETIME outputs are owned initialized repr(C), 8-byte
        // layouts matching the documented API, valid and non-overlapping.
        let cpu = if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } != 0 {
            crate::platforms::windows::cpu(idle.ticks(), kernel.ticks(), user.ticks())
        } else {
            None
        };
        let mut memory = MemoryStatusEx {
            length: std::mem::size_of::<MemoryStatusEx>() as u32,
            ..MemoryStatusEx::default()
        };
        // SAFETY: dwLength is the owned 64-byte struct size; lifetime spans call.
        let memory = if unsafe { GlobalMemoryStatusEx(&mut memory) } != 0 {
            crate::platforms::windows::MemoryStatus {
                total_phys: memory.total_phys,
                avail_phys: memory.avail_phys,
                total_page_file: memory.total_page_file,
                avail_page_file: memory.avail_page_file,
            }
            .memory()
        } else {
            None
        };
        let mut power = SystemPowerStatus::default();
        // SAFETY: owned initialized 12-byte repr(C) output matches SYSTEM_POWER_STATUS.
        let battery = if unsafe { GetSystemPowerStatus(&mut power) } != 0 {
            crate::platforms::windows::battery(power.ac, power.flags, power.percent)
        } else {
            None
        };
        // SAFETY: documented no-argument scalar call; no pointers or allocations.
        let uptime = unsafe { GetTickCount64() } as f64 / 1000.;
        let mut disks = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (path, label) in paths.iter().take(64) {
            if cancel.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let Some(drive) = path.to_str().and_then(windows_drive) else {
                continue;
            };
            if !seen.insert(drive.clone()) {
                continue;
            }
            let Some(wide) = path.to_str().and_then(windows_wide_path) else {
                continue;
            };
            let root = format!("{}\\", drive);
            let Some(root) = windows_wide_path(&root) else {
                continue;
            };
            // SAFETY: NUL-terminated root UTF-16 is owned for this scalar query.
            // Reject remote mappings before directory capacity lookup.
            if !matches!(unsafe { GetDriveTypeW(root.as_ptr()) }, 2 | 3 | 6) {
                continue;
            }
            let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
            // SAFETY: owned NUL-terminated UTF-16 buffer and aligned u64 outputs
            // live for this synchronous call. No pointer escapes the API.
            if unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, &mut total, &mut free) }
                != 0
                && total > 0
            {
                if let Some(used) = total.checked_sub(available) {
                    disks.push(Disk {
                        label: label.clone(),
                        used,
                        total,
                    });
                }
            }
        }
        let network = network(cancel);
        let processes = processes(cancel);
        // SAFETY: documented scalar API, ALL_PROCESSOR_GROUPS=0xffff, no pointers.
        let cores = core_count(unsafe { GetActiveProcessorCount(0xffff) });
        if cancel.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(RawSample {
            cpu,
            network,
            sample: Sample {
                cores,
                processes,
                memory,
                battery,
                disks,
                uptime_seconds: Some(uptime),
                ..Sample::default()
            },
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::RawSample;
    #[test]
    fn abi_fixed_width_layouts() {
        assert_eq!(std::mem::size_of::<FileTime>(), 8);
        assert_eq!(std::mem::align_of::<FileTime>(), 4);
        assert_eq!(std::mem::size_of::<MemoryStatusEx>(), 64);
        assert_eq!(std::mem::offset_of!(MemoryStatusEx, total_phys), 8);
        assert_eq!(
            std::mem::offset_of!(MemoryStatusEx, avail_extended_virtual),
            56
        );
        assert_eq!(std::mem::size_of::<SystemPowerStatus>(), 12);
        assert_eq!(std::mem::offset_of!(SystemPowerStatus, life), 4);
        assert_eq!(FileTime { high: 1, low: 2 }.ticks(), (1u64 << 32) + 2);
    }
    #[test]
    fn mac_cpu_unsigned_ticks_and_missing_wrap_policy() {
        assert_eq!(mac_cpu_ticks([10, 20, 60, 10]), Some((60, 100)));
        assert_eq!(
            mac_cpu_ticks([u32::MAX; 4]),
            Some((u64::from(u32::MAX), u64::from(u32::MAX) * 4))
        );
        let mut sample = crate::Sampler::new(1);
        sample.sample(
            RawSample {
                cpu: Some((100, 200)),
                ..RawSample::default()
            },
            0.,
        );
        assert_eq!(
            sample
                .sample(
                    RawSample {
                        cpu: Some((1, 2)),
                        ..RawSample::default()
                    },
                    1.
                )
                .cpu_percent,
            None
        );
    }
    #[test]
    fn explicit_windows_unicode_paths_and_namespace_refusal() {
        assert_eq!(windows_drive("c:\\Users\\ação\\日本"), Some("C:".into()));
        assert_eq!(windows_drive("D:/dados"), Some("D:".into()));
        let wide = windows_wide_path("C:\\ação\\日本\\🙂").unwrap();
        assert_eq!(wide.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&wide[..wide.len() - 1]).unwrap(),
            "C:\\ação\\日本\\🙂"
        );
        assert!(windows_wide_path("C:relative").is_none());
        for bad in [
            "C:relative",
            "\\\\server\\share\\",
            "\\\\?\\C:\\",
            "C:\\a\\..\\b",
            "C:\\nul\0",
            "/tmp",
            "",
        ] {
            assert_eq!(windows_drive(bad), None, "{bad:?}");
        }
    }
    #[test]
    fn windows_network_abi_and_hardware_filters() {
        assert_eq!(std::mem::size_of::<MibIfRow2>(), 1352);
        assert_eq!(std::mem::align_of::<MibIfRow2>(), 8);
        assert_eq!(std::mem::offset_of!(MibIfTable2Header, rows), 8);
        assert_eq!(std::mem::offset_of!(MibIfRow2, flags), 1152);
        assert_eq!(std::mem::offset_of!(MibIfRow2, oper_status), 1156);
        assert_eq!(std::mem::offset_of!(MibIfRow2, counters), 1208);
        let mut row = MibIfRow2 {
            flags: 1,
            oper_status: 1,
            kind: 6,
            ..MibIfRow2::default()
        };
        row.counters[0] = 100;
        row.counters[9] = 50;
        for (i, c) in "ação日本".encode_utf16().enumerate() {
            row.alias[i] = c;
        }
        assert_eq!(
            windows_network_rows(&[row, MibIfRow2 { flags: 3, ..row }]),
            Some((100, 50))
        );
        assert_eq!(
            windows_network_rows(&[MibIfRow2 {
                oper_status: 2,
                ..row
            }]),
            None
        );
        assert_eq!(windows_network_rows(&[MibIfRow2 { kind: 24, ..row }]), None);
        assert!(windows_network_rows(&vec![row; 4097]).is_none());
        row.counters[0] = u64::MAX;
        assert!(windows_network_rows(&[row, row]).is_none());
    }
    #[test]
    fn process_bytes_and_core_count_fail_closed() {
        assert_eq!(process_count_bytes(12, 16), Some(3));
        assert_eq!(process_count_bytes(0, 16), Some(0));
        for (n, cap) in [(16, 16), (20, 16), (3, 16), (4, 15)] {
            assert!(process_count_bytes(n, cap).is_none());
        }
        assert_eq!(core_count(128), Some(128));
        assert_eq!(core_count(0), None);
        assert_eq!(core_count(u32::MAX), None);
    }
}
