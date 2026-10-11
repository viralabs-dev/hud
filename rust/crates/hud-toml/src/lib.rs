//! Bounded TOML subset shared by layout and command import. No IO or execution.
//! Plain keys, flat tables/array tables, strings, arrays, inline tables, booleans, numbers.
//! Unsupported syntax is an explicit error; callers enforce their own schemas.
use serde_json::{Map, Value};
use std::io;
fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid TOML")
}
pub fn parse(bytes: &[u8], max_bytes: usize) -> io::Result<Value> {
    if bytes.len() > max_bytes {
        return Err(bad());
    }
    std::str::from_utf8(bytes).map_err(|_| bad())?;
    Ok(Value::Object(Parser { raw: bytes, pos: 0 }.parse()?))
}
struct Parser<'a> {
    raw: &'a [u8],
    pos: usize,
}
impl<'a> Parser<'a> {
    fn err<T>(&self) -> io::Result<T> {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported or invalid TOML near byte {}", self.pos),
        ))
    }
    fn whitespace(&mut self, newlines: bool) {
        loop {
            while self.raw.get(self.pos).is_some_and(|b| {
                *b == b' ' || *b == b'\t' || *b == b'\r' || (newlines && *b == b'\n')
            }) {
                self.pos += 1
            }
            if self.raw.get(self.pos) == Some(&b'#') {
                while self.raw.get(self.pos).is_some_and(|b| *b != b'\n') {
                    self.pos += 1
                }
            } else {
                break;
            }
            if !newlines {
                break;
            }
        }
    }
    fn take(&mut self, b: u8) -> bool {
        if self.raw.get(self.pos) == Some(&b) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn key(&mut self) -> io::Result<String> {
        let start = self.pos;
        while self
            .raw
            .get(self.pos)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        {
            self.pos += 1
        }
        if start == self.pos {
            return self.err();
        }
        Ok(std::str::from_utf8(&self.raw[start..self.pos])
            .map_err(|_| bad())?
            .to_owned())
    }
    fn quoted(&mut self) -> io::Result<String> {
        let quote = self.raw[self.pos];
        self.pos += 1;
        if self.raw.get(self.pos) == Some(&quote) && self.raw.get(self.pos + 1) == Some(&quote) {
            return self.err();
        }
        let mut out = String::new();
        while self.pos < self.raw.len() {
            let b = self.raw[self.pos];
            self.pos += 1;
            if b == quote {
                return Ok(out);
            }
            if b < 0x20 {
                return self.err();
            }
            if b == b'\\' && quote == b'"' {
                let e = *self.raw.get(self.pos).ok_or_else(bad)?;
                self.pos += 1;
                match e {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'b' => out.push('\u{8}'),
                    b't' => out.push('\t'),
                    b'n' => out.push('\n'),
                    b'f' => out.push('\u{c}'),
                    b'r' => out.push('\r'),
                    b'u' | b'U' => {
                        let n = if e == b'u' { 4 } else { 8 };
                        let bytes = self.raw.get(self.pos..self.pos + n).ok_or_else(bad)?;
                        if !bytes.iter().all(u8::is_ascii_hexdigit) {
                            return self.err();
                        }
                        let scalar =
                            u32::from_str_radix(std::str::from_utf8(bytes).map_err(|_| bad())?, 16)
                                .map_err(|_| bad())?;
                        out.push(char::from_u32(scalar).ok_or_else(bad)?);
                        self.pos += n
                    }
                    _ => return self.err(),
                }
            } else {
                self.pos -= 1;
                let c = std::str::from_utf8(&self.raw[self.pos..])
                    .map_err(|_| bad())?
                    .chars()
                    .next()
                    .ok_or_else(bad)?;
                out.push(c);
                self.pos += c.len_utf8();
            }
        }
        self.err()
    }
    fn value(&mut self, depth: usize) -> io::Result<Value> {
        if depth > 8 {
            return self.err();
        }
        self.whitespace(true);
        match self.raw.get(self.pos) {
            Some(b'"' | b'\'') => Ok(Value::String(self.quoted()?)),
            Some(b't') if self.raw[self.pos..].starts_with(b"true") => {
                self.pos += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.raw[self.pos..].starts_with(b"false") => {
                self.pos += 5;
                Ok(Value::Bool(false))
            }
            Some(b'[') => {
                self.pos += 1;
                let mut values = Vec::new();
                loop {
                    self.whitespace(true);
                    if self.take(b']') {
                        break;
                    }
                    values.push(self.value(depth + 1)?);
                    if values.len() > 256 {
                        return self.err();
                    }
                    self.whitespace(true);
                    if self.take(b']') {
                        break;
                    }
                    if !self.take(b',') {
                        return self.err();
                    }
                }
                Ok(Value::Array(values))
            }
            Some(b'{') => {
                self.pos += 1;
                let mut m = Map::new();
                loop {
                    self.whitespace(false);
                    if self.take(b'}') {
                        break;
                    }
                    let key = self.key()?;
                    self.whitespace(false);
                    if !self.take(b'=') {
                        return self.err();
                    }
                    let value = self.value(depth + 1)?;
                    if m.insert(key, value).is_some() {
                        return self.err();
                    }
                    self.whitespace(false);
                    if self.take(b'}') {
                        break;
                    }
                    if !self.take(b',') {
                        return self.err();
                    }
                    self.whitespace(false);
                    if self.raw.get(self.pos) == Some(&b'}') {
                        return self.err();
                    }
                }
                Ok(Value::Object(m))
            }
            Some(_) => {
                let start = self.pos;
                while self.raw.get(self.pos).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E')
                }) {
                    self.pos += 1
                }
                if start == self.pos {
                    return self.err();
                }
                let s = std::str::from_utf8(&self.raw[start..self.pos]).map_err(|_| bad())?;
                let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
                let mut exps = unsigned.split(['e', 'E']);
                let mantissa = exps.next().ok_or_else(bad)?;
                if let Some(exponent) = exps.next() {
                    let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                    if exponent.is_empty()
                        || !exponent.bytes().all(|b| b.is_ascii_digit())
                        || exps.next().is_some()
                    {
                        return self.err();
                    }
                }
                let mut parts = mantissa.split('.');
                let whole = parts.next().ok_or_else(bad)?;
                if whole.is_empty()
                    || !whole.bytes().all(|b| b.is_ascii_digit())
                    || (whole.len() > 1 && whole.starts_with('0'))
                {
                    return self.err();
                }
                if let Some(fraction) = parts.next() {
                    if fraction.is_empty()
                        || !fraction.bytes().all(|b| b.is_ascii_digit())
                        || parts.next().is_some()
                    {
                        return self.err();
                    }
                }
                if let Ok(n) = s.parse::<i64>() {
                    return Ok(serde_json::json!(n));
                }
                let n = s.parse::<f64>().map_err(|_| bad())?;
                if !n.is_finite() {
                    return self.err();
                }
                Ok(serde_json::json!(n))
            }
            None => self.err(),
        }
    }
    fn parse(mut self) -> io::Result<Map<String, Value>> {
        let mut root = Map::new();
        let mut section: Option<(String, Option<usize>)> = None;
        loop {
            self.whitespace(true);
            if self.pos == self.raw.len() {
                break;
            }
            if self.take(b'[') {
                let array = self.take(b'[');
                let name = self.key()?;
                if !self.take(b']') || (array && !self.take(b']')) {
                    return self.err();
                }
                if array {
                    let list = root
                        .entry(name.clone())
                        .or_insert_with(|| Value::Array(Vec::new()))
                        .as_array_mut()
                        .ok_or_else(bad)?;
                    list.push(Value::Object(Map::new()));
                    if list.len() > 32 {
                        return self.err();
                    }
                    section = Some((name, Some(list.len() - 1)));
                } else {
                    if root.contains_key(&name) {
                        return self.err();
                    }
                    root.insert(name.clone(), Value::Object(Map::new()));
                    section = Some((name, None));
                }
            } else {
                let key = self.key()?;
                self.whitespace(false);
                if !self.take(b'=') {
                    return self.err();
                }
                let value = self.value(0)?;
                let target = match &section {
                    Some((name, Some(index))) => root
                        .get_mut(name)
                        .and_then(Value::as_array_mut)
                        .and_then(|rows| rows.get_mut(*index))
                        .and_then(Value::as_object_mut)
                        .ok_or_else(bad)?,
                    Some((name, None)) => root
                        .get_mut(name)
                        .and_then(Value::as_object_mut)
                        .ok_or_else(bad)?,
                    None => &mut root,
                };
                if target.insert(key, value).is_some() {
                    return self.err();
                }
            }
            self.whitespace(false);
            if self.pos != self.raw.len() && !self.take(b'\n') {
                return self.err();
            }
        }
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_subset_refuses_duplicate_unknown_and_deep_syntax() {
        let v = parse(
            b"[[command]]\nname='Z'\nconfirm=true\nargv=['echo','literal']\n[[command]]\nname='A'",
            4096,
        )
        .unwrap();
        assert_eq!(v["command"][0]["name"], "Z");
        assert_eq!(v["command"][1]["name"], "A");
        for raw in [
            "x=1\nx=2",
            "x.y=3",
            "[a.b]\nx=1",
            "x=nan",
            "x=2026-10-11",
            "x=[[[[[[[[[[1]]]]]]]]]]",
        ] {
            assert!(parse(raw.as_bytes(), 4096).is_err(), "{raw}");
        }
        assert!(parse(b"x=1", 2).is_err());
        assert!(parse(&[255], 4096).is_err());
    }
}
