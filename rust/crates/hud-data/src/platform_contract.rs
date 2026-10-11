//! Portable contracts. Synthetic backends do not establish native OS support.
use crate::Date;
use std::io;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalParts {
    pub date: Date,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
impl LocalParts {
    pub fn validate(self) -> io::Result<Self> {
        if self.hour > 23 || self.minute > 59 || self.second > 59 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid local time",
            ));
        }
        Ok(self)
    }
    pub fn note_stamp(self) -> io::Result<String> {
        let p = self.validate()?;
        Ok(format!("{} {:02}:{:02}", p.date.iso(), p.hour, p.minute))
    }
    pub fn clock_candidates(self) -> io::Result<Vec<String>> {
        let p = self.validate()?;
        let iso = p.date.iso();
        let date = format!("{}/{}", &iso[8..10], &iso[5..7]);
        let weekday = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"][p.date.weekday()];
        let time = format!("{:02}:{:02}:{:02}", p.hour, p.minute, p.second);
        Ok(vec![
            format!(" {weekday} {date}  {time} "),
            format!(" {date} {time} "),
            format!(" {time} "),
            format!(" {:02}:{:02} ", p.hour, p.minute),
        ])
    }
}
pub trait LocalCalendarBackend {
    fn local_parts(&self, unix_seconds: i64) -> io::Result<LocalParts>;
}
pub struct CalendarAdapter<B>(pub B);
impl<B: LocalCalendarBackend> CalendarAdapter<B> {
    pub fn note_stamp(&self, now: i64) -> io::Result<String> {
        self.0.local_parts(now)?.note_stamp()
    }
    pub fn clock_candidates(&self, now: i64) -> io::Result<Vec<String>> {
        self.0.local_parts(now)?.clock_candidates()
    }
}

/// Required guarantees apply to the same opened handles throughout the operation.
/// Implementations must reject reparse points/symlinks and untrusted ACLs/owners.
/// No native Windows storage backend is provided by this contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCapabilities {
    pub private_owner: bool,
    pub no_links: bool,
    pub handle_relative: bool,
    pub atomic_compare_replace: bool,
}
impl StorageCapabilities {
    fn secure(self) -> bool {
        self.private_owner && self.no_links && self.handle_relative && self.atomic_compare_replace
    }
}
pub trait PrivateStorageBackend {
    fn capabilities(&self) -> StorageCapabilities;
    fn read_bounded(&self, name: &str, limit: usize) -> io::Result<Option<Vec<u8>>>;
    /// Compare exact prior bytes, including absence; preserve disk on mismatch.
    /// Return Ok only after publication; post-publication durability is explicit.
    fn compare_replace(
        &mut self,
        name: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> io::Result<Publication>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Publication {
    pub durability_confirmed: bool,
}
pub struct PrivateStore<B> {
    backend: B,
    limit: usize,
}
impl<B: PrivateStorageBackend> PrivateStore<B> {
    pub fn new(backend: B, limit: usize) -> io::Result<Self> {
        if !backend.capabilities().secure() || limit == 0 || limit > 4 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "secure bounded storage unavailable",
            ));
        }
        Ok(Self { backend, limit })
    }
    pub fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        validate_name(name)?;
        let bytes = self.backend.read_bounded(name, self.limit)?;
        if bytes.as_ref().is_some_and(|b| b.len() > self.limit) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(bytes)
    }
    pub fn save(
        &mut self,
        name: &str,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> io::Result<Publication> {
        validate_name(name)?;
        if replacement.len() > self.limit || expected.is_some_and(|b| b.len() > self.limit) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.backend.compare_replace(name, expected, replacement)
    }
}
fn validate_name(name: &str) -> io::Result<()> {
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "/\\:<>\"|?*".contains(c))
        || reserved.contains(&stem.as_str())
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Synthetic(LocalParts);
    impl LocalCalendarBackend for Synthetic {
        fn local_parts(&self, _: i64) -> io::Result<LocalParts> {
            Ok(self.0)
        }
    }
    #[test]
    fn synthetic_windows_macos_calendar_formats_and_rejects_bad_fields() {
        for date in [
            Date::new(2026, 3, 8).unwrap(),
            Date::new(2026, 11, 1).unwrap(),
        ] {
            let p = LocalParts {
                date,
                hour: 23,
                minute: 59,
                second: 58,
            };
            let a = CalendarAdapter(Synthetic(p));
            assert_eq!(a.note_stamp(0).unwrap(), format!("{} 23:59", date.iso()));
            assert_eq!(
                a.clock_candidates(0).unwrap()[0],
                format!(
                    " dom {}/{}  23:59:58 ",
                    &date.iso()[8..10],
                    &date.iso()[5..7]
                )
            );
            assert!(LocalParts { hour: 24, ..p }.clock_candidates().is_err());
        }
    }
    #[derive(Default)]
    struct Memory(Option<Vec<u8>>);
    impl PrivateStorageBackend for Memory {
        fn capabilities(&self) -> StorageCapabilities {
            StorageCapabilities {
                private_owner: true,
                no_links: true,
                handle_relative: true,
                atomic_compare_replace: true,
            }
        }
        fn read_bounded(&self, _: &str, _: usize) -> io::Result<Option<Vec<u8>>> {
            Ok(self.0.clone())
        }
        fn compare_replace(
            &mut self,
            _: &str,
            expected: Option<&[u8]>,
            replacement: &[u8],
        ) -> io::Result<Publication> {
            if self.0.as_deref() != expected {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            self.0 = Some(replacement.to_vec());
            Ok(Publication {
                durability_confirmed: false,
            })
        }
    }
    #[test]
    fn synthetic_storage_cas_limits_unicode_and_windows_names() {
        let mut s = PrivateStore::new(Memory::default(), 16).unwrap();
        assert!(
            !s.save("ação.md", None, "日本語".as_bytes())
                .unwrap()
                .durability_confirmed
        );
        assert!(s.save("ação.md", None, b"stale").is_err());
        assert_eq!(s.read("ação.md").unwrap().unwrap(), "日本語".as_bytes());
        assert!(s.save("x", None, &[0; 17]).is_err());
        for name in ["../x", "x:y", "CON.md", "LPT9", "x.", "x ", "a/b", "a\\b"] {
            assert!(s.read(name).is_err(), "{name}");
        }
    }
}
