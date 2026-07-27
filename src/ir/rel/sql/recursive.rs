//! CTE wrappers for boundaries DataFusion's SQL unparser cannot preserve.
//!
//! DataFusion can unparse each recursive term, but deliberately rejects the
//! enclosing [`LogicalPlan::RecursiveQuery`]. We replace those nodes with CTE
//! scans in the main plan, unparse every component independently, and add the
//! small wrapper the upstream unparser does not provide. The same mechanism
//! preserves explicitly marked aggregate and join scope barriers.

use std::collections::BTreeSet;
use std::sync::Arc;

use datafusion::common::tree_node::{Transformed, TreeNode};
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
    let (main, plain_ctes, recursive_ctes) = extract_ctes(plan)?;
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref());
    let main_sql = unparse_one(&main, &unparser, dialect)?;
    if plain_ctes.is_empty() && recursive_ctes.is_empty() {
        return Ok(dialect.fixup_query(main_sql));
    }

    let has_recursive = !recursive_ctes.is_empty();
    let mut definitions = Vec::with_capacity(plain_ctes.len() + recursive_ctes.len());
    for cte in recursive_ctes {
        let static_sql = unparse_one(&cte.static_term, &unparser, dialect)?;
        let recursive_sql = unparse_one(&cte.recursive_term, &unparser, dialect)?;
        let union = if cte.is_distinct {
            "UNION"
        } else {
            "UNION ALL"
        };
        definitions.push(format!(
            "{} AS (\n  {static_sql}\n  {union}\n  {recursive_sql}\n)",
            dialect.quote_ident(&cte.name)
        ));
    }
    // Plain barriers can consume recursive CTEs, so define them only after
    // every recursive work table they may reference.
    for cte in plain_ctes {
        let term_sql = unparse_one(&cte.term, &unparser, dialect)?;
        definitions.push(format!(
            "{} AS (\n  {term_sql}\n)",
            dialect.quote_ident(&cte.name)
        ));
    }
    let keyword = if has_recursive {
        "WITH RECURSIVE"
    } else {
        "WITH"
    };
    Ok(dialect.fixup_query(format!("{keyword} {}\n{main_sql}", definitions.join(",\n"))))
}

fn unparse_one(
    plan: &LogicalPlan,
    unparser: &Unparser<'_>,
    dialect: SqlDialect,
) -> SqlResult<String> {
    let statement = unparser
        .plan_to_sql(plan)
        .map_err(|err| SqlError::Unsupported(format!("unparser ({}): {err}", dialect.name())))?;
    restore_aggregate_ordering(plan, unparser, statement.to_string())
}

fn extract_ctes(plan: LogicalPlan) -> SqlResult<(LogicalPlan, Vec<PlainCte>, Vec<RecursiveCte>)> {
    let mut plain_ctes = Vec::new();
    let mut recursive_ctes = Vec::new();
    let mut seen = BTreeSet::new();
    let transformed = plan.transform_up(|node| match node {
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
            if seen.insert(recursive.name.clone()) {
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
            let name = alias.alias.table().to_string();
            let schema = Arc::new(alias.schema.as_arrow().clone());
            let work_table = Arc::new(CteWorkTable::new(&name, schema));
            let scan = TableScan::try_new(
                name.clone(),
                provider_as_source(work_table),
                None,
                Vec::new(),
                None,
            )?;
            if seen.insert(name.clone()) {
                plain_ctes.push(PlainCte {
                    name,
                    term: alias.input.as_ref().clone(),
                });
            }
            Ok(Transformed::yes(LogicalPlan::TableScan(scan)))
        }
        other => Ok(Transformed::no(other)),
    })?;
    Ok((transformed.data, plain_ctes, recursive_ctes))
}
