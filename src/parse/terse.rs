//! Core zero-copy machinery for nmcli terse (`-t`) output.
//!
//! Terse mode escapes `:` and `\` inside values with a backslash. This module
//! is the only place those escape rules live: [`TerseFields`] splits a line on
//! *unescaped* colons yielding borrowed slices, and [`unescape`] allocates
//! only when a field actually contains an escape.

use std::borrow::Cow;

use crate::parse::error::ParseError;

/// Iterator over the raw (still-escaped) fields of one terse line.
pub struct TerseFields<'a> {
    rest: Option<&'a str>,
}

impl<'a> TerseFields<'a> {
    pub fn new(line: &'a str) -> Self {
        Self { rest: Some(line) }
    }
}

impl<'a> Iterator for TerseFields<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        let s = self.rest?;
        let bytes = s.as_bytes();
        let mut escaped = false;
        for (i, &b) in bytes.iter().enumerate() {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b':' {
                self.rest = Some(&s[i + 1..]);
                return Some(&s[..i]);
            }
        }
        self.rest = None;
        Some(s)
    }
}

/// Unescape `\:` → `:` and `\\` → `\`. Borrowed (zero-copy) when the field
/// contains no backslash — the overwhelmingly common case.
pub fn unescape(raw: &str) -> Cow<'_, str> {
    if !raw.contains('\\') {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            // Trailing lone backslash is kept literally.
            out.push(chars.next().unwrap_or('\\'));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

/// Split one line into exactly `N` unescaped fields.
pub fn split_fields<'a, const N: usize>(
    line: &'a str,
    line_no: usize,
) -> Result<[Cow<'a, str>; N], ParseError> {
    let mut it = TerseFields::new(line);
    let mut out: [Cow<'a, str>; N] = std::array::from_fn(|_| Cow::Borrowed(""));
    let mut got = 0;
    for slot in out.iter_mut() {
        match it.next() {
            Some(raw) => {
                *slot = unescape(raw);
                got += 1;
            }
            None => {
                return Err(ParseError::ColumnCount { line_no, expected: N, got });
            }
        }
    }
    let extra = it.count();
    if extra > 0 {
        return Err(ParseError::ColumnCount { line_no, expected: N, got: got + extra });
    }
    Ok(out)
}

/// Split a `KEY:VALUE` line (e.g. `nmcli -t device show` output) at the first
/// unescaped colon. Keys never contain escapes; the value may.
pub fn split_kv(line: &str) -> Option<(&str, Cow<'_, str>)> {
    let mut it = TerseFields::new(line);
    let key = it.next()?;
    let value = it.rest.map(unescape).unwrap_or(Cow::Borrowed(""));
    Some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn splits_plain_fields() {
        let fields: Vec<_> = TerseFields::new("eth0:ethernet:connected:Wired").collect();
        assert_eq!(fields, ["eth0", "ethernet", "connected", "Wired"]);
    }

    #[test]
    fn escaped_colon_stays_inside_field() {
        let fields: Vec<_> = TerseFields::new(r"my\:cafe\\net:90").collect();
        assert_eq!(fields, [r"my\:cafe\\net", "90"]);
    }

    #[test]
    fn empty_fields_are_preserved() {
        let fields: Vec<_> = TerseFields::new("a::c:").collect();
        assert_eq!(fields, ["a", "", "c", ""]);
    }

    #[test]
    fn unescape_is_borrowed_for_clean_input() {
        assert!(matches!(unescape("plain ssid"), Cow::Borrowed(_)));
    }

    #[test]
    fn unescape_owns_only_when_escaped() {
        let out = unescape(r"my\:cafe\\net");
        assert!(matches!(out, Cow::Owned(_)));
        assert_eq!(out, r"my:cafe\net");
    }

    #[test]
    fn split_fields_checks_column_count() {
        let err = split_fields::<3>("a:b", 7).unwrap_err();
        assert!(matches!(
            err,
            ParseError::ColumnCount { line_no: 7, expected: 3, got: 2 }
        ));
        let err = split_fields::<1>("a:b", 1).unwrap_err();
        assert!(matches!(err, ParseError::ColumnCount { expected: 1, got: 2, .. }));
    }

    #[test]
    fn split_kv_unescapes_value_only() {
        let (key, value) = split_kv(r"GENERAL.HWADDR:00\:11\:22\:33\:44\:55").unwrap();
        assert_eq!(key, "GENERAL.HWADDR");
        assert_eq!(value, "00:11:22:33:44:55");
        let (key, value) = split_kv("GENERAL.MTU:1500").unwrap();
        assert_eq!(key, "GENERAL.MTU");
        assert!(matches!(value, Cow::Borrowed(_)));
    }
}
