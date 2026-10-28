//! Typed text-search expressions. PostgreSQL owns parsing, dictionaries, and rank.
//! Native vectors/queries stay inside the source SQL; no text serialization is used.
use super::{
    domain,
    logical::{LogicalFunction, SqlFunctionMapping},
};
use arrow::datatypes::DataType;
use datafusion::{
    common::{DataFusionError, Result},
    logical_expr::{
        ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
    },
};
use std::{
    any::Any,
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

#[derive(Debug, PartialEq, Eq, Hash)]
struct FullText {
    name: &'static str,
    signature: Signature,
}
fn string(ty: &DataType) -> bool {
    matches!(
        ty,
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
    )
}
fn native(ty: &DataType, expected: &str) -> bool {
    *ty == DataType::Null
        || domain::descriptor(ty).is_some_and(|(name, storage)| name == expected && string(storage))
}
impl ScalarUDFImpl for FullText {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, args: &[DataType]) -> Result<DataType> {
        let result = match (self.name, args) {
            ("text.vector", [text, config]) if string(text) && string(config) => {
                Some(domain::data_type("tsvector", DataType::Utf8))
            }
            ("text.query", [text, config]) if string(text) && string(config) => {
                Some(domain::data_type("tsquery", DataType::Utf8))
            }
            ("text.queryfromvector", [vector]) if native(vector, "tsvector") => {
                Some(domain::data_type("tsquery", DataType::Utf8))
            }
            ("text.rank", [vector, query, normalization])
                if native(vector, "tsvector")
                    && native(query, "tsquery")
                    && matches!(
                        normalization,
                        DataType::Int32 | DataType::Int64 | DataType::Null
                    ) =>
            {
                Some(DataType::Float64)
            }
            _ => None,
        };
        result.ok_or_else(|| {
            DataFusionError::Plan(format!("invalid {} arguments: {args:?}", self.name))
        })
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> Result<ColumnarValue> {
        Err(DataFusionError::NotImplemented(format!(
            "{} requires PostgreSQL execution",
            self.name
        )))
    }
}
pub(super) fn function(name: &str) -> Option<Arc<ScalarUDF>> {
    static FUNCTIONS: LazyLock<BTreeMap<String, Arc<ScalarUDF>>> = LazyLock::new(|| {
        [
            ("text.vector", 2, "to_tsvector(CAST(__arg1 AS regconfig), __arg0)"),
            ("text.query", 2, "plainto_tsquery(CAST(__arg1 AS regconfig), __arg0)"),
            ("text.queryfromvector", 1, "(SELECT CAST(string_agg(quote_literal(__local0.term), ' | ') AS tsquery) FROM unnest(tsvector_to_array(__arg0)) AS __local0(term))"),
            ("text.rank", 3, "CAST(ts_rank(__arg0, __arg1, CAST(__arg2 AS INTEGER)) AS DOUBLE PRECISION)"),
        ].into_iter().map(|(name, arity, sql)| {
            // Do not constant-fold database-owned dictionary semantics locally.
            let native = Arc::new(ScalarUDF::new_from_impl(FullText { name, signature: Signature::any(arity, Volatility::Volatile) }));
            let mapping = SqlFunctionMapping { value: sql.into(), ordering: None };
            (name.into(), LogicalFunction::new(name, native, BTreeMap::from([("postgres".into(), mapping)])).into_udf())
        }).collect()
    });
    FUNCTIONS.get(&name.to_ascii_lowercase()).cloned()
}
