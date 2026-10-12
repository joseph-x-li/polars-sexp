use crate::decode::{DtypeSpec, FieldSpec};
use crate::record::Value;
use crate::sexp::Sexp;
use crate::temporal::{self, DatetimeKind, Unit};

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Unknown,
    Boolean,
    Integer { wide: bool },
    Float,
    Date,
    Datetime { has_offset: bool },
    Text,
    List(Box<Shape>),
    Record(Vec<(String, Shape)>),
}

fn of_atom(atom: &str) -> Shape {
    if matches!(atom, "true" | "false") {
        return Shape::Boolean;
    }
    if atom.parse::<i64>().is_ok() {
        return Shape::Integer { wide: false };
    }
    if atom.parse::<i128>().is_ok() {
        return Shape::Integer { wide: true };
    }
    if matches!(atom, "NaN" | "inf" | "-inf") {
        return Shape::Float;
    }
    let starts_like_a_number = atom
        .trim_start_matches(['-', '+'])
        .starts_with(|c: char| c.is_ascii_digit() || c == '.');
    if starts_like_a_number && atom.parse::<f64>().is_ok() {
        return Shape::Float;
    }
    if temporal::parse_days_since_epoch(atom, None).is_some() {
        return Shape::Date;
    }
    match temporal::classify_datetime(atom) {
        Some(DatetimeKind::Instant) => Shape::Datetime { has_offset: true },
        Some(DatetimeKind::WallClock) => Shape::Datetime { has_offset: false },
        None => Shape::Text,
    }
}

fn record_keys(items: &[Sexp]) -> Option<Vec<&str>> {
    let mut keys: Vec<&str> = Vec::with_capacity(items.len());
    for item in items {
        let Sexp::List(entry) = item else { return None };
        let Some(Sexp::Atom(key)) = entry.first() else {
            return None;
        };
        if of_atom(key) != Shape::Text || keys.contains(&key.as_str()) {
            return None;
        }
        keys.push(key);
    }
    (!keys.is_empty()).then_some(keys)
}

pub fn of_value(value: Value) -> Shape {
    if let Some(atom) = value.as_atom() {
        return of_atom(atom);
    }
    let items = value.as_items().unwrap_or_default();
    match record_keys(items) {
        Some(keys) => Shape::Record(
            keys.into_iter()
                .map(|k| {
                    let field = value.record_field(k).expect("key came from this record");
                    (k.to_string(), of_value(field))
                })
                .collect(),
        ),
        None => Shape::List(Box::new(
            items
                .iter()
                .map(|c| of_value(Value::Node(c)))
                .fold(Shape::Unknown, merge),
        )),
    }
}

fn merge_records(mut left: Vec<(String, Shape)>, right: Vec<(String, Shape)>) -> Shape {
    for (key, shape) in right {
        match left.iter_mut().find(|(k, _)| *k == key) {
            Some((_, existing)) => *existing = merge(existing.clone(), shape),
            None => left.push((key, shape)),
        }
    }
    Shape::Record(left)
}

pub fn merge(left: Shape, right: Shape) -> Shape {
    use Shape::*;
    match (left, right) {
        (Unknown, s) | (s, Unknown) => s,
        (l, r) if l == r => l,
        (Integer { wide: l }, Integer { wide: r }) => Integer { wide: l || r },
        (Integer { .. }, Float) | (Float, Integer { .. }) => Float,
        (Date, Datetime { has_offset }) | (Datetime { has_offset }, Date) => {
            Datetime { has_offset }
        }
        (Datetime { .. }, Datetime { .. }) => Datetime { has_offset: true },
        (List(l), List(r)) => List(Box::new(merge(*l, *r))),
        (Record(l), Record(r)) => merge_records(l, r),
        (Record(fields), List(empty)) | (List(empty), Record(fields)) if *empty == Unknown => {
            Record(fields)
        }
        (List(empty), scalar) | (scalar, List(empty)) if *empty == Unknown => scalar,
        (List(l), scalar @ (Boolean | Integer { .. } | Float | Date | Datetime { .. } | Text))
        | (scalar @ (Boolean | Integer { .. } | Float | Date | Datetime { .. } | Text), List(l)) => {
            List(Box::new(merge(*l, scalar)))
        }
        _ => Text,
    }
}

impl Shape {
    pub fn spec(self) -> DtypeSpec {
        match self {
            Shape::Unknown | Shape::Text => DtypeSpec::String,
            Shape::Boolean => DtypeSpec::Boolean,
            Shape::Integer { wide: false } => DtypeSpec::Int64,
            Shape::Integer { wide: true } => DtypeSpec::Int128,
            Shape::Float => DtypeSpec::Float64,
            Shape::Date => DtypeSpec::Date { format: None },
            Shape::Datetime { has_offset } => DtypeSpec::Datetime {
                unit: Unit::Us,
                time_zone: has_offset.then(|| "UTC".to_string()),
                format: None,
            },
            Shape::List(inner) => DtypeSpec::List {
                inner: Box::new(inner.spec()),
            },
            Shape::Record(fields) => DtypeSpec::Struct {
                fields: fields
                    .into_iter()
                    .map(|(name, shape)| FieldSpec {
                        name,
                        dtype: shape.spec(),
                    })
                    .collect(),
            },
        }
    }
}
