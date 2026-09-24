//! Legacy AST visitor and pattern literal helpers.

use crate::language::cypher::ast::*;
use antlr4rust::tree::ParseTree;

use super::expression::parse_expr_text;
use super::{CypherParseError, CypherVisitor, ParseTreeVisitor, Result};
use crate::grammar::generated::cypher::cypherparser::*;
#[derive(Default)]
pub(super) struct AstVisitor {
    query: Option<Query>,
    errors: Vec<CypherParseError>,
}

impl AstVisitor {
    pub(super) fn finish(mut self) -> Result<Query> {
        if let Some(err) = self.errors.drain(..).next() {
            return Err(err);
        }
        self.query
            .ok_or_else(|| CypherParseError::Parse("no query found".to_string()))
    }

    fn fail(&mut self, err: CypherParseError) {
        self.errors.push(err);
    }

    fn build_query(&mut self, ctx: &OC_QueryContext<'_>) -> Option<Query> {
        let regular = ctx.oC_RegularQuery()?;
        if !regular.oC_Union_all().is_empty() {
            self.fail(CypherParseError::Unsupported(
                "UNION parsing is recognized but not lowered yet".to_string(),
            ));
            return None;
        }
        self.build_regular_query(&regular)
    }

    fn build_regular_query(&mut self, ctx: &OC_RegularQueryContext<'_>) -> Option<Query> {
        let single = ctx.oC_SingleQuery()?;
        self.build_single_query(&single)
    }

    fn build_single_query(&mut self, ctx: &OC_SingleQueryContext<'_>) -> Option<Query> {
        if let Some(single) = ctx.oC_SinglePartQuery() {
            return Some(self.build_single_part_query(&single));
        }
        if let Some(multi) = ctx.oC_MultiPartQuery() {
            return Some(self.build_multi_part_query(&multi));
        }
        None
    }

    fn build_multi_part_query(&mut self, ctx: &OC_MultiPartQueryContext<'_>) -> Query {
        let mut clauses = Vec::new();
        for reading in ctx.oC_ReadingClause_all() {
            if let Some(clause) = self.build_reading_clause(&reading) {
                clauses.push(clause);
            }
        }
        for with in ctx.oC_With_all() {
            if let Some(body) = with.oC_ProjectionBody() {
                clauses.push(Clause::With(WithClause {
                    projection: self.build_projection_body(&body),
                    predicate: with
                        .oC_Where()
                        .and_then(|where_ctx| self.build_where(&where_ctx)),
                }));
            }
        }
        if let Some(single) = ctx.oC_SinglePartQuery() {
            clauses.extend(self.build_single_part_query(&single).clauses);
        }
        Query::new(clauses)
    }

    fn build_single_part_query(&mut self, ctx: &OC_SinglePartQueryContext<'_>) -> Query {
        let mut clauses = Vec::new();
        for reading in ctx.oC_ReadingClause_all() {
            if let Some(clause) = self.build_reading_clause(&reading) {
                clauses.push(clause);
            }
        }
        if !ctx.oC_UpdatingClause_all().is_empty() {
            self.fail(CypherParseError::Unsupported(
                "mutating Cypher clauses are outside the read IR".to_string(),
            ));
        }
        if let Some(ret) = ctx.oC_Return() {
            if let Some(body) = ret.oC_ProjectionBody() {
                clauses.push(Clause::Return(ReturnClause {
                    projection: self.build_projection_body(&body),
                }));
            }
        }
        Query::new(clauses)
    }

