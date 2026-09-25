//! Typed openCypher temporal values. The legacy Gremlin DateTime value
//! retains its existing representation and semantics.
mod arithmetic;
mod components;
mod constructors;
mod parsing;
mod truncation;
mod value;

type Result<T> = std::result::Result<T, String>;

pub use arithmetic::{arithmetic, between};
pub use constructors::construct;
pub use parsing::parse;
pub use truncation::truncate;
pub use value::TemporalValue;
pub(crate) use value::contains_temporal;
