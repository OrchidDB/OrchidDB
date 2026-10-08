//! CTE wrappers for boundaries DataFusion's SQL unparser cannot preserve.
//!
//! DataFusion can unparse each recursive term, but deliberately rejects the
//! enclosing [`LogicalPlan::RecursiveQuery`]. We replace those nodes with CTE
//! scans in the main plan, unparse every component independently, and add the
//! small wrapper the upstream unparser does not provide. The same mechanism
//! preserves explicitly marked aggregate and join scope barriers.

use std::collections::BTreeSet;
use std::sync::Arc;

use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::datasource::cte_worktable::CteWorkTable;
use datafusion::datasource::provider_as_source;
use datafusion::logical_expr::{LogicalPlan, TableScan};
use datafusion::sql::unparser::Unparser;

use super::{SqlDialect, SqlError, SqlResult, restore_aggregate_ordering};

#[derive(Debug)]
struct RecursiveCte {
    name: String,
    static_term: LogicalPlan,
    recursive_term: LogicalPlan,
    is_distinct: bool,
}

#[derive(Debug)]
struct PlainCte {
    name: String,
    term: LogicalPlan,
}

pub(super) fn unparse_plan(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<String> {
    let repair_scopes = dialect.requires_scope_repair() || super::unparse::has_rdf_source(&plan);
    let (main, plain_ctes, recursive_ctes) = extract_ctes(plan, repair_scopes)?;
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref()).with_extension_unparsers(vec![Arc::new(super::lowering::RelationUnparser::new(dialect))]);
    let main_sql = unparse_one(&main, &unparser, dialect, repair_scopes)?;
    if plain_ctes.is_empty() && recursive_ctes.is_empty() {
        return dialect.finalize_sql(main_sql);
    }

    let has_recursive = !recursive_ctes.is_empty();
    // Every definition must follow the CTEs it reads: a recursive term may
    // read a hoisted barrier (e.g. a precomputed probe set), and a barrier
    // may read a recursive work table.
    let names: BTreeSet<String> = recursive_ctes
        .iter()
        .map(|cte| cte.name.clone())
        .chain(plain_ctes.iter().map(|cte| cte.name.clone()))
        .collect();
    let mut pending = Vec::with_capacity(plain_ctes.len() + recursive_ctes.len());
    for cte in recursive_ctes {
        let mut deps = referenced_ctes(&cte.static_term, &names);
        deps.extend(referenced_ctes(&cte.recursive_term, &names));
        deps.remove(&cte.name);
        let static_sql = unparse_one(&cte.static_term, &unparser, dialect, repair_scopes)?;
        let recursive_sql = unparse_one(&cte.recursive_term, &unparser, dialect, repair_scopes)?;
        let union = if cte.is_distinct {
            "UNION"
        } else {
            "UNION ALL"
        };
        pending.push((
            cte.name.clone(),
            deps,
            format!(
                "{} AS (\n  {static_sql}\n  {union}\n  {recursive_sql}\n)",
                dialect.quote_ident(&cte.name)
            ),
        ));
    }
    for cte in plain_ctes {
        let mut deps = referenced_ctes(&cte.term, &names);
        deps.remove(&cte.name);
        let term_sql = unparse_one(&cte.term, &unparser, dialect, repair_scopes)?;
        // Weighted frontiers and per-occurrence apply inputs are deliberate
        // shared relations. Keep DuckDB/Postgres from repeatedly inlining
        // their joins/windows into every correlated consumer.
        let materialized = if matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres)
            && (cte.name.starts_with("__w_sql_cte_weighted_repeat_")
            || cte.name.starts_with("__w_sql_cte_apply_row_")) {
            " MATERIALIZED"
        } else { "" };
        pending.push((
            cte.name.clone(),
            deps,
            format!("{} AS{materialized} (\n  {term_sql}\n)", dialect.quote_ident(&cte.name)),
        ));
    }
    let mut definitions = Vec::with_capacity(pending.len());
    let mut defined = BTreeSet::new();
    while !pending.is_empty() {
        let Some(ready) = pending
            .iter()
            .position(|(_, deps, _)| deps.iter().all(|dep| defined.contains(dep)))
        else {
            return Err(SqlError::Unsupported(
                "cyclic dependency between hoisted CTEs".into(),
            ));
        };
        let (name, _, definition) = pending.remove(ready);
        defined.insert(name);
        definitions.push(definition);
    }
    let keyword = if has_recursive {
        "WITH RECURSIVE"
    } else {
        "WITH"
    };
    dialect.finalize_sql(format!("{keyword} {}\n{main_sql}", definitions.join(",\n")))
}

