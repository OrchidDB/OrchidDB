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
    let (main, plain_ctes, recursive_ctes) = extract_ctes(plan)?;
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref());
    let main_sql = unparse_one(&main, &unparser, dialect)?;
    if plain_ctes.is_empty() && recursive_ctes.is_empty() {
        return Ok(dialect.fixup_query(main_sql));
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
        let static_sql = unparse_one(&cte.static_term, &unparser, dialect)?;
        let recursive_sql = unparse_one(&cte.recursive_term, &unparser, dialect)?;
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
        let term_sql = unparse_one(&cte.term, &unparser, dialect)?;
        pending.push((
            cte.name.clone(),
            deps,
            format!("{} AS (\n  {term_sql}\n)", dialect.quote_ident(&cte.name)),
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
    Ok(dialect.fixup_query(format!("{keyword} {}\n{main_sql}", definitions.join(",\n"))))
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
) -> SqlResult<String> {
    let mut statement = unparser
        .plan_to_sql(plan)
        .map_err(|err| SqlError::Unsupported(format!("unparser ({}): {err}", dialect.name())))?;
    super::functions::prepare_ast(&mut statement, dialect)?;
    restore_aggregate_ordering(plan, unparser, dialect, statement.to_string())
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
