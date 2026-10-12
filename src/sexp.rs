//! A minimal S-expression parser.
//!
//! Grammar:
//!   sexp  := atom | list
//!   list  := '(' sexp* ')'
//!   atom  := string | symbol
//!   string:= '"' (escaped char | any char except '"')* '"'
//!   symbol:= one or more chars that are not whitespace, '(', ')', '"' or ';'
//! Line comments start with ';'.

use serde_json::Value as Json;

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    /// Lists become JSON arrays, atoms become JSON strings.
    pub fn to_json(&self) -> Json {
        match self {
            Sexp::Atom(s) => Json::String(s.clone()),
            Sexp::List(items) => Json::Array(items.iter().map(Sexp::to_json).collect()),
        }
    }
}

impl std::fmt::Display for Sexp {
    /// Canonical text form: single spaces, atoms quoted only when needed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Sexp::Atom(s) => write_atom(f, s),
            Sexp::List(items) => write_list(f, items),
        }
    }
}

fn write_atom(f: &mut std::fmt::Formatter<'_>, s: &str) -> std::fmt::Result {
    let needs_quotes = s.is_empty()
        || s.bytes()
            .any(|c| c.is_ascii_whitespace() || matches!(c, b'(' | b')' | b'"' | b';' | b'\\'));
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

fn write_list(f: &mut std::fmt::Formatter<'_>, items: &[Sexp]) -> std::fmt::Result {
    f.write_str("(")?;
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            f.write_str(" ")?;
        }
        write!(f, "{item}")?;
    }
    f.write_str(")")
}

/// A view into a parsed S-expression: either one node, or the tail of a
/// record field like `(tags a b c)`, which holds several values.
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    One(&'a Sexp),
    Many(&'a [Sexp]),
}

impl<'a> Value<'a> {
    /// The atom's text, if this is a single atom.
    pub fn as_atom(&self) -> Option<&'a str> {
        match self {
            Value::One(Sexp::Atom(s)) => Some(s),
            _ => None,
        }
    }

    /// The elements, if this is a list (or a multi-value field tail).
    pub fn as_items(&self) -> Option<&'a [Sexp]> {
        match self {
            Value::One(Sexp::List(items)) => Some(items),
            Value::Many(items) => Some(items),
            Value::One(Sexp::Atom(_)) => None,
        }
    }

    /// Look up `key` treating this value as a record `((key value...) ...)`.
    /// A field with one value yields that value; with several, all of them.
    pub fn field(&self, key: &str) -> Option<Value<'a>> {
        self.as_items()?.iter().find_map(|item| match item {
            Sexp::List(kv) => match kv.split_first() {
                Some((Sexp::Atom(k), rest)) if k == key => Some(match rest {
                    [one] => Value::One(one),
                    many => Value::Many(many),
                }),
                _ => None,
            },
            Sexp::Atom(_) => None,
        })
    }

    /// Atoms as their raw text, everything else as canonical S-expression text.
    pub fn to_text(&self) -> String {
        match self {
            Value::One(Sexp::Atom(s)) => s.clone(),
            Value::One(sexp) => sexp.to_string(),
            Value::Many(items) => Sexp::List(items.to_vec()).to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub pos: usize,
    pub msg: &'static str,
}

/// Parse exactly one S-expression; trailing non-whitespace is an error.
pub fn parse(input: &str) -> Result<Sexp, ParseError> {
    let mut p = Parser {
        src: input.as_bytes(),
        pos: 0,
    };
    let sexp = p.parse_sexp()?;
    p.skip_ws();
    if p.pos != p.src.len() {
        return Err(p.err("trailing input after expression"));
    }
    Ok(sexp)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn err(&self, msg: &'static str) -> ParseError {
        ParseError { pos: self.pos, msg }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else if c == b';' {
                while let Some(c) = self.peek() {
                    if c == b'\n' {
                        break;
                    }
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn parse_sexp(&mut self) -> Result<Sexp, ParseError> {
        self.skip_ws();
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'(') => self.parse_list(),
            Some(b')') => Err(self.err("unexpected ')'")),
            Some(b'"') => self.parse_string(),
            Some(_) => Ok(self.parse_symbol()),
        }
    }

    fn parse_list(&mut self) -> Result<Sexp, ParseError> {
        self.pos += 1; // '('
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(self.err("unclosed '('")),
                Some(b')') => {
                    self.pos += 1;
                    return Ok(Sexp::List(items));
                }
                Some(_) => items.push(self.parse_sexp()?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<Sexp, ParseError> {
        self.pos += 1; // opening '"'
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    // Input was valid UTF-8 and we only split at ASCII bytes.
                    return Ok(Sexp::Atom(String::from_utf8(out).unwrap()));
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let c = self.peek().ok_or_else(|| self.err("unterminated string"))?;
                    out.push(match c {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        other => other,
                    });
                    self.pos += 1;
                }
                Some(c) => {
                    out.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    fn parse_symbol(&mut self) -> Sexp {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() || matches!(c, b'(' | b')' | b'"' | b';') {
                break;
            }
            self.pos += 1;
        }
        // Delimiters are ASCII, so this slice is on a char boundary.
        Sexp::Atom(
            std::str::from_utf8(&self.src[start..self.pos])
                .unwrap()
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(s: &str) -> Sexp {
        Sexp::Atom(s.to_string())
    }

    #[test]
    fn parses_atoms_and_lists() {
        assert_eq!(parse("foo").unwrap(), atom("foo"));
        assert_eq!(
            parse("(a (b \"c d\") ; comment\n e)").unwrap(),
            Sexp::List(vec![
                atom("a"),
                Sexp::List(vec![atom("b"), atom("c d")]),
                atom("e"),
            ])
        );
        assert_eq!(parse("()").unwrap(), Sexp::List(vec![]));
        assert_eq!(
            parse("(λ x)").unwrap(),
            Sexp::List(vec![atom("λ"), atom("x")])
        );
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse("").is_err());
        assert!(parse("(a b").is_err());
        assert!(parse("a)").is_err());
        assert!(parse("(a) b").is_err());
        assert!(parse("\"abc").is_err());
    }

    #[test]
    fn to_json() {
        let s = parse("(a (b 1))").unwrap();
        assert_eq!(s.to_json().to_string(), r#"["a",["b","1"]]"#);
    }

    #[test]
    fn display_roundtrips() {
        let src = r#"(a "b c" ("q\"x") "" (λ))"#;
        let s = parse(src).unwrap();
        assert_eq!(s.to_string(), src);
        assert_eq!(parse(&s.to_string()).unwrap(), s);
    }

    #[test]
    fn record_fields() {
        let s = parse("((name foo) (price 3) (tags a b) (inner ((x 1))))").unwrap();
        let v = Value::One(&s);
        assert_eq!(v.field("name").unwrap().as_atom(), Some("foo"));
        assert_eq!(v.field("tags").unwrap().to_text(), "(a b)");
        assert_eq!(
            v.field("inner").unwrap().field("x").unwrap().as_atom(),
            Some("1")
        );
        assert!(v.field("missing").is_none());
        assert!(v.field("name").unwrap().field("x").is_none());
    }
}
