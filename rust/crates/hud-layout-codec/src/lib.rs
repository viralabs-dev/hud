//! HUD layout v1: `HUD-LAYOUT 1\n`, then one comma-separated column per line.
//! Canonical LF termination, ASCII IDs only, no escaping or comments. This is
//! an independent Rust format, not a parser for Python configuration files.
use hud_domain::{Layout, LayoutError, PanelId};
use std::collections::BTreeSet;
use std::fmt;

pub const MAX_BYTES: usize = 16_384;
pub const MAX_COLUMNS: usize = 32;
pub const MAX_PANELS: usize = 256;
pub const MAX_ID_BYTES: usize = 64;
const HEADER: &str = "HUD-LAYOUT 1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodecError {
    TooLarge,
    InvalidColumnLimit,
    UnknownVersion,
    InvalidFormat,
    InvalidId,
    TooManyColumns,
    TooManyPanels,
    DuplicatePanel,
    Domain(LayoutError),
}
impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "layout codec: {self:?}")
    }
}
impl std::error::Error for CodecError {}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_BYTES
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Emits canonical v1 bytes. Application-owned column capacity is intentionally
/// supplied on decode rather than accepted from an untrusted saved document.
pub fn encode(layout: &Layout) -> Result<String, CodecError> {
    if layout.columns().len() > MAX_COLUMNS {
        return Err(CodecError::TooManyColumns);
    }
    let mut output = String::from(HEADER);
    output.push('\n');
    let mut count = 0;
    for column in layout.columns() {
        for (index, id) in column.iter().enumerate() {
            count += 1;
            if count > MAX_PANELS {
                return Err(CodecError::TooManyPanels);
            }
            if !valid_id(id.as_str()) {
                return Err(CodecError::InvalidId);
            }
            if index > 0 {
                output.push(',');
            }
            output.push_str(id.as_str());
            if output.len() > MAX_BYTES {
                return Err(CodecError::TooLarge);
            }
        }
        output.push('\n');
        if output.len() > MAX_BYTES {
            return Err(CodecError::TooLarge);
        }
    }
    Ok(output)
}

/// Parses a bounded document without filesystem operations. Unknown versions,
/// whitespace, CRLF, extra blank lines, duplicate IDs and trailing data fail.
pub fn decode(input: &str, max_columns: usize) -> Result<Layout, CodecError> {
    if max_columns == 0 || max_columns > MAX_COLUMNS {
        return Err(CodecError::InvalidColumnLimit);
    }
    if input.len() > MAX_BYTES {
        return Err(CodecError::TooLarge);
    }
    let (header, body) = input.split_once('\n').ok_or(CodecError::InvalidFormat)?;
    if header != HEADER {
        return Err(if header.starts_with("HUD-LAYOUT ") {
            CodecError::UnknownVersion
        } else {
            CodecError::InvalidFormat
        });
    }
    let body = body.strip_suffix('\n').ok_or(CodecError::InvalidFormat)?;
    if body.is_empty() {
        return Err(CodecError::InvalidFormat);
    }
    let mut columns = Vec::new();
    let mut seen = BTreeSet::new();
    for line in body.split('\n') {
        if columns.len() == max_columns {
            return Err(CodecError::TooManyColumns);
        }
        if line.is_empty() {
            return Err(CodecError::InvalidFormat);
        }
        let mut column = Vec::new();
        for value in line.split(',') {
            if !valid_id(value) {
                return Err(CodecError::InvalidId);
            }
            if seen.len() == MAX_PANELS {
                return Err(CodecError::TooManyPanels);
            }
            if !seen.insert(value) {
                return Err(CodecError::DuplicatePanel);
            }
            column.push(PanelId::new(value).map_err(CodecError::Domain)?);
        }
        columns.push(column);
    }
    Layout::new(columns, max_columns).map_err(CodecError::Domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(value: &str) -> PanelId {
        PanelId::new(value).unwrap()
    }
    #[test]
    fn canonical_roundtrip_preserves_order_and_capacity() {
        let layout = Layout::new(
            vec![vec![id("agents"), id("notes-2")], vec![id("shell_1")]],
            3,
        )
        .unwrap();
        let encoded = encode(&layout).unwrap();
        assert_eq!(encoded, "HUD-LAYOUT 1\nagents,notes-2\nshell_1\n");
        assert_eq!(decode(&encoded, 3).unwrap(), layout);
        assert_eq!(encode(&decode(&encoded, 3).unwrap()).unwrap(), encoded);
    }
    #[test]
    fn rejects_version_and_structural_corruption() {
        assert_eq!(
            decode("HUD-LAYOUT 2\na\n", 3),
            Err(CodecError::UnknownVersion)
        );
        for input in [
            "",
            "HUD-LAYOUT 1",
            "HUD-LAYOUT 1\n",
            "HUD-LAYOUT 1\na",
            "HUD-LAYOUT 1\na\n\n",
            "HUD-LAYOUT 1\r\na\r\n",
            "garbage\na\n",
        ] {
            assert!(decode(input, 3).is_err(), "accepted {input:?}");
        }
    }
    #[test]
    fn rejects_ambiguous_and_hostile_ids() {
        for value in [
            "",
            " a",
            "a ",
            "../secret",
            "a/b",
            "a\\b",
            "a\r",
            "á",
            "a\0",
            "a\tb",
            "a=b",
            "a#b",
        ] {
            assert!(
                decode(&format!("HUD-LAYOUT 1\n{value}\n"), 3).is_err(),
                "accepted {value:?}"
            );
        }
        assert_eq!(
            decode("HUD-LAYOUT 1\na,a\n", 3),
            Err(CodecError::DuplicatePanel)
        );
        assert_eq!(
            decode("HUD-LAYOUT 1\na\na\n", 3),
            Err(CodecError::DuplicatePanel)
        );
        assert_eq!(decode("HUD-LAYOUT 1\na,\n", 3), Err(CodecError::InvalidId));
        assert_eq!(
            decode(&format!("HUD-LAYOUT 1\n{}\n", "a".repeat(65)), 3),
            Err(CodecError::InvalidId)
        );
    }
    #[test]
    fn enforces_all_resource_limits() {
        assert_eq!(
            decode(&"x".repeat(MAX_BYTES + 1), 3),
            Err(CodecError::TooLarge)
        );
        assert_eq!(
            decode("HUD-LAYOUT 1\na\nb\n", 1),
            Err(CodecError::TooManyColumns)
        );
        assert_eq!(
            decode("HUD-LAYOUT 1\na\n", 0),
            Err(CodecError::InvalidColumnLimit)
        );
        assert_eq!(
            decode("HUD-LAYOUT 1\na\n", MAX_COLUMNS + 1),
            Err(CodecError::InvalidColumnLimit)
        );
        let ids = (0..MAX_PANELS + 1)
            .map(|n| format!("p{n}"))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            decode(&format!("HUD-LAYOUT 1\n{ids}\n"), 3),
            Err(CodecError::TooManyPanels)
        );
    }
    #[test]
    fn encode_rejects_domain_ids_outside_persistence_contract() {
        let layout = Layout::new(vec![vec![id("folder/name")]], 3).unwrap();
        assert_eq!(encode(&layout), Err(CodecError::InvalidId));
    }
}
