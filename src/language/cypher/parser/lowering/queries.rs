use crate::grammar::generated::cypher::cypherparser::{
    OC_MultiPartQueryContext, OC_MultiPartQueryContextAttrs, OC_QueryContext, OC_QueryContextAttrs,
    OC_RegularQueryContext, OC_RegularQueryContextAttrs, OC_SinglePartQueryContext,
    OC_SinglePartQueryContextAttrs, OC_SingleQueryContext, OC_SingleQueryContextAttrs,
    OC_UnionContext, OC_UnionContextAttrs,
};
use crate::language::cypher::ast::{Clause, ProcedureCallClause, Query, ReturnClause, UnionBranch};
use crate::language::cypher::parser::Result;
use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::token::Token;

use super::{context, dispatch, procedures, updating};

pub(crate) fn lower_query(ctx: &OC_QueryContext<'_>) -> Result<Query> {
    if let Some(regular) = ctx.oC_RegularQuery() {
        return lower_regular_query(regular.as_ref());
    }
    if let Some(call) = ctx.oC_StandaloneCall() {
        let call = procedures::lower_standalone_call(call.as_ref())?;
        return Ok(Query::new(vec![Clause::Call(ProcedureCallClause {
            name: call.name,
            args: call.args,
            yields: call.yields,
            yield_all: call.yield_all,
            predicate: call.predicate,
            standalone: true,
        })]));
    }
    context::missing("Cypher query missing regular query or standalone CALL")
}

pub(crate) fn lower_regular_query(ctx: &OC_RegularQueryContext<'_>) -> Result<Query> {
    let Some(single) = ctx.oC_SingleQuery() else {
        return context::missing("regular Cypher query missing single query");
    };
    let base = lower_single_query(single.as_ref())?;
    let unions = ctx
        .oC_Union_all()
        .into_iter()
        .map(|union| lower_union(union.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(base.with_unions(unions))
}

fn lower_union(ctx: &OC_UnionContext<'_>) -> Result<UnionBranch> {
    let Some(single) = ctx.oC_SingleQuery() else {
        return context::missing("UNION missing branch query");
    };
    Ok(UnionBranch {
        all: ctx.ALL().is_some(),
        query: Box::new(lower_single_query(single.as_ref())?),
    })
}

pub(crate) fn lower_single_query(ctx: &OC_SingleQueryContext<'_>) -> Result<Query> {
    if let Some(single) = ctx.oC_SinglePartQuery() {
        return lower_single_part_query(single.as_ref());
    }
    if let Some(multi) = ctx.oC_MultiPartQuery() {
        return lower_multi_part_query(multi.as_ref());
    }
    context::missing("single Cypher query had no query body")
}

pub(crate) fn lower_single_part_query(ctx: &OC_SinglePartQueryContext<'_>) -> Result<Query> {
    let mut clauses = Vec::new();
    for reading in ctx.oC_ReadingClause_all() {
        clauses.push(dispatch::lower_reading_clause(reading.as_ref())?);
    }
    for updating_clause in ctx.oC_UpdatingClause_all() {
        clauses.push(updating::lower_updating_clause(updating_clause.as_ref())?);
    }
    if let Some(ret) = ctx.oC_Return() {
        clauses.push(Clause::Return(ReturnClause {
            projection: dispatch::lower_return_clause(ret.as_ref())?.projection,
        }));
    }
    Ok(Query::new(clauses))
}

pub(crate) fn lower_multi_part_query(ctx: &OC_MultiPartQueryContext<'_>) -> Result<Query> {
    let mut clauses = Vec::new();

    let mut ordered = Vec::new();
    for reading in ctx.oC_ReadingClause_all() {
        let index = reading.start().get_token_index();
        ordered.push((index, MultiPartClause::Reading(reading)));
    }
    for updating_clause in ctx.oC_UpdatingClause_all() {
        let index = updating_clause.start().get_token_index();
        ordered.push((index, MultiPartClause::Updating(updating_clause)));
    }
    for with in ctx.oC_With_all() {
        let index = with.start().get_token_index();
        ordered.push((index, MultiPartClause::With(with)));
    }
    ordered.sort_by_key(|(index, _)| *index);

    for (_, clause) in ordered {
        match clause {
            MultiPartClause::Reading(reading) => {
                clauses.push(dispatch::lower_reading_clause(reading.as_ref())?);
            }
            MultiPartClause::Updating(updating_clause) => {
                clauses.push(updating::lower_updating_clause(updating_clause.as_ref())?);
            }
            MultiPartClause::With(with) => {
                clauses.push(Clause::With(dispatch::lower_with_clause(with.as_ref())?));
            }
        }
    }

    if let Some(single) = ctx.oC_SinglePartQuery() {
        clauses.extend(lower_single_part_query(single.as_ref())?.clauses);
    }
    Ok(Query::new(clauses))
}

enum MultiPartClause<'input> {
    Reading(
        std::rc::Rc<
            crate::grammar::generated::cypher::cypherparser::OC_ReadingClauseContextAll<'input>,
        >,
    ),
    Updating(
        std::rc::Rc<
            crate::grammar::generated::cypher::cypherparser::OC_UpdatingClauseContextAll<'input>,
        >,
    ),
    With(std::rc::Rc<crate::grammar::generated::cypher::cypherparser::OC_WithContextAll<'input>>),
}
