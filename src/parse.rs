use crate::sexp::{is_delimiter, Sexp};

const MAX_NESTING_DEPTH: usize = 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub pos: usize,
    pub msg: &'static str,
}

pub fn parse_exactly_one(input: &str) -> Result<Sexp, ParseError> {
    let mut p = Parser {
        src: input.as_bytes(),
        pos: 0,
        depth: 0,
    };
    let sexp = p.parse_sexp()?;
    p.skip_whitespace_and_comments();
    if p.pos != p.src.len() {
        return Err(p.err("trailing input after expression"));
    }
    Ok(sexp)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
    depth: usize,
}

impl Parser<'_> {
    fn err(&self, msg: &'static str) -> ParseError {
        ParseError { pos: self.pos, msg }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn consume(&mut self, expected: u8) {
        debug_assert_eq!(self.peek(), Some(expected));
        self.pos += 1;
    }

    fn skip_whitespace_and_comments(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else if c == b';' {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn parse_sexp(&mut self) -> Result<Sexp, ParseError> {
        self.skip_whitespace_and_comments();
        match self.peek() {
            None => Err(self.err("unexpected end of input")),
            Some(b'(') => self.parse_list(),
            Some(b')') => Err(self.err("unexpected ')'")),
            Some(b'"') => self.parse_string(),
            Some(_) => Ok(self.parse_symbol()),
        }
    }

    fn parse_list(&mut self) -> Result<Sexp, ParseError> {
        if self.depth == MAX_NESTING_DEPTH {
            return Err(self.err("lists nested too deeply"));
        }
        self.consume(b'(');
        self.depth += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.peek() {
                None => return Err(self.err("unclosed '('")),
                Some(b')') => {
                    self.consume(b')');
                    self.depth -= 1;
                    return Ok(Sexp::List(items));
                }
                Some(_) => items.push(self.parse_sexp()?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<Sexp, ParseError> {
        self.consume(b'"');
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.consume(b'"');
                    let text = String::from_utf8(out)
                        .expect("UTF-8 input split only at ASCII quotes stays UTF-8");
                    return Ok(Sexp::Atom(text));
                }
                Some(b'\\') => {
                    self.consume(b'\\');
                    let escaped = self.peek().ok_or_else(|| self.err("unterminated string"))?;
                    out.push(match escaped {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        literal => literal,
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
        while self.peek().is_some_and(|c| !is_delimiter(c)) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.src[start..self.pos])
            .expect("UTF-8 input split only at ASCII delimiters stays UTF-8");
        Sexp::Atom(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(s: &str) -> Sexp {
        Sexp::Atom(s.to_string())
    }

    fn list(items: &[Sexp]) -> Sexp {
        Sexp::List(items.to_vec())
    }

    #[test]
    fn semicolon_starts_a_line_comment() {
        assert_eq!(
            parse_exactly_one("(a ; ignored )\n b)").unwrap(),
            list(&[atom("a"), atom("b")])
        );
    }

    #[test]
    fn unknown_escapes_keep_the_escaped_char_even_when_multibyte() {
        assert_eq!(parse_exactly_one(r#""\q\"\λ\n""#).unwrap(), atom("q\"λ\n"));
    }

    #[test]
    fn nesting_beyond_the_depth_limit_is_an_error_not_a_stack_overflow() {
        let nested = |depth: usize| "(".repeat(depth) + &")".repeat(depth);
        assert!(parse_exactly_one(&nested(MAX_NESTING_DEPTH)).is_ok());
        assert!(parse_exactly_one(&nested(MAX_NESTING_DEPTH + 1)).is_err());
        assert!(parse_exactly_one(&nested(1_000_000)).is_err());
    }

    #[test]
    fn trailing_input_after_one_expression_is_an_error() {
        assert!(parse_exactly_one("(a) b").is_err());
        assert!(parse_exactly_one("a)").is_err());
    }
}
