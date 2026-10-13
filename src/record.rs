use crate::sexp::{push_list, Sexp};

#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    Node(&'a Sexp<'a>),
    FieldValues(&'a [Sexp<'a>]),
}

enum Shape<'a> {
    One(&'a Sexp<'a>),
    Several(&'a [Sexp<'a>]),
}

pub fn field_in<'a>(entries: &'a [Sexp<'a>], key: &str) -> Option<Value<'a>> {
    entries.iter().find_map(|item| match item {
        Sexp::List(entry) => match entry.split_first() {
            Some((Sexp::Atom(k), values)) if k == key => Some(Value::FieldValues(values)),
            _ => None,
        },
        Sexp::Atom(_) => None,
    })
}

impl<'a> Value<'a> {
    fn shape(self) -> Shape<'a> {
        match self {
            Value::Node(sexp) | Value::FieldValues([sexp]) => Shape::One(sexp),
            Value::FieldValues(values) => Shape::Several(values),
        }
    }

    pub fn as_atom(self) -> Option<&'a str> {
        match self.shape() {
            Shape::One(Sexp::Atom(s)) => Some(s),
            _ => None,
        }
    }

    pub fn as_items(self) -> Option<&'a [Sexp<'a>]> {
        match self.shape() {
            Shape::One(Sexp::List(items)) => Some(items),
            Shape::Several(items) => Some(items),
            Shape::One(Sexp::Atom(_)) => None,
        }
    }

    pub fn is_nil(self) -> bool {
        self.as_items().is_some_and(<[Sexp]>::is_empty)
    }

    pub fn as_field_values(self) -> Option<&'a [Sexp<'a>]> {
        match self {
            Value::FieldValues(values) => Some(values),
            Value::Node(_) => None,
        }
    }

    pub fn as_list_elements(self) -> Option<&'a [Sexp<'a>]> {
        match self {
            Value::FieldValues(values @ [Sexp::Atom(_)]) => Some(values),
            _ => self.as_items(),
        }
    }

    pub fn record_field(self, key: &str) -> Option<Value<'a>> {
        field_in(self.as_items()?, key)
    }

    pub fn to_text_with_bare_atom(self) -> String {
        match self.shape() {
            Shape::One(Sexp::Atom(s)) => s.to_string(),
            Shape::One(sexp) => sexp.to_string(),
            Shape::Several(values) => {
                let mut text = String::new();
                push_list(&mut text, values);
                text
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_exactly_one;

    #[test]
    fn record_field_with_several_values_reads_as_their_list() {
        let record = parse_exactly_one("((tags a b) (inner ((x 1))))").unwrap();
        let record = Value::Node(&record);
        assert_eq!(
            record
                .record_field("tags")
                .unwrap()
                .to_text_with_bare_atom(),
            "(a b)"
        );
        assert_eq!(
            record
                .record_field("inner")
                .and_then(|v| v.record_field("x"))
                .and_then(|v| v.as_atom()),
            Some("1")
        );
    }

    #[test]
    fn text_with_bare_atom_does_not_quote_a_top_level_atom() {
        let spaced = Sexp::Atom("c d".into());
        assert_eq!(Value::Node(&spaced).to_text_with_bare_atom(), "c d");
        assert_eq!(spaced.to_string(), "\"c d\"");
    }
}
