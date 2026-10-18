//! Conservative cleanup of SQL projection scaffolding.
//!
//! Computations may cross a pure rename boundary exactly once. Otherwise only
//! columns and literal values cross projection boundaries. Even an
//! immutable function can fail, and duplicating or dropping it would change
//! evaluation behavior. Cardinality operators and correlated subqueries keep
//! their original scopes.
use super::*;
use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::logical_expr::{LogicalPlanBuilder, Projection};

fn simple_value(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Alias(alias) => simple_value(&alias.expr),
        Expr::Column(_) | Expr::Literal(..) => Some(expr.clone()),
        _ => None,
    }
}

/// A pure output rename can absorb computations without duplicating, dropping,
/// or conditionally evaluating them. Keep set-returning, window, and aggregate
/// expressions in place; scalar subqueries retain their complete correlation.
fn rename_values(outer: &Projection, inner: &Projection) -> datafusion::common::Result<Option<Vec<Expr>>> {
    let mut uses = vec![0_usize; inner.expr.len()];
    for expr in &outer.expr {
        match simple_value(expr) {
            Some(Expr::Column(column)) => uses[inner.schema.index_of_column(&column)?] += 1,
            Some(Expr::Literal(..)) => {},
            _ => return Ok(None),
        }
    }
    let mut values = Vec::with_capacity(inner.expr.len());
    for (index, expr) in inner.expr.iter().enumerate() {
        if let Some(value) = simple_value(expr) { values.push(value); continue; }
        if uses[index] != 1 { return Ok(None); }
        let mut scoped = false;
        expr.apply(|expr| {
            if matches!(expr, Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)
                | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_)) {
                scoped = true;
                return Ok(TreeNodeRecursion::Stop);
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        if scoped { return Ok(None); }
        let value = match expr { Expr::Alias(alias) => alias.expr.as_ref().clone(), other => other.clone() };
        values.push(value);
    }
    Ok(Some(values))
}

// Compose scalar projects when each computed child is consumed once and is
// proven total. This permits (x + 1) followed by a comparison/CASE without
// duplicating work, but does not inline shared or effectful computations.
fn total_values(outer: &Projection, inner: &Projection) -> datafusion::common::Result<Option<Vec<Expr>>> {
    let mut uses=vec![0usize;inner.expr.len()];
    let mut scoped=false;
    for e in &outer.expr {
        e.apply(|e| {
            match e {
                Expr::Column(c)=>uses[inner.schema.index_of_column(c)?]+=1,
                Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)
                    | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_)=>scoped=true,
                _=>{},
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
    }
    if scoped {return Ok(None);}
    let mut values=Vec::new();
    for (i,e) in inner.expr.iter().enumerate() {
        if let Some(e)=simple_value(e) {values.push(e);continue;}
        let e=if let Expr::Alias(a)=e {a.expr.as_ref()} else {e};
        if uses[i]!=1 || e.is_volatile() || !crate::ir::rel::list_comparison::arithmetic_is_total(&inner.input,e) {return Ok(None);}
        let mut unsafe_scope=false;
        e.apply(|e| {
            unsafe_scope |= matches!(e,Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_)
                | Expr::SetComparison(_) | Expr::WindowFunction(_) | Expr::AggregateFunction(_) | Expr::Unnest(_));
            Ok(TreeNodeRecursion::Continue)
        })?;
        if unsafe_scope {return Ok(None);}
        values.push(e.clone());
    }
    Ok(Some(values))
}

fn has_subquery(expr: &Expr) -> datafusion::common::Result<bool> {
    let mut found = false;
    expr.apply(|expr| {
        if matches!(expr, Expr::ScalarSubquery(_) | Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_)) {
            found = true;
            return Ok(TreeNodeRecursion::Stop);
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(found)
}

/// Only literal projections over the actual one-row relation establish a
/// discardable Cartesian input. Do not infer this from cardinality estimates,
/// LIMIT, aggregate output, user table names, or arbitrary scalar expressions.
fn singleton_literals(plan: &LogicalPlan) -> Option<Vec<Expr>> {
    match plan {
        LogicalPlan::EmptyRelation(e) if e.produce_one_row && e.schema.fields().is_empty() => Some(vec![]),
        LogicalPlan::SubqueryAlias(alias) => singleton_literals(&alias.input),
        LogicalPlan::Projection(projection) => {
            let input = singleton_literals(&projection.input)?;
            projection.expr.iter().map(|expr| match simple_value(expr)? {
                value @ Expr::Literal(..) => Some(value),
                Expr::Column(column) => input.get(projection.input.schema().index_of_column(&column).ok()?).cloned(),
                _ => None,
            }).collect()
        }
        _ => None,
    }
}

fn remove_singleton_join(join: &datafusion::logical_expr::Join) -> datafusion::common::Result<Option<LogicalPlan>> {
    use datafusion::logical_expr::{JoinConstraint, JoinType};
    if join.join_type != JoinType::Inner || join.join_constraint != JoinConstraint::On
        || !join.on.is_empty() || join.filter.is_some() || join.null_aware {
        return Ok(None);
    }
    let (input, expressions) = if let Some(mut literals) = singleton_literals(&join.left) {
        literals.extend(join.right.schema().columns().into_iter().map(Expr::Column));
        (join.right.clone(), literals)
    } else if let Some(literals) = singleton_literals(&join.right) {
        let mut expressions: Vec<_> = join.left.schema().columns().into_iter().map(Expr::Column).collect();
        expressions.extend(literals);
        (join.left.clone(), expressions)
    } else {
        return Ok(None);
    };
    // Keep the complete join schema, including source qualifiers, column order,
    // typed nulls, and constants that subsequent projections may still read.
    let expressions = expressions.into_iter().zip(join.schema.iter())
        .map(|(expr, (qualifier, field))| expr.alias_qualified(qualifier.cloned(), field.name()))
        .collect();
    Ok(Some(LogicalPlan::Projection(Projection::try_new_with_schema(
        expressions, input, join.schema.clone(),
    )?)))
}

pub(super) fn simplify(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        if let LogicalPlan::Join(join) = &node
            && let Some(replacement) = remove_singleton_join(join)? {
            return Ok(Transformed::yes(replacement));
        }
        // Snapshot identity AND actual contents establish that this compiler
        // seed is exactly one row. A user table with a similar name is not a
        // constant relation. Never remove filters or a zero-row fetch.
        if let LogicalPlan::TableScan(scan) = &node
            && scan.table_name.table().starts_with("__graph_rel_one_row_")
            && !scan.projected_schema.fields().is_empty()
            && scan.filters.is_empty()
            && scan.fetch != Some(0)
        {
            let provider = datafusion::datasource::source_as_provider(&scan.source)?;
            if let Some(snapshot) = provider.as_any()
                .downcast_ref::<crate::ir::rel::temporal_semantics::Snapshot>()
                && snapshot.batches().iter().map(RecordBatch::num_rows).sum::<usize>() == 1
            {
                let batch = snapshot.batches().iter().find(|b| b.num_rows() == 1).unwrap();
                let indices = scan.projection.clone()
                    .unwrap_or_else(|| (0..batch.num_columns()).collect());
                let exprs = indices.iter().zip(scan.projected_schema.iter())
                    .map(|(index, (qualifier, field))| {
                        Ok(lit(ScalarValue::try_from_array(batch.column(*index), 0)?)
                            .alias_qualified(qualifier.cloned(), field.name()))
                    }).collect::<datafusion::common::Result<Vec<_>>>()?;
                let input = Arc::new(LogicalPlanBuilder::empty(true).build()?);
                return Ok(Transformed::yes(LogicalPlan::Projection(
                    Projection::try_new_with_schema(exprs, input, scan.projected_schema.clone())?)));
            }
        }
        let LogicalPlan::Projection(mut outer) = node else {
            return Ok(Transformed::no(node));
        };
        // Empty-output plans carry an execution/result contract; do not turn
        // their internal seed into an invalid empty SELECT list.
        if outer.expr.is_empty() {
            return Ok(Transformed::no(LogicalPlan::Projection(outer)));
        }
        // A literal-only one-row seed contributes neither values nor effects
        // when every result is independent of its columns. This includes
        // closed scalar subqueries; correlated ones must keep their input.
        if singleton_literals(&outer.input).is_some() {
            let mut independent = true;
            for expr in &outer.expr {
                independent &= expr.column_refs().is_empty();
                expr.apply(|expr| {
                    match expr {
                        Expr::OuterReferenceColumn(..) | Expr::SetComparison(_) => independent = false,
                        Expr::ScalarSubquery(query) => independent &= query.outer_ref_columns.is_empty(),
                        Expr::Exists(query) => independent &= query.subquery.outer_ref_columns.is_empty(),
                        Expr::InSubquery(query) => independent &= query.subquery.outer_ref_columns.is_empty(),
                        _ => {},
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
            }
            if independent {
                outer.input = Arc::new(LogicalPlanBuilder::empty(true).build()?);
                return Ok(Transformed::yes(LogicalPlan::Projection(outer)));
            }
        }
        if outer.expr.iter().map(has_subquery).collect::<datafusion::common::Result<Vec<_>>>()?
            .into_iter().any(|found| found)
        {
            return Ok(Transformed::no(LogicalPlan::Projection(outer)));
        }
        let mut changed = false;
        while let LogicalPlan::Projection(inner) = outer.input.as_ref() {
            // A projection directly above a window is an SQL emission boundary.
            // In particular, SPARQL adds a guard literal here so DataFusion's
            // unparser retains the window outputs. Composing away that guard
            // can leave ORDER BY or aggregates referring to a missing ordinal.
            if matches!(inner.input.as_ref(), LogicalPlan::Window(_)) {
                break;
            }
            let values = match inner.expr.iter().map(simple_value).collect::<Option<Vec<_>>>() {
                Some(values) => Some(values),
                None => match rename_values(&outer, inner)? { Some(values)=>Some(values), None=>total_values(&outer,inner)? },
            };
            let Some(values) = values else {
                break;
            };
            let exprs = outer.expr.iter().zip(outer.schema.iter()).map(|(expr, (qualifier, field))| {
                let expr = expr.clone().transform_up(|expr| {
                    if let Expr::Column(column) = &expr {
                        let index = inner.schema.index_of_column(column)?;
                        return Ok(Transformed::yes(values[index].clone()));
                    }
                    Ok(Transformed::no(expr))
                })?.data;
                // Rewriting an expression must not change the name referenced
                // by parent operators, nor retain an obsolete inner alias.
                let expr = match expr { Expr::Alias(a) => *a.expr, other => other };
                Ok(expr.alias_qualified(qualifier.cloned(), field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
            outer = Projection::try_new_with_schema(exprs, inner.input.clone(), outer.schema.clone())?;
            changed = true;
            // A scalar subquery may cross this pure rename once with its
            // input and outer-reference metadata intact. Never then compose
            // another lower projection without rebinding its correlations.
            if outer.expr.iter().map(has_subquery).collect::<datafusion::common::Result<Vec<_>>>()?
                .into_iter().any(|found| found) { break; }
        }
        Ok(Transformed::new(LogicalPlan::Projection(outer), changed, TreeNodeRecursion::Continue))
    })?.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::prelude::col;

    #[test]
    fn folded_quantifiers_emit_one_select() {
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![lit(0_i64).alias("__w_one_row")]).unwrap()
            .project(vec![col("__w_one_row"), lit(true).alias("__cypher_quantifier_0")]).unwrap()
            .project(vec![col("__cypher_quantifier_0").alias("result")]).unwrap()
            .build().unwrap();
        assert_eq!(super::super::unparse_plan(plan, SqlDialect::DuckDb).unwrap(),
            "SELECT true AS \"result\"");
    }

    #[test]
    fn keeps_zero_row_source() {
        let plan = LogicalPlanBuilder::empty(false)
            .project(vec![lit(true).alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("result")]).unwrap()
            .build().unwrap();
        let plan = simplify(plan).unwrap();
        let LogicalPlan::Projection(p) = plan else { panic!("projection expected") };
        assert!(matches!(p.input.as_ref(), LogicalPlan::EmptyRelation(e) if !e.produce_one_row));
    }

    #[test]
    fn pure_output_rename_retains_runtime_computation_once() {
        let input = occurrences();
        let computed = LogicalPlanBuilder::from(input.clone())
            .project(vec![(col("value") + lit(1_i64)).alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("result")]).unwrap().build().unwrap();
        let schema = computed.schema().clone();
        let simplified = simplify(computed).unwrap();
        assert_eq!(simplified.schema(), &schema);
        let LogicalPlan::Projection(p) = simplified else { panic!("projection expected") };
        assert_eq!(p.input.as_ref(), &input);
        assert_eq!(p.expr, vec![(col("value") + lit(1_i64)).alias("result")]);
    }

    #[test]
    fn scalar_subquery_rename_keeps_correlated_input_projection() {
        let input = LogicalPlanBuilder::from(occurrences())
            .project(vec![col("value").alias("renamed")]).unwrap().build().unwrap();
        let reference = Expr::OuterReferenceColumn(input.schema().field(0).clone(), Column::new_unqualified("renamed"));
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![reference.clone().alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![reference], spans: Default::default(),
        });
        let plan = LogicalPlanBuilder::from(input.clone())
            .project(vec![subquery.clone().alias("temporary")]).unwrap()
            .project(vec![col("temporary").alias("renamed")]).unwrap().build().unwrap();
        let LogicalPlan::Projection(p) = simplify(plan).unwrap() else { panic!("projection expected") };
        assert_eq!(p.input.as_ref(), &input);
        assert_eq!(p.expr, vec![subquery.alias("renamed")]);
    }

    #[test]
    fn closed_scalar_subquery_does_not_need_a_literal_seed() {
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![lit(7_i64).alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![], spans: Default::default(),
        });
        let seed = LogicalPlanBuilder::empty(true).project(vec![lit(0_i64).alias("seed")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(seed).project(vec![subquery.clone().alias("result")]).unwrap().build().unwrap();
        let LogicalPlan::Projection(p) = simplify(plan).unwrap() else { panic!("projection expected") };
        assert!(matches!(p.input.as_ref(), LogicalPlan::EmptyRelation(e) if e.produce_one_row && e.schema.fields().is_empty()));
        assert_eq!(p.expr, vec![subquery.alias("result")]);
    }

    #[test]
    fn set_comparison_keeps_its_correlated_literal_seed_and_scope() {
        use datafusion::logical_expr::{expr::{SetComparison, SetQuantifier}, Subquery};
        let seed = LogicalPlanBuilder::empty(true)
            .project(vec![lit(7_i64).alias("seed")]).unwrap().build().unwrap();
        let reference = Expr::OuterReferenceColumn(seed.schema().field(0).clone(), Column::new_unqualified("seed"));
        let subquery = Subquery {
            subquery: Arc::new(LogicalPlanBuilder::empty(true)
                .project(vec![reference.clone().alias("value")]).unwrap().build().unwrap()),
            outer_ref_columns: vec![reference], spans: Default::default(),
        };
        let comparison = Expr::SetComparison(SetComparison::new(Box::new(lit(7_i64)), subquery,
            datafusion::logical_expr::Operator::Eq, SetQuantifier::Any));
        assert!(has_subquery(&comparison).unwrap());
        let input = LogicalPlanBuilder::from(seed.clone()).project(vec![comparison.alias("temporary")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).project(vec![col("temporary").alias("result")]).unwrap().build().unwrap();
        let simplified = simplify(plan).unwrap();
        let LogicalPlan::Projection(outer) = simplified else { panic!("projection expected") };
        let LogicalPlan::Projection(inner) = outer.input.as_ref() else { panic!("set-comparison scope must remain") };
        assert_eq!(inner.input.as_ref(), &seed);
        assert!(matches!(inner.expr[0], Expr::Alias(ref alias) if matches!(alias.expr.as_ref(), Expr::SetComparison(_))));
    }

    #[test]
    fn does_not_duplicate_or_discard_computations() {
        // A division can throw, even though it is immutable. Neither a double
        // reference nor an unused result justifies removing this boundary.
        for exprs in [vec![col("computed").alias("a"), col("computed").alias("b")],
                      vec![lit(true).alias("result")]] {
            let plan = LogicalPlanBuilder::empty(true)
                .project(vec![(lit(1_i64) / lit(0_i64)).alias("computed")]).unwrap()
                .project(exprs).unwrap().build().unwrap();
            let original = plan.clone();
            assert_eq!(simplify(plan).unwrap(), original);
        }
    }

    #[test]
    fn does_not_duplicate_volatile_functions() {
        let random = datafusion::logical_expr::create_udf("volatile_value", vec![], DataType::Int64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::Int64(Some(1))))));
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![random.call(vec![]).alias("value")]).unwrap()
            .project(vec![col("value").alias("a"), col("value").alias("b")]).unwrap()
            .build().unwrap();
        assert_eq!(simplify(plan.clone()).unwrap(), plan);
    }
    fn singleton(expr: Expr, one_row: bool) -> LogicalPlan {
        LogicalPlanBuilder::empty(one_row).project(vec![expr.alias("seed")]).unwrap().build().unwrap()
    }
    fn occurrences() -> LogicalPlan {
        let row = |value| LogicalPlanBuilder::empty(true)
            .project(vec![lit(value).alias("value")]).unwrap().build().unwrap();
        LogicalPlanBuilder::from(row(ScalarValue::Int64(Some(1))))
            .union(row(ScalarValue::Int64(Some(1)))).unwrap()
            .union(row(ScalarValue::Int64(None))).unwrap().build().unwrap()
    }
    fn cross(left: LogicalPlan, right: LogicalPlan) -> LogicalPlan {
        LogicalPlanBuilder::from(left).cross_join(right).unwrap().build().unwrap()
    }

    #[test]
    fn singleton_cross_join_preserves_schema_constants_and_input() {
        for seed_on_left in [true,false] {
            let seed = singleton(lit(ScalarValue::Int64(None)), true);
            let rows = occurrences();
            let plan = if seed_on_left { cross(seed,rows.clone()) } else { cross(rows.clone(),seed) };
            let schema = plan.schema().clone();
            let simplified = simplify(plan).unwrap();
            assert_eq!(simplified.schema(), &schema);
            let LogicalPlan::Projection(p) = simplified else { panic!("expected projection") };
            assert_eq!(p.input.as_ref(), &rows);
            let index = if seed_on_left {0} else {1};
            assert_eq!(simple_value(&p.expr[index]), Some(lit(ScalarValue::Int64(None))));
        }
    }

    #[test]
    fn zero_row_computed_and_conditional_join_inputs_are_not_singleton_seeds() {
        let volatile = datafusion::logical_expr::create_udf("volatile_seed", vec![], DataType::Int64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::Int64(Some(1))))));
        let subquery = Expr::ScalarSubquery(datafusion::logical_expr::Subquery {
            subquery: Arc::new(singleton(lit(2_i64),true)), outer_ref_columns: vec![], spans: Default::default(),
        });
        for seed in [singleton(lit(0_i64),false), singleton(lit(1_i64)/lit(0_i64),true),
                     singleton(volatile.call(vec![]),true), singleton(subquery,true)] {
            let plan = cross(seed,occurrences());
            assert_eq!(simplify(plan.clone()).unwrap(), plan);
        }
        let plan = cross(singleton(lit(0_i64),true),occurrences());
        let LogicalPlan::Join(mut join) = plan else { panic!("join expected") };
        join.filter = Some(lit(true));
        let plan = LogicalPlan::Join(join);
        assert_eq!(simplify(plan.clone()).unwrap(),plan);
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn singleton_join_sql_keeps_duplicates_nulls_and_empty_results() {
        fn rows(plan: LogicalPlan) -> Vec<Vec<ScalarValue>> {
            let sql = super::super::unparse_plan(plan, SqlDialect::DuckDb).unwrap();
            assert!(!sql.contains("CROSS JOIN"), "{sql}");
            let connection = duckdb::Connection::open_in_memory().unwrap();
            let mut statement = connection.prepare(&sql).unwrap();
            let batches: Vec<RecordBatch> = statement.query_arrow([]).unwrap().collect();
            batches.iter().flat_map(|batch| (0..batch.num_rows()).map(|row|
                batch.columns().iter().map(|array| ScalarValue::try_from_array(array,row).unwrap()).collect::<Vec<_>>()
            )).collect()
        }
        for left in [true,false] {
            let seed = singleton(lit(7_i64),true);
            let other = occurrences();
            let plan = if left { cross(seed,other) } else { cross(other,seed) };
            let actual = rows(plan);
            // This test runs raw SQL, without execute_prepared's output-schema
            // normalization. DuckDB infers INTEGER for these small literals.
            let seven = ScalarValue::Int32(Some(7));
            let one = ScalarValue::Int32(Some(1));
            let null = ScalarValue::Int32(None);
            let expected = if left { vec![vec![seven.clone(),one.clone()],vec![seven.clone(),one],vec![seven,null]] }
                else { vec![vec![one.clone(),seven.clone()],vec![one,seven.clone()],vec![null,seven]] };
            assert_eq!(actual,expected);
        }
        let empty = LogicalPlanBuilder::from(occurrences()).filter(lit(false)).unwrap().build().unwrap();
        assert!(rows(cross(singleton(lit(7_i64),true),empty)).is_empty());
    }

}
