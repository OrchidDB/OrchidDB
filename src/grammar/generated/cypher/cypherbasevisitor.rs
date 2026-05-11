// Generated from languages/cypher/Cypher.g4 by ANTLR 4.13.2

use super::cypherparser::*;
use antlr4rust::tree::ParseTreeVisitor;

// A complete Visitor for a parse tree produced by CypherParser.

pub trait CypherBaseVisitor<'input>: ParseTreeVisitor<'input, CypherParserContextType> {
    // Visit a parse tree produced by CypherParser#oC_Cypher.
    fn visit_oc_cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Statement.
    fn visit_oc_statement(&mut self, ctx: &OC_StatementContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Query.
    fn visit_oc_query(&mut self, ctx: &OC_QueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RegularQuery.
    fn visit_oc_regularquery(&mut self, ctx: &OC_RegularQueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Union.
    fn visit_oc_union(&mut self, ctx: &OC_UnionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SingleQuery.
    fn visit_oc_singlequery(&mut self, ctx: &OC_SingleQueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SinglePartQuery.
    fn visit_oc_singlepartquery(&mut self, ctx: &OC_SinglePartQueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_MultiPartQuery.
    fn visit_oc_multipartquery(&mut self, ctx: &OC_MultiPartQueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_UpdatingClause.
    fn visit_oc_updatingclause(&mut self, ctx: &OC_UpdatingClauseContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ReadingClause.
    fn visit_oc_readingclause(&mut self, ctx: &OC_ReadingClauseContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Match.
    fn visit_oc_match(&mut self, ctx: &OC_MatchContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Unwind.
    fn visit_oc_unwind(&mut self, ctx: &OC_UnwindContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Merge.
    fn visit_oc_merge(&mut self, ctx: &OC_MergeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_MergeAction.
    fn visit_oc_mergeaction(&mut self, ctx: &OC_MergeActionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Create.
    fn visit_oc_create(&mut self, ctx: &OC_CreateContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Set.
    fn visit_oc_set(&mut self, ctx: &OC_SetContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SetItem.
    fn visit_oc_setitem(&mut self, ctx: &OC_SetItemContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Delete.
    fn visit_oc_delete(&mut self, ctx: &OC_DeleteContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Remove.
    fn visit_oc_remove(&mut self, ctx: &OC_RemoveContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RemoveItem.
    fn visit_oc_removeitem(&mut self, ctx: &OC_RemoveItemContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_InQueryCall.
    fn visit_oc_inquerycall(&mut self, ctx: &OC_InQueryCallContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_StandaloneCall.
    fn visit_oc_standalonecall(&mut self, ctx: &OC_StandaloneCallContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_YieldItems.
    fn visit_oc_yielditems(&mut self, ctx: &OC_YieldItemsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_YieldItem.
    fn visit_oc_yielditem(&mut self, ctx: &OC_YieldItemContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_With.
    fn visit_oc_with(&mut self, ctx: &OC_WithContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Return.
    fn visit_oc_return(&mut self, ctx: &OC_ReturnContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ProjectionBody.
    fn visit_oc_projectionbody(&mut self, ctx: &OC_ProjectionBodyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ProjectionItems.
    fn visit_oc_projectionitems(&mut self, ctx: &OC_ProjectionItemsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ProjectionItem.
    fn visit_oc_projectionitem(&mut self, ctx: &OC_ProjectionItemContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Order.
    fn visit_oc_order(&mut self, ctx: &OC_OrderContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Skip.
    fn visit_oc_skip(&mut self, ctx: &OC_SkipContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Limit.
    fn visit_oc_limit(&mut self, ctx: &OC_LimitContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SortItem.
    fn visit_oc_sortitem(&mut self, ctx: &OC_SortItemContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Where.
    fn visit_oc_where(&mut self, ctx: &OC_WhereContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Pattern.
    fn visit_oc_pattern(&mut self, ctx: &OC_PatternContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PatternPart.
    fn visit_oc_patternpart(&mut self, ctx: &OC_PatternPartContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_AnonymousPatternPart.
    fn visit_oc_anonymouspatternpart(&mut self, ctx: &OC_AnonymousPatternPartContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PatternElement.
    fn visit_oc_patternelement(&mut self, ctx: &OC_PatternElementContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RelationshipsPattern.
    fn visit_oc_relationshipspattern(&mut self, ctx: &OC_RelationshipsPatternContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NodePattern.
    fn visit_oc_nodepattern(&mut self, ctx: &OC_NodePatternContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PatternElementChain.
    fn visit_oc_patternelementchain(&mut self, ctx: &OC_PatternElementChainContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RelationshipPattern.
    fn visit_oc_relationshippattern(&mut self, ctx: &OC_RelationshipPatternContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RelationshipDetail.
    fn visit_oc_relationshipdetail(&mut self, ctx: &OC_RelationshipDetailContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Properties.
    fn visit_oc_properties(&mut self, ctx: &OC_PropertiesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RelationshipTypes.
    fn visit_oc_relationshiptypes(&mut self, ctx: &OC_RelationshipTypesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NodeLabels.
    fn visit_oc_nodelabels(&mut self, ctx: &OC_NodeLabelsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NodeLabel.
    fn visit_oc_nodelabel(&mut self, ctx: &OC_NodeLabelContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RangeLiteral.
    fn visit_oc_rangeliteral(&mut self, ctx: &OC_RangeLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_LabelName.
    fn visit_oc_labelname(&mut self, ctx: &OC_LabelNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RelTypeName.
    fn visit_oc_reltypename(&mut self, ctx: &OC_RelTypeNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PropertyExpression.
    fn visit_oc_propertyexpression(&mut self, ctx: &OC_PropertyExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Expression.
    fn visit_oc_expression(&mut self, ctx: &OC_ExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_OrExpression.
    fn visit_oc_orexpression(&mut self, ctx: &OC_OrExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_XorExpression.
    fn visit_oc_xorexpression(&mut self, ctx: &OC_XorExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_AndExpression.
    fn visit_oc_andexpression(&mut self, ctx: &OC_AndExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NotExpression.
    fn visit_oc_notexpression(&mut self, ctx: &OC_NotExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ComparisonExpression.
    fn visit_oc_comparisonexpression(&mut self, ctx: &OC_ComparisonExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PartialComparisonExpression.
    fn visit_oc_partialcomparisonexpression(
        &mut self,
        ctx: &OC_PartialComparisonExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_StringListNullPredicateExpression.
    fn visit_oc_stringlistnullpredicateexpression(
        &mut self,
        ctx: &OC_StringListNullPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_StringPredicateExpression.
    fn visit_oc_stringpredicateexpression(
        &mut self,
        ctx: &OC_StringPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ListPredicateExpression.
    fn visit_oc_listpredicateexpression(
        &mut self,
        ctx: &OC_ListPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NullPredicateExpression.
    fn visit_oc_nullpredicateexpression(
        &mut self,
        ctx: &OC_NullPredicateExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_AddOrSubtractExpression.
    fn visit_oc_addorsubtractexpression(
        &mut self,
        ctx: &OC_AddOrSubtractExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_MultiplyDivideModuloExpression.
    fn visit_oc_multiplydividemoduloexpression(
        &mut self,
        ctx: &OC_MultiplyDivideModuloExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PowerOfExpression.
    fn visit_oc_powerofexpression(&mut self, ctx: &OC_PowerOfExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_UnaryAddOrSubtractExpression.
    fn visit_oc_unaryaddorsubtractexpression(
        &mut self,
        ctx: &OC_UnaryAddOrSubtractExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NonArithmeticOperatorExpression.
    fn visit_oc_nonarithmeticoperatorexpression(
        &mut self,
        ctx: &OC_NonArithmeticOperatorExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ListOperatorExpression.
    fn visit_oc_listoperatorexpression(&mut self, ctx: &OC_ListOperatorExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PropertyLookup.
    fn visit_oc_propertylookup(&mut self, ctx: &OC_PropertyLookupContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Atom.
    fn visit_oc_atom(&mut self, ctx: &OC_AtomContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_CaseExpression.
    fn visit_oc_caseexpression(&mut self, ctx: &OC_CaseExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_CaseAlternative.
    fn visit_oc_casealternative(&mut self, ctx: &OC_CaseAlternativeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ListComprehension.
    fn visit_oc_listcomprehension(&mut self, ctx: &OC_ListComprehensionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PatternComprehension.
    fn visit_oc_patterncomprehension(&mut self, ctx: &OC_PatternComprehensionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Quantifier.
    fn visit_oc_quantifier(&mut self, ctx: &OC_QuantifierContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_FilterExpression.
    fn visit_oc_filterexpression(&mut self, ctx: &OC_FilterExpressionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PatternPredicate.
    fn visit_oc_patternpredicate(&mut self, ctx: &OC_PatternPredicateContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ParenthesizedExpression.
    fn visit_oc_parenthesizedexpression(
        &mut self,
        ctx: &OC_ParenthesizedExpressionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_IdInColl.
    fn visit_oc_idincoll(&mut self, ctx: &OC_IdInCollContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_FunctionInvocation.
    fn visit_oc_functioninvocation(&mut self, ctx: &OC_FunctionInvocationContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_FunctionName.
    fn visit_oc_functionname(&mut self, ctx: &OC_FunctionNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ExistentialSubquery.
    fn visit_oc_existentialsubquery(&mut self, ctx: &OC_ExistentialSubqueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ExplicitProcedureInvocation.
    fn visit_oc_explicitprocedureinvocation(
        &mut self,
        ctx: &OC_ExplicitProcedureInvocationContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ImplicitProcedureInvocation.
    fn visit_oc_implicitprocedureinvocation(
        &mut self,
        ctx: &OC_ImplicitProcedureInvocationContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ProcedureResultField.
    fn visit_oc_procedureresultfield(&mut self, ctx: &OC_ProcedureResultFieldContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ProcedureName.
    fn visit_oc_procedurename(&mut self, ctx: &OC_ProcedureNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Namespace.
    fn visit_oc_namespace(&mut self, ctx: &OC_NamespaceContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Variable.
    fn visit_oc_variable(&mut self, ctx: &OC_VariableContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Literal.
    fn visit_oc_literal(&mut self, ctx: &OC_LiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_BooleanLiteral.
    fn visit_oc_booleanliteral(&mut self, ctx: &OC_BooleanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_NumberLiteral.
    fn visit_oc_numberliteral(&mut self, ctx: &OC_NumberLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_IntegerLiteral.
    fn visit_oc_integerliteral(&mut self, ctx: &OC_IntegerLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_DoubleLiteral.
    fn visit_oc_doubleliteral(&mut self, ctx: &OC_DoubleLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ListLiteral.
    fn visit_oc_listliteral(&mut self, ctx: &OC_ListLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_MapLiteral.
    fn visit_oc_mapliteral(&mut self, ctx: &OC_MapLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_PropertyKeyName.
    fn visit_oc_propertykeyname(&mut self, ctx: &OC_PropertyKeyNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Parameter.
    fn visit_oc_parameter(&mut self, ctx: &OC_ParameterContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SchemaName.
    fn visit_oc_schemaname(&mut self, ctx: &OC_SchemaNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_ReservedWord.
    fn visit_oc_reservedword(&mut self, ctx: &OC_ReservedWordContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_SymbolicName.
    fn visit_oc_symbolicname(&mut self, ctx: &OC_SymbolicNameContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_LeftArrowHead.
    fn visit_oc_leftarrowhead(&mut self, ctx: &OC_LeftArrowHeadContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_RightArrowHead.
    fn visit_oc_rightarrowhead(&mut self, ctx: &OC_RightArrowHeadContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by CypherParser#oC_Dash.
    fn visit_oc_dash(&mut self, ctx: &OC_DashContext<'input>) {
        self.visit_children(ctx)
    }
}
