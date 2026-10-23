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
use datafusion::logical_expr::{Expr, LogicalPlan, TableScan};
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
    let plan = if matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
        restore_quantifier_payload(plan)?
    } else { plan };
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
            || cte.name.starts_with("__w_sql_cte_apply_row_")
            || cte.name.starts_with("__w_sql_cte_quantifier_base_")) {
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

/// Restore invariant quantifier payload only at SQL emission. The relational
/// executor keeps the original single-stream aggregate; SQL consumers share
/// one explicitly materialized occurrence relation, including its row numbers.
fn restore_quantifier_payload(plan: LogicalPlan) -> datafusion::common::Result<LogicalPlan> {
    use datafusion::common::Column;
    use datafusion::logical_expr::{Expr, JoinType, LogicalPlanBuilder};
    let col = |name: &str| Expr::Column(Column::new_unqualified(name));
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Aggregate(aggregate) = &node else { return Ok(Transformed::no(node)); };
        let [Expr::Column(identity)] = aggregate.group_expr.as_slice() else { return Ok(Transformed::no(node)); };
        let Some(suffix) = identity.name.strip_prefix("__w_quantifier_row_") else { return Ok(Transformed::no(node)); };
        let base_name = format!("__w_sql_cte_quantifier_base_{suffix}");
        let mut base = None;
        aggregate.input.apply(|child| {
            if let LogicalPlan::SubqueryAlias(alias) = child {
                if alias.alias.table() == base_name {
                    base = Some(child.clone());
                    return Ok(TreeNodeRecursion::Stop);
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        let Some(base) = base else { return Ok(Transformed::no(node)); };
        let mut carried = Vec::new();
        let mut counts = Vec::new();
        for expr in &aggregate.aggr_expr {
            let Expr::Alias(alias) = expr else { return Ok(Transformed::no(node)); };
            let Expr::AggregateFunction(call) = alias.expr.as_ref() else { return Ok(Transformed::no(node)); };
            if call.func.name() == "first_value" {
                if !call.func.inner().as_any().is::<datafusion::functions_aggregate::first_last::FirstValue>()
                    || call.params.filter.is_some() || !call.params.order_by.is_empty()
                    || call.params.distinct || call.params.null_treatment.is_some() {
                    return Ok(Transformed::no(node));
                }
                let [Expr::Column(column)] = call.params.args.as_slice() else { return Ok(Transformed::no(node)); };
                if !base.schema().has_column_with_unqualified_name(&column.name) {
                    return Ok(Transformed::no(node));
                }
                carried.push((column.name.clone(),alias.name.clone()));
            } else {
                counts.push(expr.clone());
            }
        }
        if carried.is_empty() || counts.is_empty() { return Ok(Transformed::no(node)); }
        let count_key = format!("__w_quantifier_count_row_{suffix}");
        let mut count_columns = vec![col(&identity.name).alias(&count_key)];
        for expr in &counts {
            let Expr::Alias(alias) = expr else { unreachable!() };
            count_columns.push(col(&alias.name));
        }
        let counts = LogicalPlanBuilder::from(aggregate.input.as_ref().clone())
            .aggregate(vec![col(&identity.name)],counts)?.project(count_columns)?
            .alias(format!("__w_sql_cte_quantifier_counts_{suffix}"))?.build()?;
        let joined = LogicalPlanBuilder::from(base)
            .join_on(counts,JoinType::Inner,vec![col(&identity.name).eq(col(&count_key))])?.build()?;
        let mut output = vec![col(&identity.name)];
        for expr in &aggregate.aggr_expr {
            let Expr::Alias(alias) = expr else { unreachable!() };
            if let Some((source,_)) = carried.iter().find(|(_,target)| target == &alias.name) {
                output.push(col(source).alias(&alias.name));
            } else {
                output.push(col(&alias.name));
            }
        }
        Ok(Transformed::yes(LogicalPlanBuilder::from(joined).project(output)?.build()?))
    })?.data)
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

// DataFusion's bare-Aggregate unparser emits aggregate results before group
// keys, while its logical schema orders group keys first. An explicit output
// projection fixes that order without adding a derived-table SQL boundary.
fn needs_aggregate_output_projection(mut plan: &LogicalPlan) -> bool {
    loop {
        plan = match plan {
            // A bare join with a UNION/projection on the left can otherwise
            // emit that child's SELECT alone, losing its right-hand columns.
            LogicalPlan::Join(_) | LogicalPlan::Window(_) => return true,
            LogicalPlan::Aggregate(aggregate) => return !aggregate.group_expr.is_empty()
                && !aggregate.aggr_expr.is_empty(),
            LogicalPlan::Sort(sort) => &sort.input,
            LogicalPlan::Limit(limit) => &limit.input,
            LogicalPlan::Filter(filter) => &filter.input,
            // Existing projections already specify order. Do not cross joins,
            // subquery aliases or other SELECT/representation boundaries.
            _ => return false,
        };
    }
}

fn unparse_one(
    plan: &LogicalPlan,
    unparser: &Unparser<'_>,
    dialect: SqlDialect,
    repair_scopes: bool,
) -> SqlResult<String> {
    let ordered;
    let plan = if needs_aggregate_output_projection(plan) {
        ordered = datafusion::logical_expr::LogicalPlanBuilder::from(plan.clone())
            .project(plan.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>())?
            .build()?;
        &ordered
    } else { plan };
    // Generated output projections can retain qualifiers hidden behind a
    // derived relation. Normalize them after adding that final projection.
    let scoped;
    let plan = if !repair_scopes {
        scoped = super::unparse::strip_column_qualifiers(plan.clone())?;
        &scoped
    } else { plan };
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
    if dialect == SqlDialect::DuckDb {
        strip_identity_aliases(&mut statement);
        super::compact_blocks::rewrite(&mut statement);
    }
    restore_aggregate_ordering(plan, unparser, dialect, statement)
}

/// Remove only exact, unqualified projection aliases. SQL scope, expression
/// evaluation, spelling and output column names remain unchanged.
fn strip_identity_aliases(statement: &mut datafusion::sql::sqlparser::ast::Statement) {
    use datafusion::sql::sqlparser::ast;
    use std::ops::ControlFlow;
    fn body(node: &mut ast::SetExpr) {
        match node {
            ast::SetExpr::Select(select) => {
                // Projection pruning can expose a zero-column one-row seed.
                // SQL has no zero-column SELECT; an unobserved constant carries
                // its cardinality inside the enclosing explicit projection.
                if select.projection.is_empty() || (select.from.is_empty() && select.projection.iter().all(|p|
                    matches!(p, ast::SelectItem::Wildcard(_)))) {
                    select.projection = vec![ast::SelectItem::ExprWithAlias {
                        expr: ast::Expr::Value(ast::Value::Number("0".into(), false).into()),
                        alias: ast::Ident::with_quote('"', "__w_one_row"),
                    }];
                }
                for item in &mut select.projection {
                    if let ast::SelectItem::ExprWithAlias { expr: ast::Expr::Identifier(id), alias } = item
                        && id == alias
                    {
                        *item = ast::SelectItem::UnnamedExpr(ast::Expr::Identifier(id.clone()));
                    }
                }
            }
            ast::SetExpr::SetOperation { left, right, .. } => { body(left); body(right); }
            _ => {}
        }
    }
    struct Strip;
    impl ast::VisitorMut for Strip {
        type Break = ();
        fn post_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<()> {
            body(&mut query.body);
            ControlFlow::Continue(())
        }
    }
    let _ = ast::VisitMut::visit(statement, &mut Strip);
}

fn extract_ctes(plan: LogicalPlan, repair_scopes: bool) -> SqlResult<(LogicalPlan, Vec<PlainCte>, Vec<RecursiveCte>)> {
    let mut plain_ctes: Vec<PlainCte> = Vec::new();
    let mut recursive_ctes: Vec<RecursiveCte> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut plain_versions = std::collections::BTreeMap::<String, Vec<usize>>::new();
    fn aggregate_scope(plan: &LogicalPlan) -> bool {
        match plan {
            LogicalPlan::Aggregate(_) => true,
            LogicalPlan::Projection(p) => aggregate_scope(&p.input),
            LogicalPlan::SubqueryAlias(a) => aggregate_scope(&a.input),
            _ => false,
        }
    }
    let transformed = plan.transform_up(|node| match node {
        LogicalPlan::Filter(mut filter) if aggregate_scope(&filter.input) => {
            // Keep projected aggregate aliases in their own SELECT. DF53 can
            // otherwise resolve a HAVING alias against the wrong aggregate.
            filter.input = Arc::new(hoist_operator(filter.input.as_ref().clone(),
                "aggregate_filter", &mut plain_ctes, &mut seen)?.data);
            Ok(Transformed::yes(LogicalPlan::Filter(filter)))
        }
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
        LogicalPlan::Join(mut join) if matches!(join.join_type,
            datafusion::logical_expr::JoinType::Inner | datafusion::logical_expr::JoinType::Left
            | datafusion::logical_expr::JoinType::Right | datafusion::logical_expr::JoinType::Full)
            && (aggregate_scope(&join.left) || aggregate_scope(&join.right)) => {
            // DF53 can discard a projected GROUP BY when it becomes a join
            // input. Materialize its SELECT boundary before unparsing the join.
            for input in [&mut join.left, &mut join.right] {
                if aggregate_scope(input) {
                    let schema = input.schema().clone();
                    let scan = hoist_operator(input.as_ref().clone(), "aggregate_join", &mut plain_ctes, &mut seen)?.data;
                    let expressions = scan.schema().columns().into_iter().zip(schema.iter())
                        .map(|(c,(q,f))| datafusion::logical_expr::Expr::Column(c).alias_qualified(q.cloned(),f.name())).collect();
                    *input = Arc::new(LogicalPlan::Projection(datafusion::logical_expr::Projection::try_new_with_schema(expressions,Arc::new(scan),schema)?));
                }
            }
            Ok(Transformed::yes(LogicalPlan::Join(join)))
        }
        LogicalPlan::Join(mut join) if matches!(join.join_type,
            datafusion::logical_expr::JoinType::LeftSemi | datafusion::logical_expr::JoinType::LeftAnti) => {
            let mut changed = false;
            if !repair_scopes && matches!(join.left.as_ref(), LogicalPlan::Filter(_)) {
                // DF's EXISTS conversion replaces the SELECT's WHERE rather
                // than combining it with the preserved-side predicate.
                join.left = Arc::new(hoist_operator(join.left.as_ref().clone(),
                    "existence_left_filter", &mut plain_ctes, &mut seen)?.data);
                changed = true;
            }
            if join.right.schema().fields().is_empty()
                && !matches!(join.right.as_ref(), LogicalPlan::TableScan(_) | LogicalPlan::SubqueryAlias(_)) {
                // A zero-column RHS can otherwise leak its filter into the
                // outer WHERE instead of evaluating it inside EXISTS.
                join.right = Arc::new(hoist_operator(join.right.as_ref().clone(),
                    "existence_input", &mut plain_ctes, &mut seen)?.data);
                changed = true;
            }
            Ok(Transformed::new(LogicalPlan::Join(join), changed, TreeNodeRecursion::Continue))
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
            if original.starts_with("__w_sql_cte_quantifier_base_")
                && reused.is_none() && plain_versions.contains_key(&original) {
                return Err(datafusion::common::DataFusionError::Plan(format!(
                    "conflicting shared quantifier occurrence definitions for {original}")));
            }
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
                if let Some(mut replacement) = name.0.last().and_then(|part| part.as_ident()).and_then(|id| self.0.get(&id.value)).cloned() {
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

#[cfg(test)]
mod identity_alias_tests {
    use super::*;
    use datafusion::sql::sqlparser::{dialect::DuckDbDialect, parser::Parser};
    #[test]
    fn exact_identity_aliases_are_removed_in_nested_and_union_queries() {
        let mut statement = Parser::parse_sql(&DuckDbDialect {},
            "WITH q AS (SELECT \"X\" AS \"X\" FROM t) SELECT \"X\" AS \"X\" FROM q UNION ALL SELECT \"X\" AS \"X\" FROM q").unwrap().remove(0);
        strip_identity_aliases(&mut statement);
        assert_eq!(statement.to_string(),
            "WITH q AS (SELECT \"X\" FROM t) SELECT \"X\" FROM q UNION ALL SELECT \"X\" FROM q");
    }
    #[test]
    fn distinct_spelling_qualification_and_computation_keep_aliases() {
        let input = "SELECT X AS \"X\", \"x\" AS \"X\", t.x AS x, (x + 1) AS x FROM t";
        let mut statement = Parser::parse_sql(&DuckDbDialect {}, input).unwrap().remove(0);
        let original = statement.clone();
        strip_identity_aliases(&mut statement);
        assert_eq!(statement, original);
    }
}
