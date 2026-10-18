//! Final SQL block compaction for a pure rename above SELECT *.
//! No expressions move across an aggregate, DISTINCT, or a row-producing call.
use datafusion::sql::sqlparser::ast;
use std::ops::ControlFlow;

fn plain(s: &ast::Select) -> bool {
    s.optimizer_hint.is_none() && s.distinct.is_none() && s.select_modifiers.is_none()
        && s.top.is_none() && s.exclude.is_none() && s.into.is_none()
        && s.lateral_views.is_empty() && s.prewhere.is_none() && s.connect_by.is_empty()
        && matches!(&s.group_by, ast::GroupByExpr::Expressions(v,m) if v.is_empty() && m.is_empty())
        && s.cluster_by.is_empty() && s.distribute_by.is_empty() && s.sort_by.is_empty()
        && s.having.is_none() && s.named_window.is_empty() && s.qualify.is_none()
        && s.value_table_mode.is_none()
}
pub(super) fn rewrite(statement: &mut ast::Statement) {
    struct Compact;
    impl ast::VisitorMut for Compact {
        type Break = ();
        fn post_visit_query(&mut self, q: &mut ast::Query) -> ControlFlow<()> {
            if q.with.is_some() || q.order_by.is_some() || q.limit_clause.is_some() || q.fetch.is_some()
                || !q.locks.is_empty() || q.for_clause.is_some() || q.settings.is_some()
                || q.format_clause.is_some() || !q.pipe_operators.is_empty() { return ControlFlow::Continue(()); }
            let ast::SetExpr::Select(s) = q.body.as_ref() else { return ControlFlow::Continue(()); };
            if !plain(s) || s.selection.is_some() { return ControlFlow::Continue(()); }
            let [from] = s.from.as_slice() else { return ControlFlow::Continue(()); };
            if !from.joins.is_empty() { return ControlFlow::Continue(()); }
            let ast::TableFactor::Derived { lateral:false, subquery, alias:None, .. } = &from.relation else { return ControlFlow::Continue(()); };
            let ast::SetExpr::Select(inner) = subquery.body.as_ref() else { return ControlFlow::Continue(()); };
            if !plain(inner) || !matches!(inner.projection.as_slice(),[ast::SelectItem::Wildcard(options)] if *options == Default::default()) { return ControlFlow::Continue(()); }
            let mut aliases = Vec::new();
            for p in &s.projection {
                match p {
                    ast::SelectItem::UnnamedExpr(ast::Expr::Identifier(_)) => {},
                    ast::SelectItem::ExprWithAlias { expr:ast::Expr::Identifier(id),alias } => {
                        if id != alias { aliases.push(alias.value.to_lowercase()); }
                    }
                    _ => return ControlFlow::Continue(()),
                }
            }
            // Positional ordering refers to the inner SELECT * layout. An
            // output alias must not capture an existing ORDER BY reference.
            struct OrderRefs<'a> { aliases: &'a [String], unsafe_ref: bool }
            impl ast::Visitor for OrderRefs<'_> {
                type Break = ();
                fn pre_visit_expr(&mut self, e: &ast::Expr) -> ControlFlow<()> {
                    if matches!(e,ast::Expr::Identifier(id) if self.aliases.contains(&id.value.to_lowercase()))
                        || matches!(e,ast::Expr::Value(v) if matches!(v.value,ast::Value::Number(..))) { self.unsafe_ref=true; }
                    ControlFlow::Continue(())
                }
            }
            let mut refs=OrderRefs { aliases:&aliases,unsafe_ref:false };
            let _=ast::Visit::visit(&subquery.order_by,&mut refs);
            if refs.unsafe_ref { return ControlFlow::Continue(()); }
            let mut result=subquery.as_ref().clone();
            let ast::SetExpr::Select(target)=result.body.as_mut() else { unreachable!() };
            target.projection=s.projection.clone();
            *q=result;
            ControlFlow::Continue(())
        }
    }
    let _=ast::VisitMut::visit(statement,&mut Compact);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compact(sql: &str) -> String {
        let mut statements=datafusion::sql::sqlparser::parser::Parser::parse_sql(
            &datafusion::sql::sqlparser::dialect::DuckDbDialect {},sql).unwrap();
        rewrite(&mut statements[0]); statements[0].to_string()
    }
    #[test]
    fn rename_above_sorted_limit_keeps_order_and_limit_in_final_block() {
        assert_eq!(compact("SELECT x AS result FROM (SELECT * FROM t ORDER BY x LIMIT 2)"),
            "SELECT x AS result FROM t ORDER BY x LIMIT 2");
        for sql in [
            "SELECT x AS y FROM (SELECT * FROM t ORDER BY y LIMIT 2)",
            "SELECT x FROM (SELECT * FROM t ORDER BY 2 LIMIT 2)",
            "SELECT DISTINCT x FROM (SELECT * FROM t LIMIT 2)",
            "SELECT x FROM (SELECT * FROM t LIMIT 2) WHERE x > 0",
            "SELECT x FROM (SELECT DISTINCT * FROM t LIMIT 2)",
        ] { assert!(compact(sql).contains("FROM (SELECT"),"{sql}"); }
    }
}
