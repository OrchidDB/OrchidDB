//! Execute engine calls on typed batches through the existing caller-owned host.
use crate::ir::rel::host::{HostRequest, SharedHost};
use arrow::{
    array::RecordBatch,
    datatypes::{Field, Schema},
};
use datafusion::{
    common::{DataFusionError, Result},
    logical_expr::{ColumnarValue, ScalarFunctionArgs},
};
use std::{cell::RefCell, sync::Arc};
thread_local! {static ACTIVE: RefCell<Option<SharedHost>> = RefCell::new(None);}
pub struct Scope(
    Option<SharedHost>,
    std::marker::PhantomData<std::rc::Rc<()>>,
);
impl Drop for Scope {
    fn drop(&mut self) {
        ACTIVE.with(|h| *h.borrow_mut() = self.0.take());
    }
}
pub fn enter(host: SharedHost) -> Scope {
    Scope(
        ACTIVE.with(|h| h.replace(Some(host))),
        std::marker::PhantomData,
    )
}
pub fn scalar(name: &str, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
    let host = ACTIVE.with(|h| h.borrow().clone()).ok_or_else(|| {
        DataFusionError::Execution("DuckDB function requires the caller's execution scope".into())
    })?;
    let arrays = args
        .args
        .into_iter()
        .map(|v| v.into_array(args.number_rows))
        .collect::<Result<Vec<_>>>()?;
    let fields = arrays
        .iter()
        .enumerate()
        .map(|(i, a)| Field::new(format!("arg{i}"), a.data_type().clone(), true))
        .collect::<Vec<_>>();
    let batch = RecordBatch::try_new_with_options(
        Arc::new(Schema::new(fields)),
        arrays,
        &arrow::array::RecordBatchOptions::new().with_row_count(Some(args.number_rows)),
    )?;
    let name = quote_name(name);
    let arguments = (0..batch.num_columns())
        .map(|i| format!("\"arg{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let output = host
        .query(
            HostRequest::new(format!(
                "SELECT {name}({arguments}) AS value FROM __orchid_arguments"
            ))
            .relation("__orchid_arguments", batch),
        )
        .map_err(DataFusionError::Execution)?;
    Ok(ColumnarValue::Array(output.column(0).clone()))
}
pub fn quote_name(name: &str) -> String {
    name.split('.')
        .map(|p| format!("\"{}\"", p.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".")
}
pub(crate) fn value(
    host: &dyn crate::ir::rel::host::HostRelational,
    name: &str,
    args: &[crate::ir::value::Value],
) -> std::result::Result<crate::ir::value::Value, String> {
    let values = crate::ir::rel::host::function_arguments(args.len(), &[args.to_vec()])?;
    let parameters = values
        .columns()
        .iter()
        .map(|a| {
            datafusion::common::ScalarValue::try_from_array(a.as_ref(), 0)
                .map_err(|e| e.to_string())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let arguments = (1..=args.len())
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(",");
    let batch = host.query(
        HostRequest::new(format!("SELECT {}({arguments}) AS value", quote_name(name)))
            .parameters(parameters),
    )?;
    Ok(crate::ir::catalog::array_value(
        batch.column(0).as_ref(),
        0,
        Some(batch.schema().field(0)),
    ))
}

pub(crate) fn aggregate(
    name: &str,
    args: &[crate::ir::expr::IrExpr],
    rows: &[crate::ir::runtime::Row],
    graph: &crate::ir::catalog::PropertyGraph,
    distinct: bool,
) -> crate::ir::runtime::IrResult<crate::ir::value::Value> {
    use crate::ir::runtime::{RuntimeError, expr::eval};
    let error = RuntimeError::Runtime;
    let host = ACTIVE
        .with(|h| h.borrow().clone())
        .ok_or_else(|| error("DuckDB aggregate requires the caller's execution scope".into()))?;
    let mut values = Vec::new();
    for row in rows {
        let args = args
            .iter()
            .map(|a| eval(a, row, graph))
            .collect::<crate::ir::runtime::IrResult<Vec<_>>>()?;
        for _ in 0..row.bulk {
            values.push(args.clone());
        }
    }
    let batch = crate::ir::rel::host::function_arguments(args.len(), &values).map_err(error)?;
    let mut parameters = Vec::new();
    let arguments = args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            // Constant arguments (quantiles, delimiters, etc.) remain constants for
            // DuckDB's bind callbacks, including on empty input.
            if let crate::ir::expr::IrExpr::Lit(_) = arg {
                let value = eval(arg, &crate::ir::runtime::Row::new(), graph)?;
                let scalar =
                    crate::ir::rel::host::function_arguments(1, &[vec![value]]).map_err(error)?;
                parameters.push(
                    datafusion::common::ScalarValue::try_from_array(scalar.column(0).as_ref(), 0)
                        .map_err(|e| error(e.to_string()))?,
                );
                Ok(format!("${}", parameters.len()))
            } else {
                Ok(format!("\"arg{i}\""))
            }
        })
        .collect::<crate::ir::runtime::IrResult<Vec<_>>>()?
        .join(",");
    let sql = format!(
        "SELECT {}({}{arguments}) AS value FROM __orchid_arguments",
        quote_name(name),
        if distinct { "DISTINCT " } else { "" }
    );
    let result = host
        .query(
            HostRequest::new(sql)
                .parameters(parameters)
                .relation("__orchid_arguments", batch),
        )
        .map_err(error)?;
    Ok(crate::ir::catalog::array_value(
        result.column(0).as_ref(),
        0,
        Some(result.schema().field(0)),
    ))
}
