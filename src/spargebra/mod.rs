//! Embedded SPARQL parser, derived from spargebra 0.4.7.
//! See ORCHIDDB.md and the adjacent upstream license files.

pub mod algebra;
mod parser;
mod query;
pub mod term;
mod update;

pub use parser::{SparqlParser, SparqlSyntaxError};
pub use query::*;
pub use update::*;
