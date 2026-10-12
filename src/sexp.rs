//! A minimal S-expression parser.
//!
//! Grammar:
//!   sexp  := atom | list
//!   list  := '(' sexp* ')'
//!   atom  := string | symbol
//!   string:= '"' (escaped char | any char except '"')* '"'
//!   symbol:= one or more chars that are not whitespace, '(', ')', '"' or ';'
//! Line comments start with ';'.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    /// Lists become JSON arrays, atoms become JSON strings.
    pub fn to_json(&self) -> Value {
        match self {
            Sexp::Atom(s) => Value::String(s.clone()),
            Sexp::List(items) => Value::Array(items.iter().map(Sexp::to_json).collect()),
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
}
