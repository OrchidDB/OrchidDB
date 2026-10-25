//! Internal compiler composition, deliberately without a DataFusion executable
//! session, runtime environment, physical planner, catalog service, or file reader.
//! The historical import namespace is retained to keep shared lowering code small.
pub use datafusion_common as common;
pub use datafusion_common::error;
pub use datafusion_expr as logical_expr;
pub use datafusion_functions as functions;
pub use datafusion_functions_aggregate as functions_aggregate;
pub use datafusion_functions_nested as functions_nested;
pub use datafusion_functions_window as functions_window;
pub use datafusion_optimizer as optimizer;
pub use datafusion_sql as sql;

pub mod scalar {
    pub use datafusion_common::ScalarValue;
}
pub mod prelude {
    pub use datafusion_common::Column;
    pub use datafusion_expr::{Expr, expr_fn::*, lit, lit_timestamp_nano, logical_plan::{JoinType, Partitioning}};
    pub use datafusion_functions::expr_fn::*;
    pub use datafusion_functions_nested::expr_fn::*;
    pub use std::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Neg, Not, Rem, Shl, Shr, Sub};
}

pub mod datasource;
pub mod catalog {
    pub use datafusion_expr::TableSource as TableProvider;
    pub mod view {
        pub use crate::datasource::ViewTable;
    }
}