fn referenced_ctes(plan: &LogicalPlan, names: &BTreeSet<String>) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let _ = plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            let table = scan.table_name.table();
            if names.contains(table) {
                found.insert(table.to_string());
            }
        }
        Ok(TreeNodeRecursion::Continue)
    });
    found
}

fn unparse_one(
    plan: &LogicalPlan,
    unparser: &Unparser<'_>,
    dialect: SqlDialect,
    repair_scopes: bool,
) -> SqlResult<String> {
    let mut statement = unparser
        .plan_to_sql(plan)
        .map_err(|err| SqlError::Unsupported(format!("unparser ({}): {err}", dialect.name())))?;
    if repair_scopes {
        if let datafusion::sql::sqlparser::ast::Statement::Query(query) = &mut statement {
            if let datafusion::sql::sqlparser::ast::SetExpr::Select(select) = query.body.as_mut() {
                if select.projection.len() == plan.schema().fields().len() {
                    for (item, field) in select.projection.iter_mut().zip(plan.schema().fields()) {
                        if let datafusion::sql::sqlparser::ast::SelectItem::UnnamedExpr(expr) = item
                        {
                            *item = datafusion::sql::sqlparser::ast::SelectItem::ExprWithAlias {
                                expr: expr.clone(),
                                alias: match expr {
                                    datafusion::sql::sqlparser::ast::Expr::Identifier(id) => id.clone(),
                                    datafusion::sql::sqlparser::ast::Expr::CompoundIdentifier(ids) => ids.last().unwrap().clone(),
                                    _ => dialect.identifier(field.name()),
                                },
                            };
                        }
                    }
                }
            }
        }
    }
    prepare_ranges(plan, &mut statement, dialect)?;
    super::functions::prepare_scoped_ast(&mut statement, dialect, repair_scopes)?;
    restore_aggregate_ordering(plan, unparser, dialect, statement.to_string())
}

