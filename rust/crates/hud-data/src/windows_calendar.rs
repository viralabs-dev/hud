//! Typed Windows calendar boundary; portable arithmetic/ABI tests run on Linux.
//! Native code remains uncompiled and unexecuted outside Windows.
#[cfg(windows)]
use crate::CalendarMoment;
use crate::Date;
use std::io;
const TICKS_PER_SECOND: i128 = 10_000_000;
const WINDOWS_EPOCH_SECONDS: i128 = 11_644_473_600;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SystemTimeFields {
    year: u16,
    month: u16,
    weekday: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FileTime {
    low: u32,
    high: u32,
}
fn unix_seconds(file: FileTime) -> io::Result<i64> {
    let ticks = (u64::from(file.high) << 32) | u64::from(file.low);
    let seconds = i128::from(ticks) / TICKS_PER_SECOND - WINDOWS_EPOCH_SECONDS;
    i64::try_from(seconds).map_err(io::Error::other)
}
fn file_time(seconds: i64) -> io::Result<FileTime> {
    let ticks = (i128::from(seconds) + WINDOWS_EPOCH_SECONDS)
        .checked_mul(TICKS_PER_SECOND)
        .and_then(|ticks| u64::try_from(ticks).ok())
        .ok_or(io::ErrorKind::InvalidInput)?;
    Ok(FileTime {
        low: ticks as u32,
        high: (ticks >> 32) as u32,
    })
}
fn date(local: SystemTimeFields) -> io::Result<Date> {
    Date::new(
        local.year,
        local.month.try_into().map_err(io::Error::other)?,
        local.day.try_into().map_err(io::Error::other)?,
    )
    .ok_or_else(|| io::Error::other("invalid Windows local date"))
}
fn midnight(mut local: SystemTimeFields) -> io::Result<SystemTimeFields> {
    date(local)?;
    local.hour = 0;
    local.minute = 0;
    local.second = 0;
    local.milliseconds = 0;
    Ok(local)
}
#[cfg(windows)]
mod native {
    use super::*;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocalTime(output: *mut SystemTimeFields);
        fn GetSystemTimeAsFileTime(output: *mut FileTime);
        fn FileTimeToSystemTime(input: *const FileTime, output: *mut SystemTimeFields) -> i32;
        fn SystemTimeToFileTime(input: *const SystemTimeFields, output: *mut FileTime) -> i32;
        fn SystemTimeToTzSpecificLocalTime(
            zone: *const std::ffi::c_void,
            utc: *const SystemTimeFields,
            local: *mut SystemTimeFields,
        ) -> i32;
        fn TzSpecificLocalTimeToSystemTime(
            zone: *const std::ffi::c_void,
            local: *const SystemTimeFields,
            utc: *mut SystemTimeFields,
        ) -> i32;
    }
    fn from_local(local: SystemTimeFields, now_unix: i64) -> io::Result<CalendarMoment> {
        let today = date(local)?;
        let local_midnight = midnight(local)?;
        let mut utc = SystemTimeFields::default();
        // SAFETY: input/output point to initialized repr(C) SYSTEMTIME buffers;
        // null timezone requests the host's current Windows timezone. No mutation.
        if unsafe { TzSpecificLocalTimeToSystemTime(std::ptr::null(), &local_midnight, &mut utc) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut roundtrip = SystemTimeFields::default();
        // SAFETY: valid initialized repr(C) buffers. Reject a nonexistent local
        // midnight rather than silently choose a different civil date/hour.
        if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut roundtrip) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if date(roundtrip)? != today
            || (roundtrip.hour, roundtrip.minute, roundtrip.second) != (0, 0, 0)
        {
            return Err(io::Error::other(
                "Windows local midnight did not round-trip in current timezone",
            ));
        }
        let mut stamp = FileTime::default();
        // SAFETY: SYSTEMTIME/FILETIME pointers are valid for the entire call.
        if unsafe { SystemTimeToFileTime(&utc, &mut stamp) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(CalendarMoment {
            today,
            midnight_unix: unix_seconds(stamp)?,
            now_unix,
        })
    }
    fn local_fields(now: i64) -> io::Result<SystemTimeFields> {
        let stamp = file_time(now)?;
        let mut utc = SystemTimeFields::default();
        let mut local = SystemTimeFields::default();
        // SAFETY: correctly sized initialized repr(C) buffers; no borrowed memory
        // escapes the call. API results checked before observing output fields.
        if unsafe { FileTimeToSystemTime(&stamp, &mut utc) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pointers valid; null timezone selects current host TZ rules.
        if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(local)
    }
    pub(crate) fn at(now: i64) -> io::Result<CalendarMoment> {
        from_local(local_fields(now)?, now)
    }
    pub(crate) fn parts(now: i64) -> io::Result<crate::platform_contract::LocalParts> {
        let local = local_fields(now)?;
        crate::platform_contract::LocalParts {
            date: date(local)?,
            hour: local.hour.try_into().map_err(io::Error::other)?,
            minute: local.minute.try_into().map_err(io::Error::other)?,
            second: local.second.try_into().map_err(io::Error::other)?,
        }
        .validate()
    }
    pub(crate) fn now() -> io::Result<CalendarMoment> {
        // Date rollover between local and UTC reads must not mix two local days.
        for _ in 0..3 {
            let mut before = SystemTimeFields::default();
            let mut after = SystemTimeFields::default();
            let mut stamp = FileTime::default();
            // SAFETY: Windows void APIs fill writable initialized buffers of their
            // documented repr(C) types; no pointers escape or global TZ is changed.
            unsafe {
                GetLocalTime(&mut before);
                GetSystemTimeAsFileTime(&mut stamp);
                GetLocalTime(&mut after);
            }
            if date(before)? == date(after)? {
                return from_local(after, unix_seconds(stamp)?);
            }
        }
        Err(io::Error::other(
            "Windows local date changed repeatedly during clock sample",
        ))
    }
}
#[cfg(windows)]
pub(crate) use native::{at, now, parts};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_abi_layout_and_epoch_conversion_checked() {
        assert_eq!(std::mem::size_of::<SystemTimeFields>(), 16);
        assert_eq!(std::mem::align_of::<SystemTimeFields>(), 2);
        assert_eq!(std::mem::size_of::<FileTime>(), 8);
        assert_eq!(std::mem::align_of::<FileTime>(), 4);
        assert_eq!(unix_seconds(FileTime::default()).unwrap(), -11_644_473_600);
        for seconds in [-11_644_473_600, -1, 0, 1, 1_772_953_200, 253_402_300_799] {
            assert_eq!(unix_seconds(file_time(seconds).unwrap()).unwrap(), seconds);
        }
        assert!(file_time(-11_644_473_601).is_err());
        assert!(file_time(i64::MAX).is_err());
    }
    #[test]
    fn windows_local_date_validation_and_midnight_fields() {
        let local = SystemTimeFields {
            year: 2026,
            month: 3,
            day: 8,
            hour: 23,
            minute: 59,
            second: 59,
            milliseconds: 999,
            ..SystemTimeFields::default()
        };
        let midnight = midnight(local).unwrap();
        assert_eq!(date(midnight).unwrap().iso(), "2026-03-08");
        assert_eq!(
            (
                midnight.hour,
                midnight.minute,
                midnight.second,
                midnight.milliseconds
            ),
            (0, 0, 0, 0)
        );
        assert!(date(SystemTimeFields {
            year: 2026,
            month: 2,
            day: 29,
            ..local
        })
        .is_err());
        assert!(date(SystemTimeFields {
            month: 256,
            ..local
        })
        .is_err());
    }
}
