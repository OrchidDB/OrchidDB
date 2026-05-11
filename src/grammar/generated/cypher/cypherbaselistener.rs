// Generated from languages/cypher/Cypher.g4 by ANTLR 4.13.2

use super::cypherparser::*;
use antlr4rust::tree::ParseTreeListener;

// A complete Visitor for a parse tree produced by CypherParser.

pub trait CypherBaseListener<'input>: ParseTreeListener<'input, CypherParserContextType> {
    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_cypher(&mut self, _ctx: &OC_CypherContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_cypher(&mut self, _ctx: &OC_CypherContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_statement(&mut self, _ctx: &OC_StatementContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_statement(&mut self, _ctx: &OC_StatementContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_query(&mut self, _ctx: &OC_QueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_query(&mut self, _ctx: &OC_QueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_regularquery(&mut self, _ctx: &OC_RegularQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_regularquery(&mut self, _ctx: &OC_RegularQueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_union(&mut self, _ctx: &OC_UnionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_union(&mut self, _ctx: &OC_UnionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_singlequery(&mut self, _ctx: &OC_SingleQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_singlequery(&mut self, _ctx: &OC_SingleQueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_singlepartquery(&mut self, _ctx: &OC_SinglePartQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_singlepartquery(&mut self, _ctx: &OC_SinglePartQueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_multipartquery(&mut self, _ctx: &OC_MultiPartQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_multipartquery(&mut self, _ctx: &OC_MultiPartQueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_updatingclause(&mut self, _ctx: &OC_UpdatingClauseContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_updatingclause(&mut self, _ctx: &OC_UpdatingClauseContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_readingclause(&mut self, _ctx: &OC_ReadingClauseContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_readingclause(&mut self, _ctx: &OC_ReadingClauseContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_match(&mut self, _ctx: &OC_MatchContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_match(&mut self, _ctx: &OC_MatchContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_unwind(&mut self, _ctx: &OC_UnwindContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_unwind(&mut self, _ctx: &OC_UnwindContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_merge(&mut self, _ctx: &OC_MergeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_merge(&mut self, _ctx: &OC_MergeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_mergeaction(&mut self, _ctx: &OC_MergeActionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_mergeaction(&mut self, _ctx: &OC_MergeActionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_create(&mut self, _ctx: &OC_CreateContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_create(&mut self, _ctx: &OC_CreateContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_set(&mut self, _ctx: &OC_SetContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_set(&mut self, _ctx: &OC_SetContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_setitem(&mut self, _ctx: &OC_SetItemContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_setitem(&mut self, _ctx: &OC_SetItemContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_delete(&mut self, _ctx: &OC_DeleteContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_delete(&mut self, _ctx: &OC_DeleteContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_remove(&mut self, _ctx: &OC_RemoveContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_remove(&mut self, _ctx: &OC_RemoveContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_removeitem(&mut self, _ctx: &OC_RemoveItemContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_removeitem(&mut self, _ctx: &OC_RemoveItemContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_inquerycall(&mut self, _ctx: &OC_InQueryCallContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_inquerycall(&mut self, _ctx: &OC_InQueryCallContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_standalonecall(&mut self, _ctx: &OC_StandaloneCallContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_standalonecall(&mut self, _ctx: &OC_StandaloneCallContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_yielditems(&mut self, _ctx: &OC_YieldItemsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_yielditems(&mut self, _ctx: &OC_YieldItemsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_yielditem(&mut self, _ctx: &OC_YieldItemContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_yielditem(&mut self, _ctx: &OC_YieldItemContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_with(&mut self, _ctx: &OC_WithContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_with(&mut self, _ctx: &OC_WithContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_return(&mut self, _ctx: &OC_ReturnContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_return(&mut self, _ctx: &OC_ReturnContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_projectionbody(&mut self, _ctx: &OC_ProjectionBodyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_projectionbody(&mut self, _ctx: &OC_ProjectionBodyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_projectionitems(&mut self, _ctx: &OC_ProjectionItemsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_projectionitems(&mut self, _ctx: &OC_ProjectionItemsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_projectionitem(&mut self, _ctx: &OC_ProjectionItemContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_projectionitem(&mut self, _ctx: &OC_ProjectionItemContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_order(&mut self, _ctx: &OC_OrderContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_order(&mut self, _ctx: &OC_OrderContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_skip(&mut self, _ctx: &OC_SkipContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_skip(&mut self, _ctx: &OC_SkipContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_limit(&mut self, _ctx: &OC_LimitContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_limit(&mut self, _ctx: &OC_LimitContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_sortitem(&mut self, _ctx: &OC_SortItemContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_sortitem(&mut self, _ctx: &OC_SortItemContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_where(&mut self, _ctx: &OC_WhereContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_where(&mut self, _ctx: &OC_WhereContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_pattern(&mut self, _ctx: &OC_PatternContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_pattern(&mut self, _ctx: &OC_PatternContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_patternpart(&mut self, _ctx: &OC_PatternPartContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_patternpart(&mut self, _ctx: &OC_PatternPartContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_anonymouspatternpart(&mut self, _ctx: &OC_AnonymousPatternPartContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_anonymouspatternpart(&mut self, _ctx: &OC_AnonymousPatternPartContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_patternelement(&mut self, _ctx: &OC_PatternElementContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_patternelement(&mut self, _ctx: &OC_PatternElementContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_relationshipspattern(&mut self, _ctx: &OC_RelationshipsPatternContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_relationshipspattern(&mut self, _ctx: &OC_RelationshipsPatternContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_nodepattern(&mut self, _ctx: &OC_NodePatternContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_nodepattern(&mut self, _ctx: &OC_NodePatternContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_patternelementchain(&mut self, _ctx: &OC_PatternElementChainContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_patternelementchain(&mut self, _ctx: &OC_PatternElementChainContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_relationshippattern(&mut self, _ctx: &OC_RelationshipPatternContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_relationshippattern(&mut self, _ctx: &OC_RelationshipPatternContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_relationshipdetail(&mut self, _ctx: &OC_RelationshipDetailContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_relationshipdetail(&mut self, _ctx: &OC_RelationshipDetailContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_properties(&mut self, _ctx: &OC_PropertiesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_properties(&mut self, _ctx: &OC_PropertiesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_relationshiptypes(&mut self, _ctx: &OC_RelationshipTypesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_relationshiptypes(&mut self, _ctx: &OC_RelationshipTypesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_nodelabels(&mut self, _ctx: &OC_NodeLabelsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_nodelabels(&mut self, _ctx: &OC_NodeLabelsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_nodelabel(&mut self, _ctx: &OC_NodeLabelContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_nodelabel(&mut self, _ctx: &OC_NodeLabelContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_rangeliteral(&mut self, _ctx: &OC_RangeLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_rangeliteral(&mut self, _ctx: &OC_RangeLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_labelname(&mut self, _ctx: &OC_LabelNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_labelname(&mut self, _ctx: &OC_LabelNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_reltypename(&mut self, _ctx: &OC_RelTypeNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_reltypename(&mut self, _ctx: &OC_RelTypeNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_propertyexpression(&mut self, _ctx: &OC_PropertyExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_propertyexpression(&mut self, _ctx: &OC_PropertyExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_expression(&mut self, _ctx: &OC_ExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_expression(&mut self, _ctx: &OC_ExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_orexpression(&mut self, _ctx: &OC_OrExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_orexpression(&mut self, _ctx: &OC_OrExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_xorexpression(&mut self, _ctx: &OC_XorExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_xorexpression(&mut self, _ctx: &OC_XorExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_andexpression(&mut self, _ctx: &OC_AndExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_andexpression(&mut self, _ctx: &OC_AndExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_notexpression(&mut self, _ctx: &OC_NotExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_notexpression(&mut self, _ctx: &OC_NotExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_comparisonexpression(&mut self, _ctx: &OC_ComparisonExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_comparisonexpression(&mut self, _ctx: &OC_ComparisonExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_partialcomparisonexpression(
        &mut self,
        _ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_partialcomparisonexpression(
        &mut self,
        _ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_stringlistnullpredicateexpression(
        &mut self,
        _ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_stringlistnullpredicateexpression(
        &mut self,
        _ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_stringpredicateexpression(
        &mut self,
        _ctx: &OC_StringPredicateExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_stringpredicateexpression(
        &mut self,
        _ctx: &OC_StringPredicateExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_listpredicateexpression(
        &mut self,
        _ctx: &OC_ListPredicateExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_listpredicateexpression(
        &mut self,
        _ctx: &OC_ListPredicateExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_nullpredicateexpression(
        &mut self,
        _ctx: &OC_NullPredicateExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_nullpredicateexpression(
        &mut self,
        _ctx: &OC_NullPredicateExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_addorsubtractexpression(
        &mut self,
        _ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_addorsubtractexpression(
        &mut self,
        _ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_multiplydividemoduloexpression(
        &mut self,
        _ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_multiplydividemoduloexpression(
        &mut self,
        _ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_powerofexpression(&mut self, _ctx: &OC_PowerOfExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_powerofexpression(&mut self, _ctx: &OC_PowerOfExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_unaryaddorsubtractexpression(
        &mut self,
        _ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_unaryaddorsubtractexpression(
        &mut self,
        _ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_nonarithmeticoperatorexpression(
        &mut self,
        _ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_nonarithmeticoperatorexpression(
        &mut self,
        _ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_listoperatorexpression(&mut self, _ctx: &OC_ListOperatorExpressionContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_listoperatorexpression(&mut self, _ctx: &OC_ListOperatorExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_propertylookup(&mut self, _ctx: &OC_PropertyLookupContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_propertylookup(&mut self, _ctx: &OC_PropertyLookupContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_atom(&mut self, _ctx: &OC_AtomContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_atom(&mut self, _ctx: &OC_AtomContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_caseexpression(&mut self, _ctx: &OC_CaseExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_caseexpression(&mut self, _ctx: &OC_CaseExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_casealternative(&mut self, _ctx: &OC_CaseAlternativeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_casealternative(&mut self, _ctx: &OC_CaseAlternativeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_listcomprehension(&mut self, _ctx: &OC_ListComprehensionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_listcomprehension(&mut self, _ctx: &OC_ListComprehensionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_patterncomprehension(&mut self, _ctx: &OC_PatternComprehensionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_patterncomprehension(&mut self, _ctx: &OC_PatternComprehensionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_quantifier(&mut self, _ctx: &OC_QuantifierContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_quantifier(&mut self, _ctx: &OC_QuantifierContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_filterexpression(&mut self, _ctx: &OC_FilterExpressionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_filterexpression(&mut self, _ctx: &OC_FilterExpressionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_patternpredicate(&mut self, _ctx: &OC_PatternPredicateContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_patternpredicate(&mut self, _ctx: &OC_PatternPredicateContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_parenthesizedexpression(
        &mut self,
        _ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_parenthesizedexpression(
        &mut self,
        _ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_idincoll(&mut self, _ctx: &OC_IdInCollContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_idincoll(&mut self, _ctx: &OC_IdInCollContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_functioninvocation(&mut self, _ctx: &OC_FunctionInvocationContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_functioninvocation(&mut self, _ctx: &OC_FunctionInvocationContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_functionname(&mut self, _ctx: &OC_FunctionNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_functionname(&mut self, _ctx: &OC_FunctionNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_existentialsubquery(&mut self, _ctx: &OC_ExistentialSubqueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_existentialsubquery(&mut self, _ctx: &OC_ExistentialSubqueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_explicitprocedureinvocation(
        &mut self,
        _ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_explicitprocedureinvocation(
        &mut self,
        _ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_implicitprocedureinvocation(
        &mut self,
        _ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_implicitprocedureinvocation(
        &mut self,
        _ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_procedureresultfield(&mut self, _ctx: &OC_ProcedureResultFieldContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_procedureresultfield(&mut self, _ctx: &OC_ProcedureResultFieldContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_procedurename(&mut self, _ctx: &OC_ProcedureNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_procedurename(&mut self, _ctx: &OC_ProcedureNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_namespace(&mut self, _ctx: &OC_NamespaceContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_namespace(&mut self, _ctx: &OC_NamespaceContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_variable(&mut self, _ctx: &OC_VariableContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_variable(&mut self, _ctx: &OC_VariableContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_literal(&mut self, _ctx: &OC_LiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_literal(&mut self, _ctx: &OC_LiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_booleanliteral(&mut self, _ctx: &OC_BooleanLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_booleanliteral(&mut self, _ctx: &OC_BooleanLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_numberliteral(&mut self, _ctx: &OC_NumberLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_numberliteral(&mut self, _ctx: &OC_NumberLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_integerliteral(&mut self, _ctx: &OC_IntegerLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_integerliteral(&mut self, _ctx: &OC_IntegerLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_doubleliteral(&mut self, _ctx: &OC_DoubleLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_doubleliteral(&mut self, _ctx: &OC_DoubleLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_listliteral(&mut self, _ctx: &OC_ListLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_listliteral(&mut self, _ctx: &OC_ListLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_mapliteral(&mut self, _ctx: &OC_MapLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_mapliteral(&mut self, _ctx: &OC_MapLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_propertykeyname(&mut self, _ctx: &OC_PropertyKeyNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_propertykeyname(&mut self, _ctx: &OC_PropertyKeyNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_parameter(&mut self, _ctx: &OC_ParameterContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_parameter(&mut self, _ctx: &OC_ParameterContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_schemaname(&mut self, _ctx: &OC_SchemaNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_schemaname(&mut self, _ctx: &OC_SchemaNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_reservedword(&mut self, _ctx: &OC_ReservedWordContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_reservedword(&mut self, _ctx: &OC_ReservedWordContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_symbolicname(&mut self, _ctx: &OC_SymbolicNameContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_symbolicname(&mut self, _ctx: &OC_SymbolicNameContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_leftarrowhead(&mut self, _ctx: &OC_LeftArrowHeadContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_leftarrowhead(&mut self, _ctx: &OC_LeftArrowHeadContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_rightarrowhead(&mut self, _ctx: &OC_RightArrowHeadContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_rightarrowhead(&mut self, _ctx: &OC_RightArrowHeadContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_oc_dash(&mut self, _ctx: &OC_DashContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  CypherBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_oc_dash(&mut self, _ctx: &OC_DashContext<'input>) {}
}