fn extract_ctes(plan: LogicalPlan, repair_scopes: bool) -> SqlResult<(LogicalPlan, Vec<PlainCte>, Vec<RecursiveCte>)> {
    let mut plain_ctes: Vec<PlainCte> = Vec::new();
    let mut recursive_ctes: Vec<RecursiveCte> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut plain_versions = std::collections::BTreeMap::<String, Vec<usize>>::new();
    let transformed = plan.transform_up(|node| match node {
        LogicalPlan::Filter(mut filter) if matches!(filter.input.as_ref(),LogicalPlan::Projection(p) if p.schema.fields().iter().any(|f|crate::ir::rel::native_values::is_value(f.data_type()))) => {
            filter.input=Arc::new(hoist_operator(filter.input.as_ref().clone(),"native_projection",&mut plain_ctes,&mut seen)?.data);
            Ok(Transformed::yes(LogicalPlan::Filter(filter)))
        }
        LogicalPlan::SubqueryAlias(mut alias)
            if repair_scopes && matches!(alias.input.as_ref(), LogicalPlan::Filter(_))
                && !alias.alias.table().starts_with("__w_sql_cte_") =>
        {
            // DF can inline a filter's predicate without changing its table
            // qualifier to the surrounding alias. Keep that predicate scoped.
            alias.input = Arc::new(
                hoist_operator(
                    alias.input.as_ref().clone(),
                    "filtered_source",
                    &mut plain_ctes,
                    &mut seen,
                )?
                .data,
            );
            Ok(Transformed::yes(LogicalPlan::SubqueryAlias(alias)))
        }
        LogicalPlan::Unnest(mut unnest) => {
            // Optimizers can push scalar consumers into the named alias that
            // originally fenced UNNEST. Fence the actual row-expanding node
            // after optimization, before the unparser can merge its SELECT
            // into a filter or substitute UNNEST inside a scalar expression.
            if !matches!(unnest.input.as_ref(), LogicalPlan::Projection(_)) {
                let columns = unnest.input.schema().columns().into_iter()
                    .map(datafusion::logical_expr::Expr::Column).collect();
                unnest.input = Arc::new(LogicalPlan::Projection(
                    datafusion::logical_expr::logical_plan::Projection::try_new(columns, unnest.input)?));
            }
            let term = LogicalPlan::Unnest(unnest);
            let columns = term.schema().fields().iter()
                .map(|field| datafusion::logical_expr::Expr::Column(
                    datafusion::common::Column::new_unqualified(field.name()))).collect();
            let term = LogicalPlan::Projection(
                datafusion::logical_expr::logical_plan::Projection::try_new(columns, Arc::new(term))?);
            hoist_operator(term, "unnest", &mut plain_ctes, &mut seen)
        }
        LogicalPlan::Window(window) if window.window_expr.iter().any(|expr|
            expr.schema_name().to_string().starts_with("__apply_corr_key_row")) => {
            // Copies of an apply boundary can acquire distinct QUALIFY or
            // projection consumers. Their unordered ROW_NUMBER identities
            // must still come from one shared evaluation of the frontier.
            let term = LogicalPlan::Window(window);
            let columns = term.schema().fields().iter()
                .map(|field| datafusion::logical_expr::Expr::Column(
                    datafusion::common::Column::new_unqualified(field.name()))).collect();
            let term = LogicalPlan::Projection(
                datafusion::logical_expr::logical_plan::Projection::try_new(columns, Arc::new(term))?);
            hoist_operator(term, "apply_row_window", &mut plain_ctes, &mut seen)
        }
        LogicalPlan::RecursiveQuery(recursive) => {
            let schema = Arc::new(recursive.static_term.schema().as_arrow().clone());
            let work_table = Arc::new(CteWorkTable::new(&recursive.name, schema));
            let scan = TableScan::try_new(
                recursive.name.clone(),
                provider_as_source(work_table),
                None,
                Vec::new(),
                None,
            )?;
            if !seen.insert(recursive.name.clone()) {
                let same = recursive_ctes.iter().any(|existing| existing.name == recursive.name
                    && existing.static_term == *recursive.static_term
                    && existing.recursive_term == *recursive.recursive_term
                    && existing.is_distinct == recursive.is_distinct);
                if !same {
                    return Err(datafusion::common::DataFusionError::Plan(format!(
                        "conflicting definitions for recursive CTE {}", recursive.name)));
                }
            } else {
                recursive_ctes.push(RecursiveCte {
                    name: recursive.name,
                    static_term: recursive.static_term.as_ref().clone(),
                    recursive_term: recursive.recursive_term.as_ref().clone(),
                    is_distinct: recursive.is_distinct,
                });
            }
            Ok(Transformed::yes(LogicalPlan::TableScan(scan)))
        }
        LogicalPlan::SubqueryAlias(alias)
            if alias.alias.table().starts_with("__w_collect_unique")
                || alias.alias.table().starts_with("__w_sql_cte_") =>
        {
            let original = alias.alias.table().to_string();
            // Optimizers may push different filters/projections into copies
            // of one named boundary. A name alone is not semantic identity:
            // reusing its first definition would silently discard predicates.
            let reused = plain_versions.get(&original).into_iter().flatten()
                .find(|&&index| plain_ctes[index].term == *alias.input)
                .map(|&index| plain_ctes[index].name.clone());
            let name = if let Some(name) = reused { name } else {
                let mut name = original.clone();
                let mut version = 0;
                while !seen.insert(name.clone()) {
                    version += 1;
                    name = format!("{original}_variant_{version}");
                }
                plain_versions.entry(original.clone()).or_default().push(plain_ctes.len());
                plain_ctes.push(PlainCte { name: name.clone(), term: alias.input.as_ref().clone() });
                name
            };
            let schema = Arc::new(alias.schema.as_arrow().clone());
            let work_table = Arc::new(CteWorkTable::new(&name, schema));
            let scan = TableScan::try_new(
                name.clone(), provider_as_source(work_table), None, Vec::new(), None,
            )?;
            let replacement = if name == original {
                LogicalPlan::TableScan(scan)
            } else {
                datafusion::logical_expr::LogicalPlanBuilder::from(LogicalPlan::TableScan(scan))
                    .alias(alias.alias)?.build()?
            };
            Ok(Transformed::yes(replacement))
        }
        other => Ok(Transformed::no(other)),
    })?;
    Ok((transformed.data, plain_ctes, recursive_ctes))
}