    fn build_reading_clause(&mut self, ctx: &OC_ReadingClauseContext<'_>) -> Option<Clause> {
        if let Some(m) = ctx.oC_Match() {
            return Some(Clause::Match(self.build_match(&m)));
        }
        if let Some(unwind) = ctx.oC_Unwind() {
            let expr = unwind
                .oC_Expression()
                .map(|expr| parse_expr_text(&expr.get_text()))
                .unwrap_or_else(|| Expr::Literal(Literal::Null));
            let alias = unwind
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text()))
                .unwrap_or_else(|| "_unwind".to_string());
            return Some(Clause::Unwind(UnwindClause { expr, alias }));
        }
        self.fail(CypherParseError::Unsupported(format!(
            "reading clause `{}` is not implemented yet",
            ctx.get_text()
        )));
        None
    }

    fn build_match(&mut self, ctx: &OC_MatchContext<'_>) -> MatchClause {
        let patterns = ctx
            .oC_Pattern()
            .map(|pattern| self.build_pattern(&pattern))
            .unwrap_or_default();
        MatchClause {
            optional: ctx.OPTIONAL().is_some(),
            patterns,
            predicate: ctx
                .oC_Where()
                .and_then(|where_ctx| self.build_where(&where_ctx)),
        }
    }

    fn build_where(&self, ctx: &OC_WhereContext<'_>) -> Option<Expr> {
        ctx.oC_Expression()
            .map(|expr| parse_expr_text(&expr.get_text()))
    }

    fn build_pattern(&mut self, ctx: &OC_PatternContext<'_>) -> Vec<PatternPart> {
        ctx.oC_PatternPart_all()
            .into_iter()
            .filter_map(|part| self.build_pattern_part(&part))
            .collect()
    }

    fn build_pattern_part(&mut self, ctx: &OC_PatternPartContext<'_>) -> Option<PatternPart> {
        let variable = ctx
            .oC_Variable()
            .map(|var| clean_identifier(&var.get_text()));
        let anon = ctx.oC_AnonymousPatternPart()?;
        let element = anon.oC_PatternElement()?;
        Some(PatternPart {
            variable,
            element: self.build_pattern_element(&element)?,
        })
    }

    fn build_pattern_element(
        &mut self,
        ctx: &OC_PatternElementContext<'_>,
    ) -> Option<PatternElement> {
        if let Some(nested) = ctx.oC_PatternElement() {
            return self.build_pattern_element(&nested);
        }
        let start_ctx = ctx.oC_NodePattern()?;
        let start = self.build_node_pattern(start_ctx.as_ref())?;
        let chains = ctx
            .oC_PatternElementChain_all()
            .into_iter()
            .filter_map(|chain| self.build_pattern_chain(&chain))
            .collect();
        Some(PatternElement { start, chains })
    }

    fn build_pattern_chain(
        &mut self,
        ctx: &OC_PatternElementChainContext<'_>,
    ) -> Option<PatternElementChain> {
        Some(PatternElementChain {
            relationship: self
                .build_relationship_pattern(ctx.oC_RelationshipPattern()?.as_ref())?,
            node: self.build_node_pattern(ctx.oC_NodePattern()?.as_ref())?,
        })
    }

    fn build_node_pattern(&mut self, ctx: &OC_NodePatternContext<'_>) -> Option<NodePattern> {
        Some(NodePattern {
            variable: ctx
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text())),
            labels: ctx
                .oC_NodeLabels()
                .map(|labels| {
                    labels
                        .oC_NodeLabel_all()
                        .into_iter()
                        .map(|label| clean_label(&label.get_text()))
                        .collect()
                })
                .unwrap_or_default(),
            properties: ctx
                .oC_Properties()
                .map(|props| parse_expr_text(&props.get_text())),
        })
    }

    fn build_relationship_pattern(
        &mut self,
        ctx: &OC_RelationshipPatternContext<'_>,
    ) -> Option<RelationshipPattern> {
        let direction = match (
            ctx.oC_LeftArrowHead().is_some(),
            ctx.oC_RightArrowHead().is_some(),
        ) {
            (true, false) => crate::ir::plan::Direction::In,
            (false, true) => crate::ir::plan::Direction::Out,
            _ => crate::ir::plan::Direction::Both,
        };
        let detail = ctx.oC_RelationshipDetail();
        Some(RelationshipPattern {
            variable: detail
                .as_ref()
                .and_then(|detail| detail.oC_Variable())
                .map(|var| clean_identifier(&var.get_text())),
            types: detail
                .as_ref()
                .and_then(|detail| detail.oC_RelationshipTypes())
                .map(|types| {
                    types
                        .oC_RelTypeName_all()
                        .into_iter()
                        .map(|ty| clean_label(&ty.get_text()))
                        .collect()
                })
                .unwrap_or_default(),
            range: detail
                .as_ref()
                .and_then(|detail| detail.oC_RangeLiteral())
                .map(|range| parse_range(&range.get_text()))
                .unwrap_or_default(),
            direction,
            properties: detail
                .as_ref()
                .and_then(|detail| detail.oC_Properties())
                .map(|props| parse_expr_text(&props.get_text())),
            recursive: None,
        })
    }

    fn build_projection_body(&mut self, ctx: &OC_ProjectionBodyContext<'_>) -> ProjectionBody {
        let items_ctx = ctx.oC_ProjectionItems();
        ProjectionBody {
            distinct: ctx.DISTINCT().is_some(),
            include_existing: items_ctx
                .as_ref()
                .map(|items| items.get_text().trim_start().starts_with('*'))
                .unwrap_or(false),
            items: items_ctx
                .map(|items| {
                    items
                        .oC_ProjectionItem_all()
                        .into_iter()
                        .map(|item| self.build_projection_item(&item))
                        .collect()
                })
                .unwrap_or_default(),
            order_by: ctx
                .oC_Order()
                .map(|order| {
                    order
                        .oC_SortItem_all()
                        .into_iter()
                        .map(|item| SortItem {
                            expr: item
                                .oC_Expression()
                                .map(|expr| parse_expr_text(&expr.get_text()))
                                .unwrap_or_else(|| Expr::Literal(Literal::Null)),
                            direction: if item.DESC().is_some() || item.DESCENDING().is_some() {
                                SortDirection::Desc
                            } else {
                                SortDirection::Asc
                            },
                        })
                        .collect()
                })
                .unwrap_or_default(),
            skip: ctx
                .oC_Skip()
                .and_then(|skip| skip.oC_Expression())
                .map(|expr| parse_expr_text(&expr.get_text())),
            limit: ctx
                .oC_Limit()
                .and_then(|limit| limit.oC_Expression())
                .map(|expr| parse_expr_text(&expr.get_text())),
        }
    }

    fn build_projection_item(&mut self, ctx: &OC_ProjectionItemContext<'_>) -> ProjectionItem {
        ProjectionItem {
            expr: ctx
                .oC_Expression()
                .map(|expr| parse_expr_text(&expr.get_text()))
                .unwrap_or_else(|| Expr::Literal(Literal::Null)),
            alias: ctx
                .oC_Variable()
                .map(|var| clean_identifier(&var.get_text())),
            explicit_alias: ctx.AS().is_some(),
        }
    }
}

