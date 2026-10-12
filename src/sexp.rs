use std::fmt;

use serde_json::Value as Json;

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    pub fn to_json(&self) -> Json {
        match self {
            Sexp::Atom(s) => Json::String(s.clone()),
            Sexp::List(items) => Json::Array(items.iter().map(Sexp::to_json).collect()),
        }
    }
}

impl fmt::Display for Sexp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sexp::Atom(s) => write_atom_quoted_if_needed(f, s),
            Sexp::List(items) => write_list(f, items),
        }
    }
}

pub(crate) fn is_delimiter(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b'(' | b')' | b'"' | b';')
}

fn write_atom_quoted_if_needed(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    let needs_quotes = s.is_empty() || s.bytes().any(is_delimiter);
    if !needs_quotes {
        return f.write_str(s);
    }
    f.write_str("\"")?;
    for c in s.chars() {
        match c {
            '"' => f.write_str("\\\"")?,
            '\\' => f.write_str("\\\\")?,
            '\n' => f.write_str("\\n")?,
            '\t' => f.write_str("\\t")?,
            '\r' => f.write_str("\\r")?,
            c => write!(f, "{c}")?,
        }
    }
    f.write_str("\"")
}

fn write_list(f: &mut fmt::Formatter<'_>, items: &[Sexp]) -> fmt::Result {
    f.write_str("(")?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(" ")?;
        }
        write!(f, "{item}")?;
    }
    f.write_str(")")
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
