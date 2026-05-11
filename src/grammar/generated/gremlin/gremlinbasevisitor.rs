// Generated from languages/gremlin/Gremlin.g4 by ANTLR 4.13.2

use super::gremlinparser::*;
use antlr4rust::tree::ParseTreeVisitor;

// A complete Visitor for a parse tree produced by GremlinParser.

pub trait GremlinBaseVisitor<'input>: ParseTreeVisitor<'input, GremlinParserContextType> {
    // Visit a parse tree produced by GremlinParser#queryList.
    fn visit_querylist(&mut self, ctx: &QueryListContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#query.
    fn visit_query(&mut self, ctx: &QueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#emptyQuery.
    fn visit_emptyquery(&mut self, ctx: &EmptyQueryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSource.
    fn visit_traversalsource(&mut self, ctx: &TraversalSourceContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#transactionPart.
    fn visit_transactionpart(&mut self, ctx: &TransactionPartContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#rootTraversal.
    fn visit_roottraversal(&mut self, ctx: &RootTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod.
    fn visit_traversalsourceselfmethod(&mut self, ctx: &TraversalSourceSelfMethodContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withBulk.
    fn visit_traversalsourceselfmethod_withbulk(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withPath.
    fn visit_traversalsourceselfmethod_withpath(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withSack.
    fn visit_traversalsourceselfmethod_withsack(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withSideEffect.
    fn visit_traversalsourceselfmethod_withsideeffect(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withStrategies.
    fn visit_traversalsourceselfmethod_withstrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_withoutStrategies.
    fn visit_traversalsourceselfmethod_withoutstrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSelfMethod_with.
    fn visit_traversalsourceselfmethod_with(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod.
    fn visit_traversalsourcespawnmethod(
        &mut self,
        ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_addE.
    fn visit_traversalsourcespawnmethod_adde(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_addV.
    fn visit_traversalsourcespawnmethod_addv(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_E.
    fn visit_traversalsourcespawnmethod_e(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_V.
    fn visit_traversalsourcespawnmethod_v(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_inject.
    fn visit_traversalsourcespawnmethod_inject(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_io.
    fn visit_traversalsourcespawnmethod_io(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_mergeV_Map.
    fn visit_traversalsourcespawnmethod_mergev_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_mergeV_Traversal.
    fn visit_traversalsourcespawnmethod_mergev_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_mergeE_Map.
    fn visit_traversalsourcespawnmethod_mergee_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_mergeE_Traversal.
    fn visit_traversalsourcespawnmethod_mergee_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_call_empty.
    fn visit_traversalsourcespawnmethod_call_empty(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_call_string.
    fn visit_traversalsourcespawnmethod_call_string(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_call_string_map.
    fn visit_traversalsourcespawnmethod_call_string_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_call_string_traversal.
    fn visit_traversalsourcespawnmethod_call_string_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_call_string_map_traversal.
    fn visit_traversalsourcespawnmethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSourceSpawnMethod_union.
    fn visit_traversalsourcespawnmethod_union(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#chainedTraversal.
    fn visit_chainedtraversal(&mut self, ctx: &ChainedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nestedTraversal.
    fn visit_nestedtraversal(&mut self, ctx: &NestedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#terminatedTraversal.
    fn visit_terminatedtraversal(&mut self, ctx: &TerminatedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod.
    fn visit_traversalmethod(&mut self, ctx: &TraversalMethodContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_V.
    fn visit_traversalmethod_v(&mut self, ctx: &TraversalMethod_VContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_E.
    fn visit_traversalmethod_e(&mut self, ctx: &TraversalMethod_EContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_addE_String.
    fn visit_traversalmethod_adde_string(
        &mut self,
        ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_addE_Traversal.
    fn visit_traversalmethod_adde_traversal(
        &mut self,
        ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_addV_Empty.
    fn visit_traversalmethod_addv_empty(
        &mut self,
        ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_addV_String.
    fn visit_traversalmethod_addv_string(
        &mut self,
        ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_addV_Traversal.
    fn visit_traversalmethod_addv_traversal(
        &mut self,
        ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_aggregate_String.
    fn visit_traversalmethod_aggregate_string(
        &mut self,
        ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_all_P.
    fn visit_traversalmethod_all_p(&mut self, ctx: &TraversalMethod_all_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_and.
    fn visit_traversalmethod_and(&mut self, ctx: &TraversalMethod_andContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_any_P.
    fn visit_traversalmethod_any_p(&mut self, ctx: &TraversalMethod_any_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_as.
    fn visit_traversalmethod_as(&mut self, ctx: &TraversalMethod_asContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asBool.
    fn visit_traversalmethod_asbool(&mut self, ctx: &TraversalMethod_asBoolContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asDate.
    fn visit_traversalmethod_asdate(&mut self, ctx: &TraversalMethod_asDateContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asNumber_Empty.
    fn visit_traversalmethod_asnumber_empty(
        &mut self,
        ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asNumber_traversalGType.
    fn visit_traversalmethod_asnumber_traversalgtype(
        &mut self,
        ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asString_Empty.
    fn visit_traversalmethod_asstring_empty(
        &mut self,
        ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_asString_Scope.
    fn visit_traversalmethod_asstring_scope(
        &mut self,
        ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_barrier_Consumer.
    fn visit_traversalmethod_barrier_consumer(
        &mut self,
        ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_barrier_Empty.
    fn visit_traversalmethod_barrier_empty(
        &mut self,
        ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_barrier_int.
    fn visit_traversalmethod_barrier_int(
        &mut self,
        ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_both.
    fn visit_traversalmethod_both(&mut self, ctx: &TraversalMethod_bothContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_bothE.
    fn visit_traversalmethod_bothe(&mut self, ctx: &TraversalMethod_bothEContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_bothV.
    fn visit_traversalmethod_bothv(&mut self, ctx: &TraversalMethod_bothVContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_branch.
    fn visit_traversalmethod_branch(&mut self, ctx: &TraversalMethod_branchContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Comparator.
    fn visit_traversalmethod_by_comparator(
        &mut self,
        ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Empty.
    fn visit_traversalmethod_by_empty(&mut self, ctx: &TraversalMethod_by_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Function.
    fn visit_traversalmethod_by_function(
        &mut self,
        ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Function_Comparator.
    fn visit_traversalmethod_by_function_comparator(
        &mut self,
        ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Order.
    fn visit_traversalmethod_by_order(&mut self, ctx: &TraversalMethod_by_OrderContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_String.
    fn visit_traversalmethod_by_string(&mut self, ctx: &TraversalMethod_by_StringContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_String_Comparator.
    fn visit_traversalmethod_by_string_comparator(
        &mut self,
        ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_T.
    fn visit_traversalmethod_by_t(&mut self, ctx: &TraversalMethod_by_TContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Traversal.
    fn visit_traversalmethod_by_traversal(
        &mut self,
        ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_by_Traversal_Comparator.
    fn visit_traversalmethod_by_traversal_comparator(
        &mut self,
        ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_call_string.
    fn visit_traversalmethod_call_string(
        &mut self,
        ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_call_string_map.
    fn visit_traversalmethod_call_string_map(
        &mut self,
        ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_call_string_traversal.
    fn visit_traversalmethod_call_string_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_call_string_map_traversal.
    fn visit_traversalmethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_cap.
    fn visit_traversalmethod_cap(&mut self, ctx: &TraversalMethod_capContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Function.
    fn visit_traversalmethod_choose_function(
        &mut self,
        ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Predicate_Traversal.
    fn visit_traversalmethod_choose_predicate_traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Predicate_Traversal_Traversal.
    fn visit_traversalmethod_choose_predicate_traversal_traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Traversal.
    fn visit_traversalmethod_choose_traversal(
        &mut self,
        ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Traversal_Traversal.
    fn visit_traversalmethod_choose_traversal_traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_choose_Traversal_Traversal_Traversal.
    fn visit_traversalmethod_choose_traversal_traversal_traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_coalesce.
    fn visit_traversalmethod_coalesce(&mut self, ctx: &TraversalMethod_coalesceContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_coin.
    fn visit_traversalmethod_coin(&mut self, ctx: &TraversalMethod_coinContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_combine_Object.
    fn visit_traversalmethod_combine_object(
        &mut self,
        ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_concat_Traversal_Traversal.
    fn visit_traversalmethod_concat_traversal_traversal(
        &mut self,
        ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_concat_String.
    fn visit_traversalmethod_concat_string(
        &mut self,
        ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_conjoin_String.
    fn visit_traversalmethod_conjoin_string(
        &mut self,
        ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_connectedComponent.
    fn visit_traversalmethod_connectedcomponent(
        &mut self,
        ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_constant.
    fn visit_traversalmethod_constant(&mut self, ctx: &TraversalMethod_constantContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_count_Empty.
    fn visit_traversalmethod_count_empty(
        &mut self,
        ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_count_Scope.
    fn visit_traversalmethod_count_scope(
        &mut self,
        ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_cyclicPath.
    fn visit_traversalmethod_cyclicpath(
        &mut self,
        ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_dateAdd.
    fn visit_traversalmethod_dateadd(&mut self, ctx: &TraversalMethod_dateAddContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_dateDiff_Traversal.
    fn visit_traversalmethod_datediff_traversal(
        &mut self,
        ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_dateDiff_Date.
    fn visit_traversalmethod_datediff_date(
        &mut self,
        ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_dedup_Scope_String.
    fn visit_traversalmethod_dedup_scope_string(
        &mut self,
        ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_dedup_String.
    fn visit_traversalmethod_dedup_string(
        &mut self,
        ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_difference_Object.
    fn visit_traversalmethod_difference_object(
        &mut self,
        ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_discard.
    fn visit_traversalmethod_discard(&mut self, ctx: &TraversalMethod_discardContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_disjunct_Object.
    fn visit_traversalmethod_disjunct_object(
        &mut self,
        ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_drop.
    fn visit_traversalmethod_drop(&mut self, ctx: &TraversalMethod_dropContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_element.
    fn visit_traversalmethod_element(&mut self, ctx: &TraversalMethod_elementContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_elementMap.
    fn visit_traversalmethod_elementmap(
        &mut self,
        ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_emit_Empty.
    fn visit_traversalmethod_emit_empty(
        &mut self,
        ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_emit_Predicate.
    fn visit_traversalmethod_emit_predicate(
        &mut self,
        ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_emit_Traversal.
    fn visit_traversalmethod_emit_traversal(
        &mut self,
        ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_fail_Empty.
    fn visit_traversalmethod_fail_empty(
        &mut self,
        ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_fail_String.
    fn visit_traversalmethod_fail_string(
        &mut self,
        ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_filter_Predicate.
    fn visit_traversalmethod_filter_predicate(
        &mut self,
        ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_filter_Traversal.
    fn visit_traversalmethod_filter_traversal(
        &mut self,
        ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_flatMap.
    fn visit_traversalmethod_flatmap(&mut self, ctx: &TraversalMethod_flatMapContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_fold_Empty.
    fn visit_traversalmethod_fold_empty(
        &mut self,
        ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_fold_Object_BiFunction.
    fn visit_traversalmethod_fold_object_bifunction(
        &mut self,
        ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_format_String.
    fn visit_traversalmethod_format_string(
        &mut self,
        ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_from_String.
    fn visit_traversalmethod_from_string(
        &mut self,
        ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_from_Traversal.
    fn visit_traversalmethod_from_traversal(
        &mut self,
        ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_group_Empty.
    fn visit_traversalmethod_group_empty(
        &mut self,
        ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_group_String.
    fn visit_traversalmethod_group_string(
        &mut self,
        ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_groupCount_Empty.
    fn visit_traversalmethod_groupcount_empty(
        &mut self,
        ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_groupCount_String.
    fn visit_traversalmethod_groupcount_string(
        &mut self,
        ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_String.
    fn visit_traversalmethod_has_string(
        &mut self,
        ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_String_Object.
    fn visit_traversalmethod_has_string_object(
        &mut self,
        ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_String_P.
    fn visit_traversalmethod_has_string_p(
        &mut self,
        ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_String_String_Object.
    fn visit_traversalmethod_has_string_string_object(
        &mut self,
        ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_String_String_P.
    fn visit_traversalmethod_has_string_string_p(
        &mut self,
        ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_T_Object.
    fn visit_traversalmethod_has_t_object(
        &mut self,
        ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_has_T_P.
    fn visit_traversalmethod_has_t_p(&mut self, ctx: &TraversalMethod_has_T_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasId_Object_Object.
    fn visit_traversalmethod_hasid_object_object(
        &mut self,
        ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasId_P.
    fn visit_traversalmethod_hasid_p(&mut self, ctx: &TraversalMethod_hasId_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasKey_P.
    fn visit_traversalmethod_haskey_p(&mut self, ctx: &TraversalMethod_hasKey_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasKey_String_String.
    fn visit_traversalmethod_haskey_string_string(
        &mut self,
        ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasLabel_P.
    fn visit_traversalmethod_haslabel_p(
        &mut self,
        ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasLabel_String_String.
    fn visit_traversalmethod_haslabel_string_string(
        &mut self,
        ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasNot.
    fn visit_traversalmethod_hasnot(&mut self, ctx: &TraversalMethod_hasNotContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasValue_Object_Object.
    fn visit_traversalmethod_hasvalue_object_object(
        &mut self,
        ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_hasValue_P.
    fn visit_traversalmethod_hasvalue_p(
        &mut self,
        ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_id.
    fn visit_traversalmethod_id(&mut self, ctx: &TraversalMethod_idContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_identity.
    fn visit_traversalmethod_identity(&mut self, ctx: &TraversalMethod_identityContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_in.
    fn visit_traversalmethod_in(&mut self, ctx: &TraversalMethod_inContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_inE.
    fn visit_traversalmethod_ine(&mut self, ctx: &TraversalMethod_inEContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_intersect_Object.
    fn visit_traversalmethod_intersect_object(
        &mut self,
        ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_inV.
    fn visit_traversalmethod_inv(&mut self, ctx: &TraversalMethod_inVContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_index.
    fn visit_traversalmethod_index(&mut self, ctx: &TraversalMethod_indexContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_inject.
    fn visit_traversalmethod_inject(&mut self, ctx: &TraversalMethod_injectContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_is_Object.
    fn visit_traversalmethod_is_object(&mut self, ctx: &TraversalMethod_is_ObjectContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_is_P.
    fn visit_traversalmethod_is_p(&mut self, ctx: &TraversalMethod_is_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_key.
    fn visit_traversalmethod_key(&mut self, ctx: &TraversalMethod_keyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_label.
    fn visit_traversalmethod_label(&mut self, ctx: &TraversalMethod_labelContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_length_Empty.
    fn visit_traversalmethod_length_empty(
        &mut self,
        ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_length_Scope.
    fn visit_traversalmethod_length_scope(
        &mut self,
        ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_limit_Scope_long.
    fn visit_traversalmethod_limit_scope_long(
        &mut self,
        ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_limit_long.
    fn visit_traversalmethod_limit_long(
        &mut self,
        ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_local.
    fn visit_traversalmethod_local(&mut self, ctx: &TraversalMethod_localContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_loops_Empty.
    fn visit_traversalmethod_loops_empty(
        &mut self,
        ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_loops_String.
    fn visit_traversalmethod_loops_string(
        &mut self,
        ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_lTrim_Empty.
    fn visit_traversalmethod_ltrim_empty(
        &mut self,
        ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_lTrim_Scope.
    fn visit_traversalmethod_ltrim_scope(
        &mut self,
        ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_map.
    fn visit_traversalmethod_map(&mut self, ctx: &TraversalMethod_mapContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_match.
    fn visit_traversalmethod_match(&mut self, ctx: &TraversalMethod_matchContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_math.
    fn visit_traversalmethod_math(&mut self, ctx: &TraversalMethod_mathContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_max_Empty.
    fn visit_traversalmethod_max_empty(&mut self, ctx: &TraversalMethod_max_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_max_Scope.
    fn visit_traversalmethod_max_scope(&mut self, ctx: &TraversalMethod_max_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mean_Empty.
    fn visit_traversalmethod_mean_empty(
        &mut self,
        ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mean_Scope.
    fn visit_traversalmethod_mean_scope(
        &mut self,
        ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_merge_Object.
    fn visit_traversalmethod_merge_object(
        &mut self,
        ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeV_empty.
    fn visit_traversalmethod_mergev_empty(
        &mut self,
        ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeV_Map.
    fn visit_traversalmethod_mergev_map(
        &mut self,
        ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeV_Traversal.
    fn visit_traversalmethod_mergev_traversal(
        &mut self,
        ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeE_empty.
    fn visit_traversalmethod_mergee_empty(
        &mut self,
        ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeE_Map.
    fn visit_traversalmethod_mergee_map(
        &mut self,
        ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_mergeE_Traversal.
    fn visit_traversalmethod_mergee_traversal(
        &mut self,
        ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_min_Empty.
    fn visit_traversalmethod_min_empty(&mut self, ctx: &TraversalMethod_min_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_min_Scope.
    fn visit_traversalmethod_min_scope(&mut self, ctx: &TraversalMethod_min_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_none_P.
    fn visit_traversalmethod_none_p(&mut self, ctx: &TraversalMethod_none_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_not.
    fn visit_traversalmethod_not(&mut self, ctx: &TraversalMethod_notContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Predicate_Traversal.
    fn visit_traversalmethod_option_predicate_traversal(
        &mut self,
        ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Merge_Map.
    fn visit_traversalmethod_option_merge_map(
        &mut self,
        ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Merge_Map_Cardinality.
    fn visit_traversalmethod_option_merge_map_cardinality(
        &mut self,
        ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Merge_Traversal.
    fn visit_traversalmethod_option_merge_traversal(
        &mut self,
        ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Object_Traversal.
    fn visit_traversalmethod_option_object_traversal(
        &mut self,
        ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_option_Traversal.
    fn visit_traversalmethod_option_traversal(
        &mut self,
        ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_optional.
    fn visit_traversalmethod_optional(&mut self, ctx: &TraversalMethod_optionalContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_or.
    fn visit_traversalmethod_or(&mut self, ctx: &TraversalMethod_orContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_order_Empty.
    fn visit_traversalmethod_order_empty(
        &mut self,
        ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_order_Scope.
    fn visit_traversalmethod_order_scope(
        &mut self,
        ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_otherV.
    fn visit_traversalmethod_otherv(&mut self, ctx: &TraversalMethod_otherVContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_out.
    fn visit_traversalmethod_out(&mut self, ctx: &TraversalMethod_outContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_outE.
    fn visit_traversalmethod_oute(&mut self, ctx: &TraversalMethod_outEContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_outV.
    fn visit_traversalmethod_outv(&mut self, ctx: &TraversalMethod_outVContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_pageRank_Empty.
    fn visit_traversalmethod_pagerank_empty(
        &mut self,
        ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_pageRank_double.
    fn visit_traversalmethod_pagerank_double(
        &mut self,
        ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_path.
    fn visit_traversalmethod_path(&mut self, ctx: &TraversalMethod_pathContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_peerPressure.
    fn visit_traversalmethod_peerpressure(
        &mut self,
        ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_product_Object.
    fn visit_traversalmethod_product_object(
        &mut self,
        ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_profile_Empty.
    fn visit_traversalmethod_profile_empty(
        &mut self,
        ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_profile_String.
    fn visit_traversalmethod_profile_string(
        &mut self,
        ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_project.
    fn visit_traversalmethod_project(&mut self, ctx: &TraversalMethod_projectContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_properties.
    fn visit_traversalmethod_properties(
        &mut self,
        ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_property_Cardinality_Object_Object_Object.
    fn visit_traversalmethod_property_cardinality_object_object_object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_property_Cardinality_Object.
    fn visit_traversalmethod_property_cardinality_object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_property_Object_Object_Object.
    fn visit_traversalmethod_property_object_object_object(
        &mut self,
        ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_property_Object.
    fn visit_traversalmethod_property_object(
        &mut self,
        ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_propertyMap.
    fn visit_traversalmethod_propertymap(
        &mut self,
        ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_range_Scope_long_long.
    fn visit_traversalmethod_range_scope_long_long(
        &mut self,
        ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_range_long_long.
    fn visit_traversalmethod_range_long_long(
        &mut self,
        ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_read.
    fn visit_traversalmethod_read(&mut self, ctx: &TraversalMethod_readContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_repeat_String_Traversal.
    fn visit_traversalmethod_repeat_string_traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_repeat_Traversal.
    fn visit_traversalmethod_repeat_traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_replace_String_String.
    fn visit_traversalmethod_replace_string_string(
        &mut self,
        ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_replace_Scope_String_String.
    fn visit_traversalmethod_replace_scope_string_string(
        &mut self,
        ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_reverse_Empty.
    fn visit_traversalmethod_reverse_empty(
        &mut self,
        ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_rTrim_Empty.
    fn visit_traversalmethod_rtrim_empty(
        &mut self,
        ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_rTrim_Scope.
    fn visit_traversalmethod_rtrim_scope(
        &mut self,
        ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sack_BiFunction.
    fn visit_traversalmethod_sack_bifunction(
        &mut self,
        ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sack_Empty.
    fn visit_traversalmethod_sack_empty(
        &mut self,
        ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sample_Scope_int.
    fn visit_traversalmethod_sample_scope_int(
        &mut self,
        ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sample_int.
    fn visit_traversalmethod_sample_int(
        &mut self,
        ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_Column.
    fn visit_traversalmethod_select_column(
        &mut self,
        ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_Pop_String.
    fn visit_traversalmethod_select_pop_string(
        &mut self,
        ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_Pop_String_String_String.
    fn visit_traversalmethod_select_pop_string_string_string(
        &mut self,
        ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_Pop_Traversal.
    fn visit_traversalmethod_select_pop_traversal(
        &mut self,
        ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_String.
    fn visit_traversalmethod_select_string(
        &mut self,
        ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_String_String_String.
    fn visit_traversalmethod_select_string_string_string(
        &mut self,
        ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_select_Traversal.
    fn visit_traversalmethod_select_traversal(
        &mut self,
        ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_shortestPath.
    fn visit_traversalmethod_shortestpath(
        &mut self,
        ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sideEffect.
    fn visit_traversalmethod_sideeffect(
        &mut self,
        ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_simplePath.
    fn visit_traversalmethod_simplepath(
        &mut self,
        ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_skip_Scope_long.
    fn visit_traversalmethod_skip_scope_long(
        &mut self,
        ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_skip_long.
    fn visit_traversalmethod_skip_long(&mut self, ctx: &TraversalMethod_skip_longContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_split_String.
    fn visit_traversalmethod_split_string(
        &mut self,
        ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_split_Scope_String.
    fn visit_traversalmethod_split_scope_string(
        &mut self,
        ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_subgraph.
    fn visit_traversalmethod_subgraph(&mut self, ctx: &TraversalMethod_subgraphContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_substring_int.
    fn visit_traversalmethod_substring_int(
        &mut self,
        ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_substring_Scope_int.
    fn visit_traversalmethod_substring_scope_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_substring_int_int.
    fn visit_traversalmethod_substring_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_substring_Scope_int_int.
    fn visit_traversalmethod_substring_scope_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sum_Empty.
    fn visit_traversalmethod_sum_empty(&mut self, ctx: &TraversalMethod_sum_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_sum_Scope.
    fn visit_traversalmethod_sum_scope(&mut self, ctx: &TraversalMethod_sum_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tail_Empty.
    fn visit_traversalmethod_tail_empty(
        &mut self,
        ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tail_Scope.
    fn visit_traversalmethod_tail_scope(
        &mut self,
        ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tail_Scope_long.
    fn visit_traversalmethod_tail_scope_long(
        &mut self,
        ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tail_long.
    fn visit_traversalmethod_tail_long(&mut self, ctx: &TraversalMethod_tail_longContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_timeLimit.
    fn visit_traversalmethod_timelimit(&mut self, ctx: &TraversalMethod_timeLimitContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_times.
    fn visit_traversalmethod_times(&mut self, ctx: &TraversalMethod_timesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_to_Direction_String.
    fn visit_traversalmethod_to_direction_string(
        &mut self,
        ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_to_String.
    fn visit_traversalmethod_to_string(&mut self, ctx: &TraversalMethod_to_StringContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_to_Traversal.
    fn visit_traversalmethod_to_traversal(
        &mut self,
        ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toE.
    fn visit_traversalmethod_toe(&mut self, ctx: &TraversalMethod_toEContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toLower_Empty.
    fn visit_traversalmethod_tolower_empty(
        &mut self,
        ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toLower_Scope.
    fn visit_traversalmethod_tolower_scope(
        &mut self,
        ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toUpper_Empty.
    fn visit_traversalmethod_toupper_empty(
        &mut self,
        ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toUpper_Scope.
    fn visit_traversalmethod_toupper_scope(
        &mut self,
        ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_toV.
    fn visit_traversalmethod_tov(&mut self, ctx: &TraversalMethod_toVContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tree_Empty.
    fn visit_traversalmethod_tree_empty(
        &mut self,
        ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_tree_String.
    fn visit_traversalmethod_tree_string(
        &mut self,
        ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_trim_Empty.
    fn visit_traversalmethod_trim_empty(
        &mut self,
        ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_trim_Scope.
    fn visit_traversalmethod_trim_scope(
        &mut self,
        ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_unfold.
    fn visit_traversalmethod_unfold(&mut self, ctx: &TraversalMethod_unfoldContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_union.
    fn visit_traversalmethod_union(&mut self, ctx: &TraversalMethod_unionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_until_Predicate.
    fn visit_traversalmethod_until_predicate(
        &mut self,
        ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_until_Traversal.
    fn visit_traversalmethod_until_traversal(
        &mut self,
        ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_value.
    fn visit_traversalmethod_value(&mut self, ctx: &TraversalMethod_valueContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_valueMap_String.
    fn visit_traversalmethod_valuemap_string(
        &mut self,
        ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_valueMap_boolean_String.
    fn visit_traversalmethod_valuemap_boolean_string(
        &mut self,
        ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_values.
    fn visit_traversalmethod_values(&mut self, ctx: &TraversalMethod_valuesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_where_P.
    fn visit_traversalmethod_where_p(&mut self, ctx: &TraversalMethod_where_PContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_where_String_P.
    fn visit_traversalmethod_where_string_p(
        &mut self,
        ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_where_Traversal.
    fn visit_traversalmethod_where_traversal(
        &mut self,
        ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_with_String.
    fn visit_traversalmethod_with_string(
        &mut self,
        ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_with_String_Object.
    fn visit_traversalmethod_with_string_object(
        &mut self,
        ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMethod_write.
    fn visit_traversalmethod_write(&mut self, ctx: &TraversalMethod_writeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalStrategy.
    fn visit_traversalstrategy(&mut self, ctx: &TraversalStrategyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#configuration.
    fn visit_configuration(&mut self, ctx: &ConfigurationContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalScope.
    fn visit_traversalscope(&mut self, ctx: &TraversalScopeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalBarrier.
    fn visit_traversalbarrier(&mut self, ctx: &TraversalBarrierContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalT.
    fn visit_traversalt(&mut self, ctx: &TraversalTContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTShort.
    fn visit_traversaltshort(&mut self, ctx: &TraversalTShortContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTLong.
    fn visit_traversaltlong(&mut self, ctx: &TraversalTLongContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalMerge.
    fn visit_traversalmerge(&mut self, ctx: &TraversalMergeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalOrder.
    fn visit_traversalorder(&mut self, ctx: &TraversalOrderContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalDirection.
    fn visit_traversaldirection(&mut self, ctx: &TraversalDirectionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalDirectionShort.
    fn visit_traversaldirectionshort(&mut self, ctx: &TraversalDirectionShortContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalDirectionLong.
    fn visit_traversaldirectionlong(&mut self, ctx: &TraversalDirectionLongContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalCardinality.
    fn visit_traversalcardinality(&mut self, ctx: &TraversalCardinalityContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalColumn.
    fn visit_traversalcolumn(&mut self, ctx: &TraversalColumnContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPop.
    fn visit_traversalpop(&mut self, ctx: &TraversalPopContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalOperator.
    fn visit_traversaloperator(&mut self, ctx: &TraversalOperatorContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPick.
    fn visit_traversalpick(&mut self, ctx: &TraversalPickContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalDT.
    fn visit_traversaldt(&mut self, ctx: &TraversalDTContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalGType.
    fn visit_traversalgtype(&mut self, ctx: &TraversalGTypeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate.
    fn visit_traversalpredicate(&mut self, ctx: &TraversalPredicateContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod.
    fn visit_traversalterminalmethod(&mut self, ctx: &TraversalTerminalMethodContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalSackMethod.
    fn visit_traversalsackmethod(&mut self, ctx: &TraversalSackMethodContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalComparator.
    fn visit_traversalcomparator(&mut self, ctx: &TraversalComparatorContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalFunction.
    fn visit_traversalfunction(&mut self, ctx: &TraversalFunctionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalBiFunction.
    fn visit_traversalbifunction(&mut self, ctx: &TraversalBiFunctionContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_eq.
    fn visit_traversalpredicate_eq(&mut self, ctx: &TraversalPredicate_eqContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_neq.
    fn visit_traversalpredicate_neq(&mut self, ctx: &TraversalPredicate_neqContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_typeOf.
    fn visit_traversalpredicate_typeof(&mut self, ctx: &TraversalPredicate_typeOfContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_lt.
    fn visit_traversalpredicate_lt(&mut self, ctx: &TraversalPredicate_ltContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_lte.
    fn visit_traversalpredicate_lte(&mut self, ctx: &TraversalPredicate_lteContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_gt.
    fn visit_traversalpredicate_gt(&mut self, ctx: &TraversalPredicate_gtContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_gte.
    fn visit_traversalpredicate_gte(&mut self, ctx: &TraversalPredicate_gteContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_inside.
    fn visit_traversalpredicate_inside(&mut self, ctx: &TraversalPredicate_insideContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_outside.
    fn visit_traversalpredicate_outside(
        &mut self,
        ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_between.
    fn visit_traversalpredicate_between(
        &mut self,
        ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_within.
    fn visit_traversalpredicate_within(&mut self, ctx: &TraversalPredicate_withinContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_without.
    fn visit_traversalpredicate_without(
        &mut self,
        ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_not.
    fn visit_traversalpredicate_not(&mut self, ctx: &TraversalPredicate_notContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_containing.
    fn visit_traversalpredicate_containing(
        &mut self,
        ctx: &TraversalPredicate_containingContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_notContaining.
    fn visit_traversalpredicate_notcontaining(
        &mut self,
        ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_startingWith.
    fn visit_traversalpredicate_startingwith(
        &mut self,
        ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_notStartingWith.
    fn visit_traversalpredicate_notstartingwith(
        &mut self,
        ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_endingWith.
    fn visit_traversalpredicate_endingwith(
        &mut self,
        ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_notEndingWith.
    fn visit_traversalpredicate_notendingwith(
        &mut self,
        ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_regex.
    fn visit_traversalpredicate_regex(&mut self, ctx: &TraversalPredicate_regexContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalPredicate_notRegex.
    fn visit_traversalpredicate_notregex(
        &mut self,
        ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_explain.
    fn visit_traversalterminalmethod_explain(
        &mut self,
        ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_hasNext.
    fn visit_traversalterminalmethod_hasnext(
        &mut self,
        ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_iterate.
    fn visit_traversalterminalmethod_iterate(
        &mut self,
        ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_tryNext.
    fn visit_traversalterminalmethod_trynext(
        &mut self,
        ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_next.
    fn visit_traversalterminalmethod_next(
        &mut self,
        ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_toList.
    fn visit_traversalterminalmethod_tolist(
        &mut self,
        ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_toSet.
    fn visit_traversalterminalmethod_toset(
        &mut self,
        ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalTerminalMethod_toBulkSet.
    fn visit_traversalterminalmethod_tobulkset(
        &mut self,
        ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionKeys.
    fn visit_withoptionkeys(&mut self, ctx: &WithOptionKeysContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#connectedComponentConstants.
    fn visit_connectedcomponentconstants(
        &mut self,
        ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#pageRankConstants.
    fn visit_pagerankconstants(&mut self, ctx: &PageRankConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#peerPressureConstants.
    fn visit_peerpressureconstants(&mut self, ctx: &PeerPressureConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants.
    fn visit_shortestpathconstants(&mut self, ctx: &ShortestPathConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsValues.
    fn visit_withoptionsvalues(&mut self, ctx: &WithOptionsValuesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsKeys.
    fn visit_iooptionskeys(&mut self, ctx: &IoOptionsKeysContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsValues.
    fn visit_iooptionsvalues(&mut self, ctx: &IoOptionsValuesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#connectedComponentConstants_component.
    fn visit_connectedcomponentconstants_component(
        &mut self,
        ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#connectedComponentConstants_edges.
    fn visit_connectedcomponentconstants_edges(
        &mut self,
        ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#connectedComponentConstants_propertyName.
    fn visit_connectedcomponentconstants_propertyname(
        &mut self,
        ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#pageRankConstants_edges.
    fn visit_pagerankconstants_edges(&mut self, ctx: &PageRankConstants_edgesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#pageRankConstants_times.
    fn visit_pagerankconstants_times(&mut self, ctx: &PageRankConstants_timesContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#pageRankConstants_propertyName.
    fn visit_pagerankconstants_propertyname(
        &mut self,
        ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#peerPressureConstants_edges.
    fn visit_peerpressureconstants_edges(
        &mut self,
        ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#peerPressureConstants_times.
    fn visit_peerpressureconstants_times(
        &mut self,
        ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#peerPressureConstants_propertyName.
    fn visit_peerpressureconstants_propertyname(
        &mut self,
        ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants_target.
    fn visit_shortestpathconstants_target(
        &mut self,
        ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants_edges.
    fn visit_shortestpathconstants_edges(
        &mut self,
        ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants_distance.
    fn visit_shortestpathconstants_distance(
        &mut self,
        ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants_maxDistance.
    fn visit_shortestpathconstants_maxdistance(
        &mut self,
        ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathConstants_includeEdges.
    fn visit_shortestpathconstants_includeedges(
        &mut self,
        ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_tokens.
    fn visit_withoptionsconstants_tokens(
        &mut self,
        ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_none.
    fn visit_withoptionsconstants_none(&mut self, ctx: &WithOptionsConstants_noneContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_ids.
    fn visit_withoptionsconstants_ids(&mut self, ctx: &WithOptionsConstants_idsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_labels.
    fn visit_withoptionsconstants_labels(
        &mut self,
        ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_keys.
    fn visit_withoptionsconstants_keys(&mut self, ctx: &WithOptionsConstants_keysContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_values.
    fn visit_withoptionsconstants_values(
        &mut self,
        ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_all.
    fn visit_withoptionsconstants_all(&mut self, ctx: &WithOptionsConstants_allContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_indexer.
    fn visit_withoptionsconstants_indexer(
        &mut self,
        ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_list.
    fn visit_withoptionsconstants_list(&mut self, ctx: &WithOptionsConstants_listContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsConstants_map.
    fn visit_withoptionsconstants_map(&mut self, ctx: &WithOptionsConstants_mapContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsConstants_reader.
    fn visit_iooptionsconstants_reader(&mut self, ctx: &IoOptionsConstants_readerContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsConstants_writer.
    fn visit_iooptionsconstants_writer(&mut self, ctx: &IoOptionsConstants_writerContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsConstants_gryo.
    fn visit_iooptionsconstants_gryo(&mut self, ctx: &IoOptionsConstants_gryoContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsConstants_graphson.
    fn visit_iooptionsconstants_graphson(
        &mut self,
        ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsConstants_graphml.
    fn visit_iooptionsconstants_graphml(
        &mut self,
        ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#connectedComponentStringConstant.
    fn visit_connectedcomponentstringconstant(
        &mut self,
        ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#pageRankStringConstant.
    fn visit_pagerankstringconstant(&mut self, ctx: &PageRankStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#peerPressureStringConstant.
    fn visit_peerpressurestringconstant(
        &mut self,
        ctx: &PeerPressureStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#shortestPathStringConstant.
    fn visit_shortestpathstringconstant(
        &mut self,
        ctx: &ShortestPathStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#withOptionsStringConstant.
    fn visit_withoptionsstringconstant(&mut self, ctx: &WithOptionsStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#ioOptionsStringConstant.
    fn visit_iooptionsstringconstant(&mut self, ctx: &IoOptionsStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#booleanArgument.
    fn visit_booleanargument(&mut self, ctx: &BooleanArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#integerArgument.
    fn visit_integerargument(&mut self, ctx: &IntegerArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringArgument.
    fn visit_stringargument(&mut self, ctx: &StringArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringNullableArgument.
    fn visit_stringnullableargument(&mut self, ctx: &StringNullableArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringNullableArgumentVarargs.
    fn visit_stringnullableargumentvarargs(
        &mut self,
        ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#dateArgument.
    fn visit_dateargument(&mut self, ctx: &DateArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericArgument.
    fn visit_genericargument(&mut self, ctx: &GenericArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericArgumentVarargs.
    fn visit_genericargumentvarargs(&mut self, ctx: &GenericArgumentVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericMapArgument.
    fn visit_genericmapargument(&mut self, ctx: &GenericMapArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericMapNullableArgument.
    fn visit_genericmapnullableargument(
        &mut self,
        ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalStrategyVarargs.
    fn visit_traversalstrategyvarargs(&mut self, ctx: &TraversalStrategyVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#traversalStrategyExpr.
    fn visit_traversalstrategyexpr(&mut self, ctx: &TraversalStrategyExprContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#classTypeList.
    fn visit_classtypelist(&mut self, ctx: &ClassTypeListContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#classTypeExpr.
    fn visit_classtypeexpr(&mut self, ctx: &ClassTypeExprContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nestedTraversalList.
    fn visit_nestedtraversallist(&mut self, ctx: &NestedTraversalListContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nestedTraversalExpr.
    fn visit_nestedtraversalexpr(&mut self, ctx: &NestedTraversalExprContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericCollectionLiteral.
    fn visit_genericcollectionliteral(&mut self, ctx: &GenericCollectionLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericLiteralVarargs.
    fn visit_genericliteralvarargs(&mut self, ctx: &GenericLiteralVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericLiteralExpr.
    fn visit_genericliteralexpr(&mut self, ctx: &GenericLiteralExprContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericMapNullableLiteral.
    fn visit_genericmapnullableliteral(&mut self, ctx: &GenericMapNullableLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericRangeLiteral.
    fn visit_genericrangeliteral(&mut self, ctx: &GenericRangeLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericSetLiteral.
    fn visit_genericsetliteral(&mut self, ctx: &GenericSetLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringNullableLiteralVarargs.
    fn visit_stringnullableliteralvarargs(
        &mut self,
        ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericLiteral.
    fn visit_genericliteral(&mut self, ctx: &GenericLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#genericMapLiteral.
    fn visit_genericmapliteral(&mut self, ctx: &GenericMapLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#mapKey.
    fn visit_mapkey(&mut self, ctx: &MapKeyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#mapEntry.
    fn visit_mapentry(&mut self, ctx: &MapEntryContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringLiteral.
    fn visit_stringliteral(&mut self, ctx: &StringLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#stringNullableLiteral.
    fn visit_stringnullableliteral(&mut self, ctx: &StringNullableLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#integerLiteral.
    fn visit_integerliteral(&mut self, ctx: &IntegerLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#floatLiteral.
    fn visit_floatliteral(&mut self, ctx: &FloatLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#numericLiteral.
    fn visit_numericliteral(&mut self, ctx: &NumericLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#booleanLiteral.
    fn visit_booleanliteral(&mut self, ctx: &BooleanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#dateLiteral.
    fn visit_dateliteral(&mut self, ctx: &DateLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nullLiteral.
    fn visit_nullliteral(&mut self, ctx: &NullLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nanLiteral.
    fn visit_nanliteral(&mut self, ctx: &NanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#infLiteral.
    fn visit_infliteral(&mut self, ctx: &InfLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#uuidLiteral.
    fn visit_uuidliteral(&mut self, ctx: &UuidLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#nakedKey.
    fn visit_nakedkey(&mut self, ctx: &NakedKeyContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#classType.
    fn visit_classtype(&mut self, ctx: &ClassTypeContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#variable.
    fn visit_variable(&mut self, ctx: &VariableContext<'input>) {
        self.visit_children(ctx)
    }

    // Visit a parse tree produced by GremlinParser#keyword.
    fn visit_keyword(&mut self, ctx: &KeywordContext<'input>) {
        self.visit_children(ctx)
    }
}
