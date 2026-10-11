use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};
#[cfg(unix)]
pub fn open(path: &Path, private: bool, follow: bool) -> io::Result<File> {
    use rustix::fs::{self, FileType, Mode, OFlags};
    let mut flags = OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC;
    if !follow {
        flags |= OFlags::NOFOLLOW;
    }
    let fd = if !follow {
        use std::path::Component;
        if path.components().any(|x| matches!(x, Component::ParentDir)) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut parent = fs::open(
            if path.is_absolute() {
                Path::new("/")
            } else {
                Path::new(".")
            },
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let parts: Vec<_> = path
            .components()
            .filter_map(|x| {
                if let Component::Normal(n) = x {
                    Some(n)
                } else {
                    None
                }
            })
            .collect();
        let (last, ancestors) = parts.split_last().ok_or(io::ErrorKind::InvalidInput)?;
        for name in ancestors {
            parent = fs::openat(
                &parent,
                *name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
        }
        fs::openat(&parent, *last, flags, Mode::empty())?
    } else {
        fs::open(path, flags, Mode::empty())?
    };
    let st = fs::fstat(&fd)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || (private && (st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o022 != 0))
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(File::from(fd))
}
#[cfg(not(unix))]
pub fn open(_: &Path, _: bool, _: bool) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "safe resource reader not implemented on this platform",
    ))
}
pub fn head(path: &Path, limit: usize, exact: bool, follow: bool) -> io::Result<String> {
    let file = open(path, !follow, follow)?;
    let mut data = Vec::new();
    file.take((limit + 1) as u64).read_to_end(&mut data)?;
    if data.len() > limit {
        if exact {
            return Err(io::ErrorKind::InvalidData.into());
        }
        data.truncate(limit);
    }
    if exact {
        String::from_utf8(data).map_err(|_| io::ErrorKind::InvalidData.into())
    } else {
        Ok(String::from_utf8_lossy(&data).into_owned())
    }
}
pub fn tail(path: &Path, limit: usize) -> io::Result<String> {
    let mut file = open(path, true, false)?;
    let size = file.metadata()?.len();
    let start = size.saturating_sub(limit as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut data = Vec::new();
    file.take(limit as u64).read_to_end(&mut data)?;
    if start > 0 {
        data = data.split_once_byte(b'\n');
    }
    Ok(String::from_utf8_lossy(&data).into_owned())
}
trait SplitByte {
    fn split_once_byte(self, byte: u8) -> Self;
}
impl SplitByte for Vec<u8> {
    fn split_once_byte(self, byte: u8) -> Self {
        match self.iter().position(|b| *b == byte) {
            Some(i) => self[i + 1..].to_vec(),
            None => Vec::new(),
        }
    }
}
