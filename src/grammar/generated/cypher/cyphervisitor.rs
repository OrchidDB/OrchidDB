#![allow(nonstandard_style)]
// Generated from languages/cypher/Cypher.g4 by ANTLR 4.13.2
use super::cypherparser::*;
use antlr4rust::tree::{ParseTreeVisitor, ParseTreeVisitorCompat};

/**
 * This interface defines a complete generic visitor for a parse tree produced
 * by {@link CypherParser}.
 */
pub trait CypherVisitor<'input>: ParseTreeVisitor<'input, CypherParserContextType> {
    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Cypher}.
     * @param ctx the parse tree
     */
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Statement}.
     * @param ctx the parse tree
     */
    fn visit_oC_Statement(&mut self, ctx: &OC_StatementContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Query}.
     * @param ctx the parse tree
     */
    fn visit_oC_Query(&mut self, ctx: &OC_QueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RegularQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_RegularQuery(&mut self, ctx: &OC_RegularQueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Union}.
     * @param ctx the parse tree
     */
    fn visit_oC_Union(&mut self, ctx: &OC_UnionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SingleQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_SingleQuery(&mut self, ctx: &OC_SingleQueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SinglePartQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_SinglePartQuery(&mut self, ctx: &OC_SinglePartQueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MultiPartQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_MultiPartQuery(&mut self, ctx: &OC_MultiPartQueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_UpdatingClause}.
     * @param ctx the parse tree
     */
    fn visit_oC_UpdatingClause(&mut self, ctx: &OC_UpdatingClauseContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ReadingClause}.
     * @param ctx the parse tree
     */
    fn visit_oC_ReadingClause(&mut self, ctx: &OC_ReadingClauseContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Match}.
     * @param ctx the parse tree
     */
    fn visit_oC_Match(&mut self, ctx: &OC_MatchContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Unwind}.
     * @param ctx the parse tree
     */
    fn visit_oC_Unwind(&mut self, ctx: &OC_UnwindContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Merge}.
     * @param ctx the parse tree
     */
    fn visit_oC_Merge(&mut self, ctx: &OC_MergeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MergeAction}.
     * @param ctx the parse tree
     */
    fn visit_oC_MergeAction(&mut self, ctx: &OC_MergeActionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Create}.
     * @param ctx the parse tree
     */
    fn visit_oC_Create(&mut self, ctx: &OC_CreateContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Set}.
     * @param ctx the parse tree
     */
    fn visit_oC_Set(&mut self, ctx: &OC_SetContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SetItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_SetItem(&mut self, ctx: &OC_SetItemContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Delete}.
     * @param ctx the parse tree
     */
    fn visit_oC_Delete(&mut self, ctx: &OC_DeleteContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Remove}.
     * @param ctx the parse tree
     */
    fn visit_oC_Remove(&mut self, ctx: &OC_RemoveContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RemoveItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_RemoveItem(&mut self, ctx: &OC_RemoveItemContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_InQueryCall}.
     * @param ctx the parse tree
     */
    fn visit_oC_InQueryCall(&mut self, ctx: &OC_InQueryCallContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StandaloneCall}.
     * @param ctx the parse tree
     */
    fn visit_oC_StandaloneCall(&mut self, ctx: &OC_StandaloneCallContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_YieldItems}.
     * @param ctx the parse tree
     */
    fn visit_oC_YieldItems(&mut self, ctx: &OC_YieldItemsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_YieldItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_YieldItem(&mut self, ctx: &OC_YieldItemContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_With}.
     * @param ctx the parse tree
     */
    fn visit_oC_With(&mut self, ctx: &OC_WithContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Return}.
     * @param ctx the parse tree
     */
    fn visit_oC_Return(&mut self, ctx: &OC_ReturnContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionBody}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionBody(&mut self, ctx: &OC_ProjectionBodyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionItems}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionItems(&mut self, ctx: &OC_ProjectionItemsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionItem(&mut self, ctx: &OC_ProjectionItemContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Order}.
     * @param ctx the parse tree
     */
    fn visit_oC_Order(&mut self, ctx: &OC_OrderContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Skip}.
     * @param ctx the parse tree
     */
    fn visit_oC_Skip(&mut self, ctx: &OC_SkipContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Limit}.
     * @param ctx the parse tree
     */
    fn visit_oC_Limit(&mut self, ctx: &OC_LimitContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SortItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_SortItem(&mut self, ctx: &OC_SortItemContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Where}.
     * @param ctx the parse tree
     */
    fn visit_oC_Where(&mut self, ctx: &OC_WhereContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Pattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_Pattern(&mut self, ctx: &OC_PatternContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternPart}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternPart(&mut self, ctx: &OC_PatternPartContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AnonymousPatternPart}.
     * @param ctx the parse tree
     */
    fn visit_oC_AnonymousPatternPart(&mut self, ctx: &OC_AnonymousPatternPartContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternElement}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternElement(&mut self, ctx: &OC_PatternElementContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipsPattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipsPattern(&mut self, ctx: &OC_RelationshipsPatternContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodePattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodePattern(&mut self, ctx: &OC_NodePatternContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternElementChain}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternElementChain(&mut self, ctx: &OC_PatternElementChainContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipPattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipPattern(&mut self, ctx: &OC_RelationshipPatternContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipDetail}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipDetail(&mut self, ctx: &OC_RelationshipDetailContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RecursiveRelationshipFilter}.
     * @param ctx the parse tree
     */
    fn visit_oC_RecursiveRelationshipFilter(
        &mut self,
        ctx: &OC_RecursiveRelationshipFilterContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Properties}.
     * @param ctx the parse tree
     */
    fn visit_oC_Properties(&mut self, ctx: &OC_PropertiesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipTypes}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipTypes(&mut self, ctx: &OC_RelationshipTypesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodeLabels}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodeLabels(&mut self, ctx: &OC_NodeLabelsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodeLabel}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodeLabel(&mut self, ctx: &OC_NodeLabelContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RangeLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_RangeLiteral(&mut self, ctx: &OC_RangeLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_LabelName}.
     * @param ctx the parse tree
     */
    fn visit_oC_LabelName(&mut self, ctx: &OC_LabelNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelTypeName}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelTypeName(&mut self, ctx: &OC_RelTypeNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyExpression(&mut self, ctx: &OC_PropertyExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Expression}.
     * @param ctx the parse tree
     */
    fn visit_oC_Expression(&mut self, ctx: &OC_ExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_OrExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_OrExpression(&mut self, ctx: &OC_OrExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_XorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_XorExpression(&mut self, ctx: &OC_XorExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AndExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_AndExpression(&mut self, ctx: &OC_AndExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NotExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NotExpression(&mut self, ctx: &OC_NotExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ComparisonExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ComparisonExpression(&mut self, ctx: &OC_ComparisonExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PartialComparisonExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PartialComparisonExpression(
        &mut self,
        ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StringListNullPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_StringListNullPredicateExpression(
        &mut self,
        ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StringPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_StringPredicateExpression(
        &mut self,
        ctx: &OC_StringPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListPredicateExpression(
        &mut self,
        ctx: &OC_ListPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NullPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NullPredicateExpression(
        &mut self,
        ctx: &OC_NullPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AddOrSubtractExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_AddOrSubtractExpression(
        &mut self,
        ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MultiplyDivideModuloExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_MultiplyDivideModuloExpression(
        &mut self,
        ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PowerOfExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PowerOfExpression(&mut self, ctx: &OC_PowerOfExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_UnaryAddOrSubtractExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_UnaryAddOrSubtractExpression(
        &mut self,
        ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NonArithmeticOperatorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NonArithmeticOperatorExpression(
        &mut self,
        ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListOperatorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListOperatorExpression(&mut self, ctx: &OC_ListOperatorExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyLookup}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyLookup(&mut self, ctx: &OC_PropertyLookupContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Atom}.
     * @param ctx the parse tree
     */
    fn visit_oC_Atom(&mut self, ctx: &OC_AtomContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CaseExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_CaseExpression(&mut self, ctx: &OC_CaseExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CaseAlternative}.
     * @param ctx the parse tree
     */
    fn visit_oC_CaseAlternative(&mut self, ctx: &OC_CaseAlternativeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListComprehension}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListComprehension(&mut self, ctx: &OC_ListComprehensionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternComprehension}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternComprehension(&mut self, ctx: &OC_PatternComprehensionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Quantifier}.
     * @param ctx the parse tree
     */
    fn visit_oC_Quantifier(&mut self, ctx: &OC_QuantifierContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FilterExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_FilterExpression(&mut self, ctx: &OC_FilterExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternPredicate}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternPredicate(&mut self, ctx: &OC_PatternPredicateContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ParenthesizedExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ParenthesizedExpression(
        &mut self,
        ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_IdInColl}.
     * @param ctx the parse tree
     */
    fn visit_oC_IdInColl(&mut self, ctx: &OC_IdInCollContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FunctionInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_FunctionInvocation(&mut self, ctx: &OC_FunctionInvocationContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastExpression(&mut self, ctx: &OC_CastExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastType}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastType(&mut self, ctx: &OC_CastTypeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeArgument}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeArgument(&mut self, ctx: &OC_CastTypeArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeField}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeField(&mut self, ctx: &OC_CastTypeFieldContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeName}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeName(&mut self, ctx: &OC_CastTypeNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FunctionName}.
     * @param ctx the parse tree
     */
    fn visit_oC_FunctionName(&mut self, ctx: &OC_FunctionNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ExistentialSubquery}.
     * @param ctx the parse tree
     */
    fn visit_oC_ExistentialSubquery(&mut self, ctx: &OC_ExistentialSubqueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ExplicitProcedureInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_ExplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ImplicitProcedureInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_ImplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProcedureResultField}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProcedureResultField(&mut self, ctx: &OC_ProcedureResultFieldContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProcedureName}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProcedureName(&mut self, ctx: &OC_ProcedureNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Namespace}.
     * @param ctx the parse tree
     */
    fn visit_oC_Namespace(&mut self, ctx: &OC_NamespaceContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Variable}.
     * @param ctx the parse tree
     */
    fn visit_oC_Variable(&mut self, ctx: &OC_VariableContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Literal}.
     * @param ctx the parse tree
     */
    fn visit_oC_Literal(&mut self, ctx: &OC_LiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_BooleanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_BooleanLiteral(&mut self, ctx: &OC_BooleanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NumberLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_NumberLiteral(&mut self, ctx: &OC_NumberLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_IntegerLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_IntegerLiteral(&mut self, ctx: &OC_IntegerLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_DoubleLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_DoubleLiteral(&mut self, ctx: &OC_DoubleLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListLiteral(&mut self, ctx: &OC_ListLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MapLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_MapLiteral(&mut self, ctx: &OC_MapLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyKeyName}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyKeyName(&mut self, ctx: &OC_PropertyKeyNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Parameter}.
     * @param ctx the parse tree
     */
    fn visit_oC_Parameter(&mut self, ctx: &OC_ParameterContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SchemaName}.
     * @param ctx the parse tree
     */
    fn visit_oC_SchemaName(&mut self, ctx: &OC_SchemaNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ReservedWord}.
     * @param ctx the parse tree
     */
    fn visit_oC_ReservedWord(&mut self, ctx: &OC_ReservedWordContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SymbolicName}.
     * @param ctx the parse tree
     */
    fn visit_oC_SymbolicName(&mut self, ctx: &OC_SymbolicNameContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_LeftArrowHead}.
     * @param ctx the parse tree
     */
    fn visit_oC_LeftArrowHead(&mut self, ctx: &OC_LeftArrowHeadContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RightArrowHead}.
     * @param ctx the parse tree
     */
    fn visit_oC_RightArrowHead(&mut self, ctx: &OC_RightArrowHeadContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Dash}.
     * @param ctx the parse tree
     */
    fn visit_oC_Dash(&mut self, ctx: &OC_DashContext<'input>) {
        self.visit_children(ctx)
    }
}

pub trait CypherVisitorCompat<'input>:
    ParseTreeVisitorCompat<'input, Node = CypherParserContextType>
{
    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Cypher}.
     * @param ctx the parse tree
     */
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Statement}.
     * @param ctx the parse tree
     */
    fn visit_oC_Statement(&mut self, ctx: &OC_StatementContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Query}.
     * @param ctx the parse tree
     */
    fn visit_oC_Query(&mut self, ctx: &OC_QueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RegularQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_RegularQuery(&mut self, ctx: &OC_RegularQueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Union}.
     * @param ctx the parse tree
     */
    fn visit_oC_Union(&mut self, ctx: &OC_UnionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SingleQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_SingleQuery(&mut self, ctx: &OC_SingleQueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SinglePartQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_SinglePartQuery(
        &mut self,
        ctx: &OC_SinglePartQueryContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MultiPartQuery}.
     * @param ctx the parse tree
     */
    fn visit_oC_MultiPartQuery(&mut self, ctx: &OC_MultiPartQueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_UpdatingClause}.
     * @param ctx the parse tree
     */
    fn visit_oC_UpdatingClause(&mut self, ctx: &OC_UpdatingClauseContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ReadingClause}.
     * @param ctx the parse tree
     */
    fn visit_oC_ReadingClause(&mut self, ctx: &OC_ReadingClauseContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Match}.
     * @param ctx the parse tree
     */
    fn visit_oC_Match(&mut self, ctx: &OC_MatchContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Unwind}.
     * @param ctx the parse tree
     */
    fn visit_oC_Unwind(&mut self, ctx: &OC_UnwindContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Merge}.
     * @param ctx the parse tree
     */
    fn visit_oC_Merge(&mut self, ctx: &OC_MergeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MergeAction}.
     * @param ctx the parse tree
     */
    fn visit_oC_MergeAction(&mut self, ctx: &OC_MergeActionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Create}.
     * @param ctx the parse tree
     */
    fn visit_oC_Create(&mut self, ctx: &OC_CreateContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Set}.
     * @param ctx the parse tree
     */
    fn visit_oC_Set(&mut self, ctx: &OC_SetContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SetItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_SetItem(&mut self, ctx: &OC_SetItemContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Delete}.
     * @param ctx the parse tree
     */
    fn visit_oC_Delete(&mut self, ctx: &OC_DeleteContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Remove}.
     * @param ctx the parse tree
     */
    fn visit_oC_Remove(&mut self, ctx: &OC_RemoveContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RemoveItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_RemoveItem(&mut self, ctx: &OC_RemoveItemContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_InQueryCall}.
     * @param ctx the parse tree
     */
    fn visit_oC_InQueryCall(&mut self, ctx: &OC_InQueryCallContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StandaloneCall}.
     * @param ctx the parse tree
     */
    fn visit_oC_StandaloneCall(&mut self, ctx: &OC_StandaloneCallContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_YieldItems}.
     * @param ctx the parse tree
     */
    fn visit_oC_YieldItems(&mut self, ctx: &OC_YieldItemsContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_YieldItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_YieldItem(&mut self, ctx: &OC_YieldItemContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_With}.
     * @param ctx the parse tree
     */
    fn visit_oC_With(&mut self, ctx: &OC_WithContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Return}.
     * @param ctx the parse tree
     */
    fn visit_oC_Return(&mut self, ctx: &OC_ReturnContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionBody}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionBody(&mut self, ctx: &OC_ProjectionBodyContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionItems}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionItems(
        &mut self,
        ctx: &OC_ProjectionItemsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProjectionItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProjectionItem(&mut self, ctx: &OC_ProjectionItemContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Order}.
     * @param ctx the parse tree
     */
    fn visit_oC_Order(&mut self, ctx: &OC_OrderContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Skip}.
     * @param ctx the parse tree
     */
    fn visit_oC_Skip(&mut self, ctx: &OC_SkipContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Limit}.
     * @param ctx the parse tree
     */
    fn visit_oC_Limit(&mut self, ctx: &OC_LimitContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SortItem}.
     * @param ctx the parse tree
     */
    fn visit_oC_SortItem(&mut self, ctx: &OC_SortItemContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Where}.
     * @param ctx the parse tree
     */
    fn visit_oC_Where(&mut self, ctx: &OC_WhereContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Pattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_Pattern(&mut self, ctx: &OC_PatternContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternPart}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternPart(&mut self, ctx: &OC_PatternPartContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AnonymousPatternPart}.
     * @param ctx the parse tree
     */
    fn visit_oC_AnonymousPatternPart(
        &mut self,
        ctx: &OC_AnonymousPatternPartContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternElement}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternElement(&mut self, ctx: &OC_PatternElementContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipsPattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipsPattern(
        &mut self,
        ctx: &OC_RelationshipsPatternContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodePattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodePattern(&mut self, ctx: &OC_NodePatternContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternElementChain}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternElementChain(
        &mut self,
        ctx: &OC_PatternElementChainContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipPattern}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipPattern(
        &mut self,
        ctx: &OC_RelationshipPatternContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipDetail}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipDetail(
        &mut self,
        ctx: &OC_RelationshipDetailContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RecursiveRelationshipFilter}.
     * @param ctx the parse tree
     */
    fn visit_oC_RecursiveRelationshipFilter(
        &mut self,
        ctx: &OC_RecursiveRelationshipFilterContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Properties}.
     * @param ctx the parse tree
     */
    fn visit_oC_Properties(&mut self, ctx: &OC_PropertiesContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelationshipTypes}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelationshipTypes(
        &mut self,
        ctx: &OC_RelationshipTypesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodeLabels}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodeLabels(&mut self, ctx: &OC_NodeLabelsContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NodeLabel}.
     * @param ctx the parse tree
     */
    fn visit_oC_NodeLabel(&mut self, ctx: &OC_NodeLabelContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RangeLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_RangeLiteral(&mut self, ctx: &OC_RangeLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_LabelName}.
     * @param ctx the parse tree
     */
    fn visit_oC_LabelName(&mut self, ctx: &OC_LabelNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RelTypeName}.
     * @param ctx the parse tree
     */
    fn visit_oC_RelTypeName(&mut self, ctx: &OC_RelTypeNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyExpression(
        &mut self,
        ctx: &OC_PropertyExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Expression}.
     * @param ctx the parse tree
     */
    fn visit_oC_Expression(&mut self, ctx: &OC_ExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_OrExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_OrExpression(&mut self, ctx: &OC_OrExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_XorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_XorExpression(&mut self, ctx: &OC_XorExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AndExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_AndExpression(&mut self, ctx: &OC_AndExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NotExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NotExpression(&mut self, ctx: &OC_NotExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ComparisonExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ComparisonExpression(
        &mut self,
        ctx: &OC_ComparisonExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PartialComparisonExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PartialComparisonExpression(
        &mut self,
        ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StringListNullPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_StringListNullPredicateExpression(
        &mut self,
        ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_StringPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_StringPredicateExpression(
        &mut self,
        ctx: &OC_StringPredicateExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListPredicateExpression(
        &mut self,
        ctx: &OC_ListPredicateExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NullPredicateExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NullPredicateExpression(
        &mut self,
        ctx: &OC_NullPredicateExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_AddOrSubtractExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_AddOrSubtractExpression(
        &mut self,
        ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MultiplyDivideModuloExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_MultiplyDivideModuloExpression(
        &mut self,
        ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PowerOfExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_PowerOfExpression(
        &mut self,
        ctx: &OC_PowerOfExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_UnaryAddOrSubtractExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_UnaryAddOrSubtractExpression(
        &mut self,
        ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NonArithmeticOperatorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_NonArithmeticOperatorExpression(
        &mut self,
        ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListOperatorExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListOperatorExpression(
        &mut self,
        ctx: &OC_ListOperatorExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyLookup}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyLookup(&mut self, ctx: &OC_PropertyLookupContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Atom}.
     * @param ctx the parse tree
     */
    fn visit_oC_Atom(&mut self, ctx: &OC_AtomContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CaseExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_CaseExpression(&mut self, ctx: &OC_CaseExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CaseAlternative}.
     * @param ctx the parse tree
     */
    fn visit_oC_CaseAlternative(
        &mut self,
        ctx: &OC_CaseAlternativeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListComprehension}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListComprehension(
        &mut self,
        ctx: &OC_ListComprehensionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternComprehension}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternComprehension(
        &mut self,
        ctx: &OC_PatternComprehensionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Quantifier}.
     * @param ctx the parse tree
     */
    fn visit_oC_Quantifier(&mut self, ctx: &OC_QuantifierContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FilterExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_FilterExpression(
        &mut self,
        ctx: &OC_FilterExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PatternPredicate}.
     * @param ctx the parse tree
     */
    fn visit_oC_PatternPredicate(
        &mut self,
        ctx: &OC_PatternPredicateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ParenthesizedExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_ParenthesizedExpression(
        &mut self,
        ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_IdInColl}.
     * @param ctx the parse tree
     */
    fn visit_oC_IdInColl(&mut self, ctx: &OC_IdInCollContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FunctionInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_FunctionInvocation(
        &mut self,
        ctx: &OC_FunctionInvocationContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastExpression}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastExpression(&mut self, ctx: &OC_CastExpressionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastType}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastType(&mut self, ctx: &OC_CastTypeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeArgument}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeArgument(
        &mut self,
        ctx: &OC_CastTypeArgumentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeField}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeField(&mut self, ctx: &OC_CastTypeFieldContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_CastTypeName}.
     * @param ctx the parse tree
     */
    fn visit_oC_CastTypeName(&mut self, ctx: &OC_CastTypeNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_FunctionName}.
     * @param ctx the parse tree
     */
    fn visit_oC_FunctionName(&mut self, ctx: &OC_FunctionNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ExistentialSubquery}.
     * @param ctx the parse tree
     */
    fn visit_oC_ExistentialSubquery(
        &mut self,
        ctx: &OC_ExistentialSubqueryContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ExplicitProcedureInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_ExplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ImplicitProcedureInvocation}.
     * @param ctx the parse tree
     */
    fn visit_oC_ImplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProcedureResultField}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProcedureResultField(
        &mut self,
        ctx: &OC_ProcedureResultFieldContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ProcedureName}.
     * @param ctx the parse tree
     */
    fn visit_oC_ProcedureName(&mut self, ctx: &OC_ProcedureNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Namespace}.
     * @param ctx the parse tree
     */
    fn visit_oC_Namespace(&mut self, ctx: &OC_NamespaceContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Variable}.
     * @param ctx the parse tree
     */
    fn visit_oC_Variable(&mut self, ctx: &OC_VariableContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Literal}.
     * @param ctx the parse tree
     */
    fn visit_oC_Literal(&mut self, ctx: &OC_LiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_BooleanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_BooleanLiteral(&mut self, ctx: &OC_BooleanLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_NumberLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_NumberLiteral(&mut self, ctx: &OC_NumberLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_IntegerLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_IntegerLiteral(&mut self, ctx: &OC_IntegerLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_DoubleLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_DoubleLiteral(&mut self, ctx: &OC_DoubleLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ListLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_ListLiteral(&mut self, ctx: &OC_ListLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_MapLiteral}.
     * @param ctx the parse tree
     */
    fn visit_oC_MapLiteral(&mut self, ctx: &OC_MapLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_PropertyKeyName}.
     * @param ctx the parse tree
     */
    fn visit_oC_PropertyKeyName(
        &mut self,
        ctx: &OC_PropertyKeyNameContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Parameter}.
     * @param ctx the parse tree
     */
    fn visit_oC_Parameter(&mut self, ctx: &OC_ParameterContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SchemaName}.
     * @param ctx the parse tree
     */
    fn visit_oC_SchemaName(&mut self, ctx: &OC_SchemaNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_ReservedWord}.
     * @param ctx the parse tree
     */
    fn visit_oC_ReservedWord(&mut self, ctx: &OC_ReservedWordContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_SymbolicName}.
     * @param ctx the parse tree
     */
    fn visit_oC_SymbolicName(&mut self, ctx: &OC_SymbolicNameContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_LeftArrowHead}.
     * @param ctx the parse tree
     */
    fn visit_oC_LeftArrowHead(&mut self, ctx: &OC_LeftArrowHeadContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_RightArrowHead}.
     * @param ctx the parse tree
     */
    fn visit_oC_RightArrowHead(&mut self, ctx: &OC_RightArrowHeadContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link CypherParser#oC_Dash}.
     * @param ctx the parse tree
     */
    fn visit_oC_Dash(&mut self, ctx: &OC_DashContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }
}

impl<'input, T> CypherVisitor<'input> for T
where
    T: CypherVisitorCompat<'input>,
{
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Cypher(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Statement(&mut self, ctx: &OC_StatementContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Statement(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Query(&mut self, ctx: &OC_QueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Query(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RegularQuery(&mut self, ctx: &OC_RegularQueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RegularQuery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Union(&mut self, ctx: &OC_UnionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Union(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SingleQuery(&mut self, ctx: &OC_SingleQueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SingleQuery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SinglePartQuery(&mut self, ctx: &OC_SinglePartQueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SinglePartQuery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_MultiPartQuery(&mut self, ctx: &OC_MultiPartQueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_MultiPartQuery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_UpdatingClause(&mut self, ctx: &OC_UpdatingClauseContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_UpdatingClause(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ReadingClause(&mut self, ctx: &OC_ReadingClauseContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ReadingClause(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Match(&mut self, ctx: &OC_MatchContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Match(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Unwind(&mut self, ctx: &OC_UnwindContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Unwind(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Merge(&mut self, ctx: &OC_MergeContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Merge(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_MergeAction(&mut self, ctx: &OC_MergeActionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_MergeAction(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Create(&mut self, ctx: &OC_CreateContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Create(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Set(&mut self, ctx: &OC_SetContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Set(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SetItem(&mut self, ctx: &OC_SetItemContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SetItem(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Delete(&mut self, ctx: &OC_DeleteContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Delete(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Remove(&mut self, ctx: &OC_RemoveContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Remove(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RemoveItem(&mut self, ctx: &OC_RemoveItemContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RemoveItem(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_InQueryCall(&mut self, ctx: &OC_InQueryCallContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_InQueryCall(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_StandaloneCall(&mut self, ctx: &OC_StandaloneCallContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_StandaloneCall(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_YieldItems(&mut self, ctx: &OC_YieldItemsContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_YieldItems(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_YieldItem(&mut self, ctx: &OC_YieldItemContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_YieldItem(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_With(&mut self, ctx: &OC_WithContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_With(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Return(&mut self, ctx: &OC_ReturnContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Return(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ProjectionBody(&mut self, ctx: &OC_ProjectionBodyContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ProjectionBody(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ProjectionItems(&mut self, ctx: &OC_ProjectionItemsContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ProjectionItems(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ProjectionItem(&mut self, ctx: &OC_ProjectionItemContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ProjectionItem(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Order(&mut self, ctx: &OC_OrderContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Order(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Skip(&mut self, ctx: &OC_SkipContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Skip(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Limit(&mut self, ctx: &OC_LimitContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Limit(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SortItem(&mut self, ctx: &OC_SortItemContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SortItem(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Where(&mut self, ctx: &OC_WhereContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Where(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Pattern(&mut self, ctx: &OC_PatternContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Pattern(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PatternPart(&mut self, ctx: &OC_PatternPartContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PatternPart(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_AnonymousPatternPart(&mut self, ctx: &OC_AnonymousPatternPartContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_AnonymousPatternPart(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PatternElement(&mut self, ctx: &OC_PatternElementContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PatternElement(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RelationshipsPattern(&mut self, ctx: &OC_RelationshipsPatternContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RelationshipsPattern(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NodePattern(&mut self, ctx: &OC_NodePatternContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NodePattern(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PatternElementChain(&mut self, ctx: &OC_PatternElementChainContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PatternElementChain(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RelationshipPattern(&mut self, ctx: &OC_RelationshipPatternContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RelationshipPattern(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RelationshipDetail(&mut self, ctx: &OC_RelationshipDetailContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RelationshipDetail(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RecursiveRelationshipFilter(
        &mut self,
        ctx: &OC_RecursiveRelationshipFilterContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RecursiveRelationshipFilter(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Properties(&mut self, ctx: &OC_PropertiesContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Properties(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RelationshipTypes(&mut self, ctx: &OC_RelationshipTypesContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RelationshipTypes(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NodeLabels(&mut self, ctx: &OC_NodeLabelsContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NodeLabels(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NodeLabel(&mut self, ctx: &OC_NodeLabelContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NodeLabel(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RangeLiteral(&mut self, ctx: &OC_RangeLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RangeLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_LabelName(&mut self, ctx: &OC_LabelNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_LabelName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RelTypeName(&mut self, ctx: &OC_RelTypeNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RelTypeName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PropertyExpression(&mut self, ctx: &OC_PropertyExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PropertyExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Expression(&mut self, ctx: &OC_ExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Expression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_OrExpression(&mut self, ctx: &OC_OrExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_OrExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_XorExpression(&mut self, ctx: &OC_XorExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_XorExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_AndExpression(&mut self, ctx: &OC_AndExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_AndExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NotExpression(&mut self, ctx: &OC_NotExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NotExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ComparisonExpression(&mut self, ctx: &OC_ComparisonExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ComparisonExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PartialComparisonExpression(
        &mut self,
        ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PartialComparisonExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_StringListNullPredicateExpression(
        &mut self,
        ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) {
        let result =
            <Self as CypherVisitorCompat>::visit_oC_StringListNullPredicateExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_StringPredicateExpression(
        &mut self,
        ctx: &OC_StringPredicateExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_StringPredicateExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ListPredicateExpression(
        &mut self,
        ctx: &OC_ListPredicateExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ListPredicateExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NullPredicateExpression(
        &mut self,
        ctx: &OC_NullPredicateExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NullPredicateExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_AddOrSubtractExpression(
        &mut self,
        ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_AddOrSubtractExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_MultiplyDivideModuloExpression(
        &mut self,
        ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) {
        let result =
            <Self as CypherVisitorCompat>::visit_oC_MultiplyDivideModuloExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PowerOfExpression(&mut self, ctx: &OC_PowerOfExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PowerOfExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_UnaryAddOrSubtractExpression(
        &mut self,
        ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) {
        let result =
            <Self as CypherVisitorCompat>::visit_oC_UnaryAddOrSubtractExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NonArithmeticOperatorExpression(
        &mut self,
        ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) {
        let result =
            <Self as CypherVisitorCompat>::visit_oC_NonArithmeticOperatorExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ListOperatorExpression(&mut self, ctx: &OC_ListOperatorExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ListOperatorExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PropertyLookup(&mut self, ctx: &OC_PropertyLookupContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PropertyLookup(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Atom(&mut self, ctx: &OC_AtomContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Atom(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CaseExpression(&mut self, ctx: &OC_CaseExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CaseExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CaseAlternative(&mut self, ctx: &OC_CaseAlternativeContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CaseAlternative(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ListComprehension(&mut self, ctx: &OC_ListComprehensionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ListComprehension(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PatternComprehension(&mut self, ctx: &OC_PatternComprehensionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PatternComprehension(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Quantifier(&mut self, ctx: &OC_QuantifierContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Quantifier(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_FilterExpression(&mut self, ctx: &OC_FilterExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_FilterExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PatternPredicate(&mut self, ctx: &OC_PatternPredicateContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PatternPredicate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ParenthesizedExpression(
        &mut self,
        ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ParenthesizedExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_IdInColl(&mut self, ctx: &OC_IdInCollContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_IdInColl(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_FunctionInvocation(&mut self, ctx: &OC_FunctionInvocationContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_FunctionInvocation(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CastExpression(&mut self, ctx: &OC_CastExpressionContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CastExpression(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CastType(&mut self, ctx: &OC_CastTypeContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CastType(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CastTypeArgument(&mut self, ctx: &OC_CastTypeArgumentContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CastTypeArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CastTypeField(&mut self, ctx: &OC_CastTypeFieldContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CastTypeField(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_CastTypeName(&mut self, ctx: &OC_CastTypeNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_CastTypeName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_FunctionName(&mut self, ctx: &OC_FunctionNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_FunctionName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ExistentialSubquery(&mut self, ctx: &OC_ExistentialSubqueryContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ExistentialSubquery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ExplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ExplicitProcedureInvocation(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ImplicitProcedureInvocation(
        &mut self,
        ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ImplicitProcedureInvocation(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ProcedureResultField(&mut self, ctx: &OC_ProcedureResultFieldContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ProcedureResultField(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ProcedureName(&mut self, ctx: &OC_ProcedureNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ProcedureName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Namespace(&mut self, ctx: &OC_NamespaceContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Namespace(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Variable(&mut self, ctx: &OC_VariableContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Variable(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Literal(&mut self, ctx: &OC_LiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Literal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_BooleanLiteral(&mut self, ctx: &OC_BooleanLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_BooleanLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_NumberLiteral(&mut self, ctx: &OC_NumberLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_NumberLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_IntegerLiteral(&mut self, ctx: &OC_IntegerLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_IntegerLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_DoubleLiteral(&mut self, ctx: &OC_DoubleLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_DoubleLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ListLiteral(&mut self, ctx: &OC_ListLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ListLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_MapLiteral(&mut self, ctx: &OC_MapLiteralContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_MapLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_PropertyKeyName(&mut self, ctx: &OC_PropertyKeyNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_PropertyKeyName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Parameter(&mut self, ctx: &OC_ParameterContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Parameter(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SchemaName(&mut self, ctx: &OC_SchemaNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SchemaName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_ReservedWord(&mut self, ctx: &OC_ReservedWordContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_ReservedWord(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_SymbolicName(&mut self, ctx: &OC_SymbolicNameContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_SymbolicName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_LeftArrowHead(&mut self, ctx: &OC_LeftArrowHeadContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_LeftArrowHead(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_RightArrowHead(&mut self, ctx: &OC_RightArrowHeadContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_RightArrowHead(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_oC_Dash(&mut self, ctx: &OC_DashContext<'input>) {
        let result = <Self as CypherVisitorCompat>::visit_oC_Dash(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }
}
