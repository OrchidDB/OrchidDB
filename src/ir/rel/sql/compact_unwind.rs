//! A terminal literal UNWIND needs no scope barriers or materialized seed table.
//! Keep the general lowering intact: filters, aggregates, scalar consumers and
//! multiple expansions still use the normal scope-preserving SQL emitter.
use super::*;
use datafusion::common::DFSchema;
use datafusion::logical_expr::ExprSchemable;

#[derive(Clone)]
struct Cell {
    value: Expr,
    expanded: bool,
}

fn resolve(expr: &Expr, input: &LogicalPlan, cells: &[Cell]) -> Option<Cell> {
    match expr {
        Expr::Alias(a) => resolve(&a.expr, input, cells),
        Expr::Column(c) => cells.get(input.schema().index_of_column(c).ok()?).cloned(),
        Expr::Literal(..) => Some(Cell {
            value: expr.clone(),
            expanded: false,
        }),
        Expr::Cast(c) => {
            let value = resolve(&c.expr, input, cells)?;
            if value.expanded {
                return None;
            }
            Some(Cell {
                value: value.value.cast_to(&c.data_type, &DFSchema::empty()).ok()?,
                expanded: false,
            })
        }
        Expr::ScalarFunction(f) if f.func.name() == "make_array" => {
            let mut f = f.clone();
            f.args = f
                .args
                .iter()
                .map(|arg| {
                    let cell = resolve(arg, input, cells)?;
                    (!cell.expanded).then_some(cell.value)
                })
                .collect::<Option<_>>()?;
            Some(Cell {
                value: Expr::ScalarFunction(f),
                expanded: false,
            })
        }
        _ => None,
    }
}

fn cells(plan: &LogicalPlan, expansions: &mut usize) -> Option<Vec<Cell>> {
    match plan {
        LogicalPlan::Projection(p) => {
            let input = cells(&p.input, expansions)?;
            p.expr
                .iter()
                .map(|expr| resolve(expr, &p.input, &input))
                .collect()
        }
        LogicalPlan::SubqueryAlias(a) => cells(&a.input, expansions),
        LogicalPlan::EmptyRelation(e) if e.produce_one_row && e.schema.fields().is_empty() => {
            Some(vec![])
        }
        LogicalPlan::TableScan(scan)
            if scan.table_name.table().starts_with("__graph_rel_one_row_")
                && scan.filters.is_empty()
                && scan.fetch != Some(0) =>
        {
            // Never infer cardinality from an internal-looking table name.
            let provider = datafusion::datasource::source_as_provider(&scan.source).ok()?;
            let snapshot = provider
                .as_any()
                .downcast_ref::<crate::ir::rel::temporal_semantics::Snapshot>()?;
            if snapshot
                .batches()
                .iter()
                .map(RecordBatch::num_rows)
                .sum::<usize>()
                != 1
            {
                return None;
            }
            let batch = snapshot.batches().iter().find(|b| b.num_rows() == 1)?;
            let indices = scan
                .projection
                .clone()
                .unwrap_or_else(|| (0..batch.num_columns()).collect());
            indices
                .into_iter()
                .map(|i| {
                    Some(Cell {
                        value: lit(ScalarValue::try_from_array(batch.column(i), 0).ok()?),
                        expanded: false,
                    })
                })
                .collect()
        }
        LogicalPlan::Unnest(u)
            if *expansions == 0
                && u.list_type_columns.len() == 1
                && u.struct_type_columns.is_empty()
                && !u.options.preserve_nulls =>
        {
            let input = cells(&u.input, expansions)?;
            if *expansions != 0 {
                return None;
            }
            let (index, list) = &u.list_type_columns[0];
            if list.depth != 1 {
                return None;
            }
            *expansions += 1;
            u.dependency_indices
                .iter()
                .map(|i| {
                    let mut cell = input.get(*i)?.clone();
                    cell.expanded = *i == *index;
                    Some(cell)
                })
                .collect()
        }
        _ => None,
    }
}

// Nominal graph/temporal/native-value structs need representation selection;
// leave those to the general pipeline rather than interpreting physical fields.
fn plain_type(ty: &DataType) -> bool {
    match ty {
        DataType::Null
        | DataType::Boolean
        | DataType::Int64
        | DataType::Float64
        | DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Utf8View => true,
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            plain_type(f.data_type())
        }
        _ => false,
    }
}

pub(super) fn emit(plan: &LogicalPlan, dialect: SqlDialect) -> SqlResult<Option<String>> {
    if dialect != SqlDialect::DuckDb {
        return Ok(None);
    }
    let mut expansions = 0;
    let Some(cells) = cells(plan, &mut expansions) else {
        return Ok(None);
    };
    if expansions != 1 || !cells.iter().any(|c| c.expanded) {
        return Ok(None);
    }
    if cells.iter().any(|cell| {
        !cell
            .value
            .get_type(&DFSchema::empty())
            .is_ok_and(|ty| plain_type(&ty))
    }) {
        return Ok(None);
    }
    let mut values: Vec<Expr> = vec![];
    let mut names = vec![];
    let mut projection = vec![];
    for (cell, field) in cells.iter().zip(plan.schema().fields()) {
        let index = values
            .iter()
            .position(|e| e == &cell.value)
            .unwrap_or_else(|| {
                let index = values.len();
                values.push(cell.value.clone());
                names.push(format!("v{index}"));
                index
            });
        let column = dialect.quote_ident(&names[index]);
        let expr = if cell.expanded {
            format!("unnest({column})")
        } else {
            column
        };
        projection.push(format!("{expr} AS {}", dialect.quote_ident(field.name())));
    }
    let values = values
        .iter()
        .map(|value| {
            let encoded = super::unparse::encode_expression_literals(value.clone(), dialect)?.data;
            let context = datafusion::logical_expr::LogicalPlanBuilder::empty(true)
                .project(vec![encoded.clone()])?
                .build()?;
            let sql = super::logical_functions::with_plan(&context, dialect, || {
                let unparser_dialect = dialect.unparser_dialect();
                let mut expression =
                    Unparser::new(unparser_dialect.as_ref()).expr_to_sql(&encoded)?;
                super::functions::prepare_ast(&mut expression, dialect)?;
                Ok(expression.to_string())
            })?;
            // A VALUES column must have exactly the logical list/scalar type,
            // including empty lists and null elements.
            let ty = value.get_type(&DFSchema::empty())?;
            if matches!(
                value,
                Expr::Literal(
                    ScalarValue::List(_)
                        | ScalarValue::LargeList(_)
                        | ScalarValue::FixedSizeList(_),
                    _
                )
            ) {
                Ok(sql)
            } else {
                Ok(format!("CAST({sql} AS {})", dialect.sql_type(&ty)?))
            }
        })
        .collect::<SqlResult<Vec<_>>>()?;
    Ok(Some(format!(
        "SELECT {} FROM (VALUES ({})) AS v({})",
        projection.join(", "),
        values.join(", "),
        names
            .iter()
            .map(|n| dialect.quote_ident(n))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}