impl<'input> ParseTreeVisitor<'input, CypherParserContextType> for AstVisitor {}

impl<'input> CypherVisitor<'input> for AstVisitor {
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        let Some(statement) = ctx.oC_Statement() else {
            self.fail(CypherParseError::Parse("missing statement".to_string()));
            return;
        };
        self.visit_oC_Statement(&statement);
    }

    fn visit_oC_Statement(&mut self, ctx: &OC_StatementContext<'input>) {
        let Some(query_ctx) = ctx.oC_Query() else {
            self.fail(CypherParseError::Parse("missing query".to_string()));
            return;
        };
        self.query = self.build_query(&query_ctx);
    }
}

pub(super) fn clean_identifier(text: &str) -> String {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(trimmed)
        .to_string()
}

pub(super) fn clean_label(text: &str) -> String {
    clean_identifier(text.trim().trim_start_matches(':').trim_start_matches('|'))
}

pub(super) fn parse_range(text: &str) -> RangeLiteral {
    let body = text.trim().trim_start_matches('*');
    if body.is_empty() {
        return RangeLiteral {
            min: 1,
            max: None,
            explicit: true,
        };
    }
    if let Some((min, max)) = body.split_once("..") {
        return RangeLiteral {
            min: min.parse().unwrap_or(1),
            max: if max.is_empty() {
                None
            } else {
                max.parse().ok()
            },
            explicit: true,
        };
    }
    let exact = body.parse().unwrap_or(1);
    RangeLiteral {
        min: exact,
        max: Some(exact),
        explicit: true,
    }
}
