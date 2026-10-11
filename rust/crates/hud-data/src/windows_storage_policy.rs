//! Conservative Windows security policy, executable with binary fixtures on Linux.
//! No native handles, FFI, filesystem writes, or Windows support claim.
//! Only protected owner-only DACLs with ordinary explicit allow ACEs are accepted.
use std::io;
const MAX_DESCRIPTOR: usize = 65_536;
const MAX_ACES: usize = 64;
const MAX_CHAIN: usize = 64;
pub const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "unsupported or unsafe Windows storage evidence",
    )
}
fn range(bytes: &[u8], offset: usize, length: usize) -> io::Result<&[u8]> {
    bytes
        .get(offset..offset.checked_add(length).ok_or_else(invalid)?)
        .ok_or_else(invalid)
}
fn word(bytes: &[u8], offset: usize) -> io::Result<u16> {
    let b = range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
fn dword(bytes: &[u8], offset: usize) -> io::Result<u32> {
    let b = range(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sid(Vec<u8>);
impl Sid {
    /// Exact SID encoding: revision 1, 1..15 subauthorities, no trailing bytes.
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 8
            || bytes[0] != 1
            || !(1..=15).contains(&bytes[1])
            || bytes.len() != 8 + usize::from(bytes[1]) * 4
        {
            return Err(invalid());
        }
        Ok(Self(bytes.to_vec()))
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}
fn sid_at(bytes: &[u8], offset: usize) -> io::Result<(Sid, usize)> {
    if offset < 20 || !offset.is_multiple_of(4) {
        return Err(invalid());
    }
    let header = range(bytes, offset, 8)?;
    let size = 8 + usize::from(header[1]) * 4;
    Ok((Sid::parse(range(bytes, offset, size)?)?, size))
}
/// Validates an entire self-relative descriptor; rejects unsupported ACE/control
/// forms rather than attempting Windows effective-access evaluation.
pub fn validate_private_descriptor(bytes: &[u8], current_user: &Sid) -> io::Result<()> {
    if bytes.len() < 20 || bytes.len() > MAX_DESCRIPTOR || bytes[0] != 1 || bytes[1] != 0 {
        return Err(invalid());
    }
    // SELF_RELATIVE | DACL_PROTECTED | DACL_PRESENT, with no unsupported flags.
    if word(bytes, 2)? != 0x9004 || dword(bytes, 12)? != 0 {
        return Err(invalid());
    }
    let owner_offset = dword(bytes, 4)? as usize;
    let (owner, owner_size) = sid_at(bytes, owner_offset)?;
    if &owner != current_user {
        return Err(invalid());
    }
    let mut occupied = vec![(0, 20), (owner_offset, owner_offset + owner_size)];
    let group_offset = dword(bytes, 8)? as usize;
    if group_offset != 0 {
        let (_, size) = sid_at(bytes, group_offset)?;
        occupied.push((group_offset, group_offset + size));
    }
    let acl_offset = dword(bytes, 16)? as usize;
    if acl_offset < 20 || !acl_offset.is_multiple_of(4) {
        return Err(invalid());
    } // null DACL denied
    let header = range(bytes, acl_offset, 8)?;
    if header[0] != 2 || header[1] != 0 || word(header, 6)? != 0 {
        return Err(invalid());
    }
    let size = usize::from(word(header, 2)?);
    let count = usize::from(word(header, 4)?);
    if size < 8 || count == 0 || count > MAX_ACES {
        return Err(invalid());
    }
    let acl = range(bytes, acl_offset, size)?;
    occupied.push((acl_offset, acl_offset + size));
    occupied.sort_unstable();
    if occupied.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err(invalid());
    }
    // Gaps/padding are allowed only when zero; no ignored hidden payload.
    let mut end = 0;
    for &(start, next) in &occupied {
        if bytes[end..start].iter().any(|b| *b != 0) {
            return Err(invalid());
        }
        end = next;
    }
    if bytes[end..].iter().any(|b| *b != 0) {
        return Err(invalid());
    }
    let mut cursor = 8;
    for _ in 0..count {
        let h = range(acl, cursor, 8)?;
        // ACCESS_ALLOWED_ACE only, no inheritance/object/callback semantics.
        if h[0] != 0 || h[1] != 0 {
            return Err(invalid());
        }
        let ace_size = usize::from(word(h, 2)?);
        let ace = range(acl, cursor, ace_size)?;
        if ace_size < 20 || !ace_size.is_multiple_of(4) {
            return Err(invalid());
        }
        let mask = dword(ace, 4)?;
        // File-specific, standard, and generic rights only; no unknown bits.
        if mask == 0 || mask & !0xf01f_01ff != 0 || Sid::parse(&ace[8..])? != *current_user {
            return Err(invalid());
        }
        cursor += ace_size;
    }
    if cursor != acl.len() {
        return Err(invalid());
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FileIdentity {
    pub volume: u64,
    pub file_id: [u8; 16],
}
/// Evidence must be obtained from the same opened handle, not path metadata.
/// The future native adapter must keep every handle alive through commit.
#[derive(Clone, Debug)]
pub struct HandleEvidence {
    pub identity: FileIdentity,
    pub directory: bool,
    pub disk_file: bool,
    pub attributes: u32,
    pub reparse_tag: u32,
    pub links: u32,
    pub security_descriptor: Vec<u8>,
}
fn validate_handle(e: &HandleEvidence, user: &Sid, directory: bool) -> io::Result<()> {
    if !e.disk_file
        || e.directory != directory
        || (e.attributes & 0x10 != 0) != directory
        || e.attributes & 0x400 != 0
        || e.reparse_tag != 0
        || (!directory && e.links != 1)
        || e.identity.file_id == [0; 16]
    {
        return Err(invalid());
    }
    validate_private_descriptor(&e.security_descriptor, user)
}
#[derive(Clone, Debug)]
pub struct PinnedChain {
    user: Sid,
    identities: Vec<FileIdentity>,
}
impl PinnedChain {
    pub fn capture(user: Sid, directories: &[HandleEvidence]) -> io::Result<Self> {
        if directories.is_empty() || directories.len() > MAX_CHAIN {
            return Err(invalid());
        }
        let mut identities = Vec::with_capacity(directories.len());
        for e in directories {
            validate_handle(e, &user, true)?;
            if e.identity.volume != directories[0].identity.volume
                || identities.contains(&e.identity)
            {
                return Err(invalid());
            }
            identities.push(e.identity);
        }
        Ok(Self { user, identities })
    }
    pub fn revalidate(&self, directories: &[HandleEvidence]) -> io::Result<()> {
        let next = Self::capture(self.user.clone(), directories)?;
        if self.identities != next.identities {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Review token binds exact bytes, file identity, and the complete parent chain.
/// This is not an atomic commit primitive; callers still need native exclusion
/// and handle-relative publication. An unsupported adapter must refuse writes.
#[derive(Clone, Debug)]
pub struct FilePreview {
    chain: PinnedChain,
    identity: FileIdentity,
    bytes: Vec<u8>,
}
impl FilePreview {
    pub fn capture(chain: PinnedChain, file: &HandleEvidence, bytes: Vec<u8>) -> io::Result<Self> {
        validate_handle(file, &chain.user, false)?;
        if bytes.len() > MAX_PREVIEW_BYTES || file.identity.volume != chain.identities[0].volume {
            return Err(invalid());
        }
        Ok(Self {
            chain,
            identity: file.identity,
            bytes,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn revalidate(
        &self,
        directories: &[HandleEvidence],
        file: &HandleEvidence,
        bytes: &[u8],
    ) -> io::Result<()> {
        self.chain.revalidate(directories)?;
        validate_handle(file, &self.chain.user, false)?;
        if file.identity != self.identity || bytes != self.bytes {
            return Err(invalid());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sid(value: u32) -> Sid {
        let mut b = vec![1, 1, 0, 0, 0, 0, 0, 5];
        b.extend(value.to_le_bytes());
        Sid::parse(&b).unwrap()
    }
    fn descriptor(user: &Sid) -> Vec<u8> {
        let mut b = vec![0; 20];
        b[0] = 1;
        b[2..4].copy_from_slice(&0x9004u16.to_le_bytes());
        b[4..8].copy_from_slice(&20u32.to_le_bytes());
        b.extend(user.bytes());
        let offset = b.len() as u32;
        b[16..20].copy_from_slice(&offset.to_le_bytes());
        let ace_size = 8 + user.bytes().len();
        let acl_size = 8 + ace_size;
        b.extend([2, 0]);
        b.extend((acl_size as u16).to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend([0, 0]);
        b.extend([0, 0]);
        b.extend((ace_size as u16).to_le_bytes());
        b.extend(0x001f01ffu32.to_le_bytes());
        b.extend(user.bytes());
        b
    }
    fn evidence(id: u8, directory: bool) -> HandleEvidence {
        HandleEvidence {
            identity: FileIdentity {
                volume: 7,
                file_id: [id; 16],
            },
            directory,
            disk_file: true,
            attributes: if directory { 0x10 } else { 0 },
            reparse_tag: 0,
            links: 1,
            security_descriptor: descriptor(&sid(42)),
        }
    }
    #[test]
    fn valid_private_descriptor_and_every_truncation() {
        let user = sid(42);
        let b = descriptor(&user);
        validate_private_descriptor(&b, &user).unwrap();
        for n in 0..b.len() {
            assert!(validate_private_descriptor(&b[..n], &user).is_err(), "{n}");
        }
        assert!(validate_private_descriptor(&b, &sid(43)).is_err());
        assert!(Sid::parse(&[1; 100]).is_err());
    }
    #[test]
    fn null_dacl_unprotected_overlap_unknown_and_foreign_aces_denied() {
        let user = sid(42);
        let original = descriptor(&user);
        for (offset, value) in [
            (2, 0),
            (3, 0x80),
            (16, 0),
            (32, 4),
            (40, 1),
            (41, 0x10),
            (46, 0x80),
            (59, 43),
        ] {
            let mut b = original.clone();
            b[offset] = value;
            assert!(
                validate_private_descriptor(&b, &user).is_err(),
                "offset {offset}"
            );
        }
        let mut b = original.clone();
        b[16..20].copy_from_slice(&20u32.to_le_bytes());
        assert!(validate_private_descriptor(&b, &user).is_err());
        let mut b = original;
        b.extend([1]);
        assert!(validate_private_descriptor(&b, &user).is_err());
    }
    #[test]
    fn binary_limits_group_overlap_and_mutations_never_panic() {
        let user = sid(42);
        let original = descriptor(&user);
        assert!(validate_private_descriptor(&vec![0; MAX_DESCRIPTOR + 1], &user).is_err());
        let mut group = original.clone();
        group[8..12].copy_from_slice(&20u32.to_le_bytes());
        assert!(validate_private_descriptor(&group, &user).is_err());
        let mut group = original.clone();
        let offset = group.len() as u32;
        group.extend(sid(7).bytes());
        group[8..12].copy_from_slice(&offset.to_le_bytes());
        validate_private_descriptor(&group, &user).unwrap();
        for position in 0..original.len() {
            for value in 0..=255 {
                let mut bytes = original.clone();
                bytes[position] = value;
                let _ = validate_private_descriptor(&bytes, &user);
            }
        }
        assert!(Sid::parse(&[1, 16, 0, 0, 0, 0, 0, 5]).is_err());
        assert!(PinnedChain::capture(user, &vec![evidence(1, true); MAX_CHAIN + 1]).is_err());
    }
    #[test]
    fn chain_and_preview_reject_replacements_stale_bytes_and_unsafe_objects() {
        let directories = vec![evidence(1, true), evidence(2, true)];
        let file = evidence(3, false);
        let chain = PinnedChain::capture(sid(42), &directories).unwrap();
        let preview =
            FilePreview::capture(chain, &file, "ação 日本語".as_bytes().to_vec()).unwrap();
        preview
            .revalidate(&directories, &file, preview.bytes())
            .unwrap();
        let mut replaced = file.clone();
        replaced.identity.file_id = [9; 16];
        assert!(preview
            .revalidate(&directories, &replaced, preview.bytes())
            .is_err());
        let mut changed = directories.clone();
        changed[1].identity.file_id = [4; 16];
        assert!(preview
            .revalidate(&changed, &file, preview.bytes())
            .is_err());
        assert!(preview.revalidate(&directories, &file, b"manual").is_err());
        for bad in 0..5 {
            let mut f = file.clone();
            match bad {
                0 => f.links = 2,
                1 => f.reparse_tag = 1,
                2 => f.attributes = 0x400,
                3 => f.disk_file = false,
                _ => f.identity.volume = 8,
            }
            assert!(preview
                .revalidate(&directories, &f, preview.bytes())
                .is_err());
        }
        let mut changed = directories.clone();
        changed[0].security_descriptor = descriptor(&sid(99));
        assert!(preview
            .revalidate(&changed, &file, preview.bytes())
            .is_err());
        assert!(PinnedChain::capture(sid(42), &[]).is_err());
        assert!(PinnedChain::capture(sid(42), &[evidence(1, true), evidence(1, true)]).is_err());
        assert!(FilePreview::capture(
            PinnedChain::capture(sid(42), &directories).unwrap(),
            &file,
            vec![0; MAX_PREVIEW_BYTES + 1]
        )
        .is_err());
    }
}
