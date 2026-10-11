//! Host-local calendar boundary. No shell, external commands or global TZ edits.
use crate::Date;
use std::io;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CalendarMoment {
    pub today: Date,
    pub midnight_unix: i64,
    pub now_unix: i64,
}
pub trait CalendarClock: Send + Sync + 'static {
    fn moment(&self) -> io::Result<CalendarMoment>;
}
#[derive(Clone, Copy)]
pub struct FakeClock(pub CalendarMoment);
impl CalendarClock for FakeClock {
    fn moment(&self) -> io::Result<CalendarMoment> {
        Ok(self.0)
    }
}
pub struct SystemLocalClock;
impl CalendarClock for SystemLocalClock {
    fn moment(&self) -> io::Result<CalendarMoment> {
        #[cfg(windows)]
        {
            return crate::windows_calendar::now();
        }
        #[cfg(not(windows))]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_secs();
        #[cfg(not(windows))]
        {
            local_moment(i64::try_from(now).map_err(io::Error::other)?)
        }
    }
}
#[cfg(unix)]
pub fn local_moment(now: i64) -> io::Result<CalendarMoment> {
    #[allow(clippy::useless_conversion)] // time_t can be narrower on other Unix targets.
    let timestamp: libc::time_t = now.try_into().map_err(io::Error::other)?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: timestamp and output are valid pointers to correctly sized libc
    // types; localtime_r initializes output on success and no pointer escapes.
    let result = unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) };
    if result.is_null() {
        return Err(io::Error::other("host local calendar conversion failed"));
    }
    // SAFETY: a non-null successful localtime_r initialized the entire tm.
    let mut local = unsafe { local.assume_init() };
    let year: u16 = (local.tm_year + 1900)
        .try_into()
        .map_err(io::Error::other)?;
    let month: u8 = (local.tm_mon + 1).try_into().map_err(io::Error::other)?;
    let day: u8 = local.tm_mday.try_into().map_err(io::Error::other)?;
    let today =
        Date::new(year, month, day).ok_or_else(|| io::Error::other("invalid host local date"))?;
    local.tm_hour = 0;
    local.tm_min = 0;
    local.tm_sec = 0;
    // DST must be recalculated at midnight, not copied from the current hour.
    local.tm_isdst = -1;
    // SAFETY: mktime mutates only this initialized local tm; libc owns TZ state.
    let midnight = unsafe { libc::mktime(&mut local) };
    if midnight == -1 {
        return Err(io::Error::other("host local midnight conversion failed"));
    }
    Ok(CalendarMoment {
        today,
        midnight_unix: midnight as i64,
        now_unix: now,
    })
}
#[cfg(windows)]
pub fn local_moment(now: i64) -> io::Result<CalendarMoment> {
    crate::windows_calendar::at(now)
}
#[cfg(not(any(unix, windows)))]
pub fn local_moment(_now: i64) -> io::Result<CalendarMoment> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native local calendar is not implemented on this platform",
    ))
}
/// Python-compatible local timestamp for append-only notes.
#[cfg(unix)]
pub fn note_stamp(now: i64) -> io::Result<String> {
    #[allow(clippy::useless_conversion)]
    let timestamp: libc::time_t = now.try_into().map_err(io::Error::other)?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: localtime_r initializes a correctly sized tm on successful return.
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return Err(io::Error::other("local note timestamp unavailable"));
    }
    // SAFETY: successful localtime_r initialized tm.
    let local = unsafe { local.assume_init() };
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        local.tm_year + 1900,
        local.tm_mon + 1,
        local.tm_mday,
        local.tm_hour,
        local.tm_min
    ))
}
#[cfg(not(any(unix, windows)))]
pub fn note_stamp(_: i64) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "local notes persistence currently POSIX-only",
    ))
}

/// Host-local clock candidates, widest to narrowest; never changes process TZ.
#[cfg(unix)]
pub fn clock_candidates(now: i64) -> io::Result<Vec<String>> {
    #[allow(clippy::useless_conversion)]
    let timestamp: libc::time_t = now.try_into().map_err(io::Error::other)?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: valid timestamp/output pointers; successful call initializes tm.
    if unsafe { libc::localtime_r(&timestamp, local.as_mut_ptr()) }.is_null() {
        return Err(io::Error::other("clock unavailable"));
    }
    // SAFETY: successful localtime_r initialized tm.
    let t = unsafe { local.assume_init() };
    let weekday = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"]
        [usize::try_from(t.tm_wday).map_err(io::Error::other)? % 7];
    let time = format!("{:02}:{:02}:{:02}", t.tm_hour, t.tm_min, t.tm_sec);
    let date = format!("{:02}/{:02}", t.tm_mday, t.tm_mon + 1);
    Ok(vec![
        format!(" {weekday} {date}  {time} "),
        format!(" {date} {time} "),
        format!(" {time} "),
        format!(" {:02}:{:02} ", t.tm_hour, t.tm_min),
    ])
}
#[cfg(not(any(unix, windows)))]
pub fn clock_candidates(_: i64) -> io::Result<Vec<String>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "native clock unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_clock_handles_dst_short_long_days_without_24h_assumption() {
        let day = Date::new(2026, 3, 8).unwrap();
        let before = FakeClock(CalendarMoment {
            today: day,
            midnight_unix: 100000,
            now_unix: 100001,
        });
        let after = FakeClock(CalendarMoment {
            today: day.add_days(1).unwrap(),
            midnight_unix: 182800,
            now_unix: 182801,
        });
        assert_eq!(
            after.moment().unwrap().midnight_unix - before.moment().unwrap().midnight_unix,
            23 * 3600
        );
        let fall = FakeClock(CalendarMoment {
            today: Date::new(2026, 11, 1).unwrap(),
            midnight_unix: 200000,
            now_unix: 200001,
        });
        let next = FakeClock(CalendarMoment {
            today: Date::new(2026, 11, 2).unwrap(),
            midnight_unix: 290000,
            now_unix: 290001,
        });
        assert_eq!(
            next.moment().unwrap().midnight_unix - fall.moment().unwrap().midnight_unix,
            25 * 3600
        );
    }
    #[cfg(unix)]
    #[test]
    fn system_clock_native_conversion_round_trips_date() {
        let moment = SystemLocalClock.moment().unwrap();
        assert!(moment.midnight_unix <= moment.now_unix);
        assert_eq!(
            local_moment(moment.midnight_unix).unwrap().today,
            moment.today
        );
    }
}

#[cfg(windows)]
pub fn note_stamp(now: i64) -> io::Result<String> {
    crate::windows_calendar::parts(now)?.note_stamp()
}
#[cfg(windows)]
pub fn clock_candidates(now: i64) -> io::Result<Vec<String>> {
    crate::windows_calendar::parts(now)?.clock_candidates()
}
