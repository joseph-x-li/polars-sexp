use std::borrow::Cow;
use std::fmt;

use serde::ser::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp<'a> {
    Atom(Cow<'a, str>),
    List(Vec<Sexp<'a>>),
}

impl Serialize for Sexp<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Sexp::Atom(s) => serializer.serialize_str(s),
            Sexp::List(items) => serializer.collect_seq(items),
        }
    }
}

impl Sexp<'_> {
    pub fn push_to(&self, out: &mut String) {
        match self {
            Sexp::Atom(s) => push_atom(out, s),
            Sexp::List(items) => push_list(out, items),
        }
    }
}

pub fn push_list(out: &mut String, items: &[Sexp]) {
    out.push('(');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        item.push_to(out);
    }
    out.push(')');
}

impl fmt::Display for Sexp<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::new();
        self.push_to(&mut text);
        f.write_str(&text)
    }
}

pub(crate) const fn is_delimiter(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b'(' | b')' | b'"' | b';')
}

const IS_DELIMITER: [bool; 256] = {
    let mut table = [false; 256];
    let mut byte = 0;
    while byte < 256 {
        table[byte] = is_delimiter(byte as u8);
        byte += 1;
    }
    table
};

pub fn push_atom(out: &mut String, s: &str) {
    let needs_quotes = s.is_empty() || s.bytes().any(|b| IS_DELIMITER[usize::from(b)]);
    if !needs_quotes {
        out.push_str(s);
        return;
    }
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use crate::parse::parse_exactly_one;

    #[test]
    fn display_quotes_atoms_only_when_they_would_not_reparse_bare() {
        let src = r#"(plain "has space" "" back\slash ("q\"x"))"#;
        assert_eq!(parse_exactly_one(src).unwrap().to_string(), src);
    }
}
