//! PostgreSQL stores each child list as JSONB inside a one-dimensional array.
//! This preserves ragged lists, empty children and null children without DDL.
use super::*;
use datafusion::logical_expr::{
    ExprSchemable, ScalarUDF, Signature, Volatility, expr_fn::SimpleScalarUDF,
};

pub(crate) fn nested(ty: &DataType) -> bool {
    matches!(ty, DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _)
        if matches!(f.data_type(), DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)))
}
fn placeholder(name: &str, args: Vec<Expr>, ty: DataType) -> Expr {
    ScalarUDF::from(SimpleScalarUDF::new_with_signature(
        name,
        Signature::any(args.len(), Volatility::Immutable),
        ty,
        Arc::new(|_| {
            Err(DataFusionError::NotImplemented(
                "PostgreSQL list SQL placeholder".into(),
            ))
        }),
    ))
    .call(args)
}
pub(super) fn encode(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    encode_plan(plan, &std::cell::Cell::new(0))
}
fn encode_plan(plan: LogicalPlan, source_id: &std::cell::Cell<usize>) -> SqlResult<LogicalPlan> {
    Ok(plan
        .transform_down_up_with_subqueries(
            |node| {
                if let Some(search) = crate::ir::rel::search::node(&node) {
                    let mut search = search.clone();
                    search.source = Arc::new(
                        encode_plan(search.source.as_ref().clone(), source_id)
                            .map_err(|e| DataFusionError::Plan(e.to_string()))?,
                    );
                    // Keep target scans and projections flattenable for pgvector.
                    // Normalize nested target payloads only where scalar expressions
                    // consume them, and after top-k for projected search hits.
                    let normalize = |e: Expr| -> datafusion::common::Result<Expr> {
                        Ok(e.transform_up(|e| {
                            if let Expr::Column(c) = &e {
                                if search.target.schema().has_column(c) {
                                    let ty = e.get_type(search.target.schema())?;
                                    if nested(&ty) {
                                        return Ok(Transformed::yes(placeholder(
                                            "__orchiddb_pg_list_normalize",
                                            vec![e],
                                            ty,
                                        )));
                                    }
                                }
                            }
                            Ok(Transformed::no(e))
                        })?
                        .data)
                    };
                    search.score = normalize(search.score.clone())?;
                    search.predicate = search.predicate.clone().map(normalize).transpose()?;
                    return Ok(Transformed::new(
                        search.into_plan(),
                        true,
                        TreeNodeRecursion::Jump,
                    ));
                }
                Ok(Transformed::no(node))
            },
            |node| {
                if let LogicalPlan::TableScan(scan) = &node {
                    if node.schema().fields().iter().any(|f| nested(f.data_type())) {
                        let table = scan.table_name.clone();
                        let columns = node
                            .schema()
                            .columns()
                            .into_iter()
                            .zip(node.schema().fields())
                            .map(|(column, field)| {
                                let value = Expr::Column(column);
                                if nested(field.data_type()) {
                                    placeholder(
                                        "__orchiddb_pg_list_normalize",
                                        vec![value],
                                        field.data_type().clone(),
                                    )
                                    .alias(field.name())
                                } else {
                                    value
                                }
                            })
                            .collect::<Vec<_>>();
                        source_id.set(source_id.get() + 1);
                        let id = source_id.get();
                        let plan = datafusion::logical_expr::LogicalPlanBuilder::from(node)
                            .project(columns)?
                            .alias(format!("__w_sql_cte_pg_lists_{id}"))?
                            .alias(table)?
                            .build()?;
                        return Ok(Transformed::yes(plan));
                    }
                }
                let schemas = node
                    .inputs()
                    .iter()
                    .map(|p| p.schema().clone())
                    .chain(std::iter::once(node.schema().clone()))
                    .collect::<Vec<_>>();
                let preserve = matches!(
                    node,
                    LogicalPlan::Projection(_) | LogicalPlan::Aggregate(_) | LogicalPlan::Window(_)
                );
                let mapped = node.map_expressions(|expr| {
                    let original = expr.qualified_name();
                    expr.transform_up(|expr| {
                        if let Expr::Literal(value, _) = &expr {
                            if nested(&value.data_type()) && !value.is_null() {
                                let ScalarValue::List(array) = value else {
                                    return Ok(Transformed::no(expr));
                                };
                                let items = array.value(0);
                                let args = (0..items.len())
                                    .map(|i| {
                                        let value = ScalarValue::try_from_array(items.as_ref(), i)?;
                                        Ok(if value.is_null() {
                                            lit(ScalarValue::Utf8(None))
                                        } else {
                                            lit(crate::federation::scalar_json(&value)?.to_string())
                                        })
                                    })
                                    .collect::<datafusion::common::Result<Vec<_>>>()?;
                                return Ok(Transformed::yes(placeholder(
                                    "__orchiddb_pg_list_literal",
                                    args,
                                    value.data_type(),
                                )));
                            }
                        }
                        if let Expr::ScalarFunction(f) = &expr {
                            if f.name() == "make_array" {
                                if let Some(ty) = schemas
                                    .iter()
                                    .find_map(|s| expr.get_type(s).ok())
                                    .filter(nested)
                                {
                                    let DataType::List(field) = &ty else {
                                        return Ok(Transformed::no(expr));
                                    };
                                    let args = f
                                        .args
                                        .iter()
                                        .map(|arg| {
                                            crate::ir::functions::typed_argument_cast_for_engine(
                                                arg.clone(),
                                                field.data_type().clone(),
                                                "postgres",
                                            )
                                        })
                                        .collect::<datafusion::common::Result<Vec<_>>>()?;
                                    return Ok(Transformed::yes(placeholder(
                                        "__orchiddb_pg_list_build",
                                        args,
                                        ty,
                                    )));
                                }
                            }
                            if f.name() == "array_element" && f.args.len() == 2 {
                                if let Some(ty) = schemas
                                    .iter()
                                    .find_map(|s| f.args[0].get_type(s).ok())
                                    .filter(nested)
                                {
                                    let DataType::List(field) = ty else {
                                        return Ok(Transformed::no(expr));
                                    };
                                    let result = field.data_type().clone();
                                    let mut args = f.args.clone();
                                    args.push(lit(crate::ir::functions::postgres_type(&result)?));
                                    return Ok(Transformed::yes(placeholder(
                                        "__orchiddb_pg_list_element",
                                        args,
                                        result,
                                    )));
                                }
                            }
                        }
                        Ok(Transformed::no(expr))
                    })?
                    .map_data(|expr| {
                        Ok(if preserve && expr.qualified_name() != original {
                            expr.alias_qualified(original.0, original.1)
                        } else {
                            expr
                        })
                    })
                })?;
                if let LogicalPlan::Unnest(unnest) = &mapped.data {
                    let outputs = unnest
                        .list_type_columns
                        .iter()
                        .filter(|(i, _)| nested(unnest.input.schema().field(*i).data_type()))
                        .map(|(_, c)| c.output_column.clone())
                        .collect::<BTreeSet<_>>();
                    if !outputs.is_empty() {
                        let schema = unnest.schema.clone();
                        let columns = schema
                            .columns()
                            .into_iter()
                            .zip(schema.fields())
                            .map(|(column, field)| {
                                if outputs.contains(&column)
                                    && matches!(field.data_type(), DataType::List(_))
                                {
                                    let value = placeholder(
                                        "__orchiddb_pg_list_decode",
                                        vec![
                                            Expr::Column(column.clone()),
                                            lit(crate::ir::functions::postgres_type(
                                                field.data_type(),
                                            )?),
                                        ],
                                        field.data_type().clone(),
                                    );
                                    Ok(value.alias_qualified(column.relation, column.name))
                                } else {
                                    Ok(Expr::Column(column))
                                }
                            })
                            .collect::<datafusion::common::Result<Vec<_>>>()?;
                        let projection = datafusion::logical_expr::Projection::try_new_with_schema(
                            columns,
                            Arc::new(mapped.data),
                            schema,
                        )?;
                        return Ok(Transformed::yes(LogicalPlan::Projection(projection)));
                    }
                }
                Ok(mapped)
            },
        )?
        .data)
}