fn hoist_operator(
    term: LogicalPlan,
    kind: &str,
    plain_ctes: &mut Vec<PlainCte>,
    seen: &mut BTreeSet<String>,
) -> datafusion::common::Result<Transformed<LogicalPlan>> {
    let prefix = format!("__w_sql_cte_{kind}_operator_");
    let reused = plain_ctes.iter().find(|cte| cte.name.starts_with(&prefix) && cte.term == term);
    let name = if let Some(cte) = reused { cte.name.clone() } else {
        let mut index = plain_ctes.len();
        let name = loop {
            let candidate = format!("{prefix}{index}");
            if seen.insert(candidate.clone()) { break candidate; }
            index += 1;
        };
        plain_ctes.push(PlainCte { name: name.clone(), term: term.clone() });
        name
    };
    let schema = Arc::new(term.schema().as_arrow().clone());
    let work_table = Arc::new(CteWorkTable::new(&name, schema));
    let scan = TableScan::try_new(name, provider_as_source(work_table), None, Vec::new(), None)?;
    Ok(Transformed::yes(LogicalPlan::TableScan(scan)))
}

/// Replace only providers created by range lowering, never user table names.
fn prepare_ranges(plan: &LogicalPlan, statement: &mut datafusion::sql::sqlparser::ast::Statement, dialect: SqlDialect) -> SqlResult<()> {
    use datafusion::sql::sqlparser::{ast, dialect::GenericDialect, parser::Parser};
    use std::{collections::BTreeMap, ops::ControlFlow};
    let mut ranges = BTreeMap::new();
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            if let Ok(provider) = datafusion::datasource::source_as_provider(&scan.source) {
                if let Some(range) = provider.as_any().downcast_ref::<super::super::range::IntegerRange>() {
                    let sql = format!("SELECT * FROM generate_series({}, {}, {}) AS {}({})", range.start, range.stop, range.step,
                        dialect.quote_ident(scan.table_name.table()), dialect.quote_ident(&range.column));
                    let mut parsed = Parser::parse_sql(&GenericDialect, &sql).map_err(|error| datafusion::common::DataFusionError::Plan(error.to_string()))?;
                    if let ast::Statement::Query(query) = parsed.remove(0) {
                        if let ast::SetExpr::Select(select) = *query.body {
                            ranges.insert(scan.table_name.table().to_owned(), select.from[0].relation.clone());
                        }
                    }
                }
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    struct Replace(BTreeMap<String, ast::TableFactor>);
    impl ast::VisitorMut for Replace {
        type Break = ();
        fn post_visit_table_factor(&mut self, factor: &mut ast::TableFactor) -> ControlFlow<()> {
            if let ast::TableFactor::Table { name, alias, .. } = factor {
                if let Some(mut replacement) = self.0.get(&name.to_string().trim_matches('"').to_owned()).cloned() {
                    if let (Some(alias), ast::TableFactor::Table { alias: target, .. }) = (alias, &mut replacement) {
                        if let Some(target) = target { target.name = alias.name.clone(); }
                    }
                    *factor = replacement;
                }
            }
            ControlFlow::Continue(())
        }
    }
    let _ = ast::VisitMut::visit(statement, &mut Replace(ranges));
    Ok(())
}

#[cfg(all(test, feature = "duckdb"))]
mod cte_tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::empty::EmptyTable;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use datafusion::prelude::{col, lit};

    #[test]
    fn conflicting_named_ctes_keep_each_filter() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
        let source = provider_as_source(Arc::new(EmptyTable::new(schema)));
        let input = LogicalPlanBuilder::scan("numbers", source, None).unwrap().build().unwrap();
        let branch = |n| LogicalPlanBuilder::from(input.clone())
            .filter(col("n").eq(lit(n))).unwrap()
            .alias("__w_sql_cte_shared").unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(branch(1_i64)).union(branch(2_i64)).unwrap().build().unwrap();
        let sql = unparse_plan(plan, SqlDialect::DuckDb).unwrap();
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE numbers(n BIGINT); INSERT INTO numbers VALUES (1),(2),(3)").unwrap();
        let mut statement = db.prepare(&sql).unwrap();
        let mut values = statement.query_map([], |row| row.get::<_, i64>(0)).unwrap()
            .collect::<duckdb::Result<Vec<_>>>().unwrap();
        values.sort();
        assert_eq!(values, vec![1, 2], "{sql}");
    }

    #[test]
    fn identical_named_ctes_share_one_definition() {
        let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
        let source = provider_as_source(Arc::new(EmptyTable::new(schema)));
        let input = LogicalPlanBuilder::scan("numbers", source, None).unwrap()
            .alias("__w_sql_cte_shared").unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input.clone()).union(input).unwrap().build().unwrap();
        let (_, definitions, _) = extract_ctes(plan, false).unwrap();
        assert_eq!(definitions.len(), 1);
    }
}
