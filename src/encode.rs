use polars::prelude::*;
use pyo3_polars::export::polars_arrow::array::ListArray;
use pyo3_polars::export::polars_arrow::offset::OffsetsBuffer;

use crate::sexp::push_atom;
use crate::temporal::{self, Unit};

const NIL: &str = "()";

enum Writer {
    Null,
    Text(StringChunked),
    Boolean(BooleanChunked),
    Signed(Int64Chunked),
    Unsigned(UInt64Chunked),
    Signed128(Int128Chunked),
    Unsigned128(UInt128Chunked),
    Float32(Float32Chunked),
    Float64(Float64Chunked),
    Date(Int32Chunked),
    Datetime {
        counts: Int64Chunked,
        unit: Unit,
        zone: Option<chrono_tz::Tz>,
    },
    Duration {
        counts: Int64Chunked,
        unit: Unit,
    },
    Time(Int64Chunked),
    List {
        validity: BooleanChunked,
        offsets: OffsetsBuffer<i64>,
        elements: Box<Writer>,
    },
    Struct {
        validity: BooleanChunked,
        fields: Vec<StructField>,
    },
}

struct StructField {
    opening: String,
    writer: Writer,
}

impl Writer {
    fn new(s: &Series) -> PolarsResult<Writer> {
        let s = s.rechunk();
        Ok(match s.dtype() {
            DataType::Null => Writer::Null,
            DataType::String => Writer::Text(s.str()?.clone()),
            DataType::Boolean => Writer::Boolean(s.bool()?.clone()),
            DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                Writer::Signed(s.cast(&DataType::Int64)?.i64()?.clone())
            }
            DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                Writer::Unsigned(s.cast(&DataType::UInt64)?.u64()?.clone())
            }
            DataType::Int128 => Writer::Signed128(s.i128()?.clone()),
            DataType::UInt128 => Writer::Unsigned128(s.u128()?.clone()),
            DataType::Float32 => Writer::Float32(s.f32()?.clone()),
            DataType::Float64 => Writer::Float64(s.f64()?.clone()),
            DataType::Date => Writer::Date(s.date()?.physical().clone()),
            DataType::Datetime(unit, tz) => Writer::Datetime {
                counts: s.datetime()?.physical().clone(),
                unit: Unit::from_polars(*unit),
                zone: tz.as_ref().map(|tz| tz.to_chrono()).transpose()?,
            },
            DataType::Duration(unit) => Writer::Duration {
                counts: s.duration()?.physical().clone(),
                unit: Unit::from_polars(*unit),
            },
            DataType::Time => Writer::Time(s.time()?.physical().clone()),
            DataType::Decimal(_, _) | DataType::Categorical(_, _) | DataType::Enum(_, _) => {
                Writer::Text(s.cast(&DataType::String)?.str()?.clone())
            }
            DataType::Array(inner, _) => Writer::new(&s.cast(&DataType::List(inner.clone()))?)?,
            DataType::List(_) => {
                let ca = s.list()?;
                let arr: &ListArray<i64> = ca.downcast_as_array();
                Writer::List {
                    validity: s.is_not_null(),
                    offsets: arr.offsets().clone(),
                    elements: Box::new(Writer::new(&ca.get_inner())?),
                }
            }
            DataType::Struct(_) => {
                let fields = s
                    .struct_()?
                    .fields_as_series()
                    .iter()
                    .map(|field| {
                        let mut opening = String::from("(");
                        push_atom(&mut opening, field.name());
                        opening.push(' ');
                        Ok(StructField {
                            opening,
                            writer: Writer::new(field)?,
                        })
                    })
                    .collect::<PolarsResult<_>>()?;
                Writer::Struct {
                    validity: s.is_not_null(),
                    fields,
                }
            }
            dtype => polars_bail!(InvalidOperation: "sexp.encode does not support {}", dtype),
        })
    }

    fn is_null(&self, row: usize) -> bool {
        match self {
            Writer::Null => true,
            Writer::Text(ca) => ca.get(row).is_none(),
            Writer::Boolean(ca) => ca.get(row).is_none(),
            Writer::Signed(ca) => ca.get(row).is_none(),
            Writer::Unsigned(ca) => ca.get(row).is_none(),
            Writer::Signed128(ca) => ca.get(row).is_none(),
            Writer::Unsigned128(ca) => ca.get(row).is_none(),
            Writer::Float32(ca) => ca.get(row).is_none(),
            Writer::Float64(ca) => ca.get(row).is_none(),
            Writer::Date(ca) => ca.get(row).is_none(),
            Writer::Time(ca)
            | Writer::Datetime { counts: ca, .. }
            | Writer::Duration { counts: ca, .. } => ca.get(row).is_none(),
            Writer::List { validity, .. } | Writer::Struct { validity, .. } => {
                validity.get(row) != Some(true)
            }
        }
    }

    fn write_or_nil(&self, row: usize, out: &mut String) {
        if self.is_null(row) {
            out.push_str(NIL);
        } else {
            self.write(row, out);
        }
    }

    fn write_or_nil_on_overflow(out: &mut String, push: impl FnOnce(&mut String) -> Option<()>) {
        let written = out.len();
        if push(out).is_none() {
            out.truncate(written);
            out.push_str(NIL);
        }
    }

    fn write(&self, row: usize, out: &mut String) {
        let mut integers = itoa::Buffer::new();
        let mut floats = ryu::Buffer::new();
        match self {
            Writer::Null => out.push_str(NIL),
            Writer::Text(ca) => push_atom(out, ca.get(row).unwrap_or_default()),
            Writer::Boolean(ca) => out.push_str(if ca.get(row) == Some(true) {
                "true"
            } else {
                "false"
            }),
            Writer::Signed(ca) => out.push_str(integers.format(ca.get(row).unwrap_or_default())),
            Writer::Unsigned(ca) => out.push_str(integers.format(ca.get(row).unwrap_or_default())),
            Writer::Signed128(ca) => out.push_str(integers.format(ca.get(row).unwrap_or_default())),
            Writer::Unsigned128(ca) => {
                out.push_str(integers.format(ca.get(row).unwrap_or_default()))
            }
            Writer::Float32(ca) => out.push_str(floats.format(ca.get(row).unwrap_or_default())),
            Writer::Float64(ca) => out.push_str(floats.format(ca.get(row).unwrap_or_default())),
            Writer::Date(ca) => Self::write_or_nil_on_overflow(out, |out| {
                temporal::push_date(out, ca.get(row).unwrap_or_default())
            }),
            Writer::Datetime { counts, unit, zone } => Self::write_or_nil_on_overflow(out, |out| {
                let count = counts.get(row).unwrap_or_default();
                temporal::push_timestamp(out, count, *unit, zone.as_ref())
            }),
            Writer::Duration { counts, unit } => {
                temporal::push_duration(out, counts.get(row).unwrap_or_default(), *unit)
            }
            Writer::Time(ca) => temporal::push_time(out, ca.get(row).unwrap_or_default()),
            Writer::List {
                offsets, elements, ..
            } => {
                let (start, end) = offsets.start_end(row);
                out.push('(');
                for element in start..end {
                    if element > start {
                        out.push(' ');
                    }
                    elements.write_or_nil(element, out);
                }
                out.push(')');
            }
            Writer::Struct { fields, .. } => {
                out.push('(');
                let mut present = fields.iter().filter(|f| !f.writer.is_null(row)).peekable();
                if present.peek().is_none() {
                    if let Some(first) = fields.first() {
                        out.push_str(&first.opening);
                        out.push_str(NIL);
                        out.push(')');
                    }
                }
                for (i, field) in present.enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    out.push_str(&field.opening);
                    field.writer.write(row, out);
                    out.push(')');
                }
                out.push(')');
            }
        }
    }
}

pub fn encode(s: &Series) -> PolarsResult<StringChunked> {
    let writer = Writer::new(s)?;
    let mut builder = StringChunkedBuilder::new(s.name().clone(), s.len());
    let mut row_text = String::new();
    for row in 0..s.len() {
        if writer.is_null(row) {
            builder.append_null();
        } else {
            row_text.clear();
            writer.write(row, &mut row_text);
            builder.append_value(&row_text);
        }
    }
    Ok(builder.finish())
}