pub(super) fn adapt(
    expr: &mut datafusion::sql::sqlparser::ast::Expr,
    name: &str,
    args: &[datafusion::sql::sqlparser::ast::Expr],
) -> SqlResult<bool> {
    use datafusion::sql::sqlparser::ast;
    if name == "__orchiddb_pg_list_normalize" && args.len() == 1 {
        *expr = super::functions::template(
            "(SELECT CASE WHEN a IS NULL THEN CAST(NULL AS JSONB[]) ELSE ARRAY(SELECT nullif(v,CAST('null' AS JSONB)) FROM jsonb_array_elements(to_jsonb(a)) WITH ORDINALITY AS __local1(v,n) ORDER BY n) END FROM (SELECT __arg0 AS a) AS __local0)",
            args,
        )?;
        return Ok(true);
    }
    if name == "__orchiddb_pg_list_literal" || name == "__orchiddb_pg_list_build" {
        let cells = (0..args.len())
            .map(|i| {
                if name.ends_with("build") {
                    format!("to_jsonb(__arg{i})")
                } else {
                    format!("CAST(__arg{i} AS JSONB)")
                }
            })
            .collect::<Vec<_>>()
            .join(",");
        *expr = super::functions::template(&format!("CAST(ARRAY[{cells}] AS JSONB[])"), args)?;
        return Ok(true);
    }
    if (name == "__orchiddb_pg_list_element" && args.len() == 3)
        || (name == "__orchiddb_pg_list_decode" && args.len() == 2)
    {
        let ast::Expr::Value(value) = &args[args.len() - 1] else {
            return Err(SqlError::Unsupported("list element type".into()));
        };
        let ast::Value::SingleQuotedString(ty) = &value.value else {
            return Err(SqlError::Unsupported("list element type".into()));
        };
        let cell = if ty == "JSONB[]" {
            "nullif(v,CAST('null' AS JSONB))".into()
        } else {
            format!(
                "CAST(v #>> '{{}}' AS {})",
                ty.strip_suffix("[]")
                    .ok_or_else(|| SqlError::Unsupported("list element type".into()))?
            )
        };
        let source = if name.ends_with("decode") {
            "SELECT to_jsonb(__arg0) AS j"
        } else {
            "SELECT a[CASE WHEN i<0 THEN cardinality(a)+i+1 ELSE i END] AS j FROM (SELECT __arg0 AS a,__arg1 AS i) AS __local0"
        };
        *expr = super::functions::template(
            &format!(
                "(SELECT CASE WHEN j IS NULL OR j=CAST('null' AS JSONB) THEN CAST(NULL AS {ty}) ELSE CAST(ARRAY(SELECT {cell} FROM jsonb_array_elements(j) WITH ORDINALITY AS __local2(v,n) ORDER BY n) AS {ty}) END FROM ({source}) AS __local1)"
            ),
            &args[..args.len() - 1],
        )?;
        return Ok(true);
    }
    Ok(false)
}
