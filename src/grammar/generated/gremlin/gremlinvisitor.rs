#![allow(nonstandard_style)]
// Generated from languages/gremlin/Gremlin.g4 by ANTLR 4.13.2
use super::gremlinparser::*;
use antlr4rust::tree::{ParseTreeVisitor, ParseTreeVisitorCompat};

/**
 * This interface defines a complete generic visitor for a parse tree produced
 * by {@link GremlinParser}.
 */
pub trait GremlinVisitor<'input>: ParseTreeVisitor<'input, GremlinParserContextType> {
    /**
     * Visit a parse tree produced by {@link GremlinParser#queryList}.
     * @param ctx the parse tree
     */
    fn visit_queryList(&mut self, ctx: &QueryListContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#query}.
     * @param ctx the parse tree
     */
    fn visit_query(&mut self, ctx: &QueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#emptyQuery}.
     * @param ctx the parse tree
     */
    fn visit_emptyQuery(&mut self, ctx: &EmptyQueryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSource}.
     * @param ctx the parse tree
     */
    fn visit_traversalSource(&mut self, ctx: &TraversalSourceContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#transactionPart}.
     * @param ctx the parse tree
     */
    fn visit_transactionPart(&mut self, ctx: &TransactionPartContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#rootTraversal}.
     * @param ctx the parse tree
     */
    fn visit_rootTraversal(&mut self, ctx: &RootTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod(&mut self, ctx: &TraversalSourceSelfMethodContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withBulk}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withBulk(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withPath(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSack}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withSack(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSideEffect}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withSideEffect(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withStrategies}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withoutStrategies}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withoutStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_with(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod(
        &mut self,
        ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_addE(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_addV(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_E}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_E(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_V}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_V(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_inject}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_inject(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_io}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_io(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_empty}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_empty(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_union}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_union(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#chainedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_chainedTraversal(&mut self, ctx: &ChainedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversal(&mut self, ctx: &NestedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#terminatedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_terminatedTraversal(&mut self, ctx: &TerminatedTraversalContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod(&mut self, ctx: &TraversalMethodContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_V}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_V(&mut self, ctx: &TraversalMethod_VContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_E}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_E(&mut self, ctx: &TraversalMethod_EContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addE_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addE_String(
        &mut self,
        ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addE_Traversal(
        &mut self,
        ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_Empty(
        &mut self,
        ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_String(
        &mut self,
        ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_Traversal(
        &mut self,
        ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_aggregate_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_aggregate}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_aggregate_String(
        &mut self,
        ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_all_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_all}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_all_P(&mut self, ctx: &TraversalMethod_all_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_and}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_and(&mut self, ctx: &TraversalMethod_andContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_any_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_any}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_any_P(&mut self, ctx: &TraversalMethod_any_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_as}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_as(&mut self, ctx: &TraversalMethod_asContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_asBool}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asBool(&mut self, ctx: &TraversalMethod_asBoolContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_asDate}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asDate(&mut self, ctx: &TraversalMethod_asDateContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asNumber_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asNumber_Empty(
        &mut self,
        ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asNumber_traversalGType}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asNumber_traversalGType(
        &mut self,
        ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asString_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asString_Empty(
        &mut self,
        ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asString_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asString_Scope(
        &mut self,
        ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_Consumer}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_Consumer(
        &mut self,
        ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_Empty(
        &mut self,
        ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_int(
        &mut self,
        ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_both}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_both(&mut self, ctx: &TraversalMethod_bothContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_bothE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_bothE(&mut self, ctx: &TraversalMethod_bothEContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_bothV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_bothV(&mut self, ctx: &TraversalMethod_bothVContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_branch}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_branch(&mut self, ctx: &TraversalMethod_branchContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Empty(&mut self, ctx: &TraversalMethod_by_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Function(
        &mut self,
        ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Function_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Function_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Order}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Order(&mut self, ctx: &TraversalMethod_by_OrderContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_String(&mut self, ctx: &TraversalMethod_by_StringContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_String_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_String_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_T}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_T(&mut self, ctx: &TraversalMethod_by_TContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Traversal(
        &mut self,
        ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Traversal_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Traversal_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string(
        &mut self,
        ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_map(
        &mut self,
        ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_cap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_cap(&mut self, ctx: &TraversalMethod_capContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Function(
        &mut self,
        ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Predicate_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_coalesce}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_coalesce(&mut self, ctx: &TraversalMethod_coalesceContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_coin}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_coin(&mut self, ctx: &TraversalMethod_coinContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_combine_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_combine}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_combine_Object(
        &mut self,
        ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_concat_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_concat_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_concat_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_concat_String(
        &mut self,
        ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_conjoin_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_conjoin}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_conjoin_String(
        &mut self,
        ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_connectedComponent}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_connectedComponent(
        &mut self,
        ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_constant}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_constant(&mut self, ctx: &TraversalMethod_constantContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_count_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_count_Empty(
        &mut self,
        ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_count_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_count_Scope(
        &mut self,
        ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_cyclicPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_cyclicPath(
        &mut self,
        ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_dateAdd}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateAdd(&mut self, ctx: &TraversalMethod_dateAddContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dateDiff_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateDiff_Traversal(
        &mut self,
        ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dateDiff_Date}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateDiff_Date(
        &mut self,
        ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dedup_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dedup_Scope_String(
        &mut self,
        ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dedup_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dedup_String(
        &mut self,
        ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_difference_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_difference}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_difference_Object(
        &mut self,
        ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_discard}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_discard(&mut self, ctx: &TraversalMethod_discardContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_disjunct_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_disjunct}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_disjunct_Object(
        &mut self,
        ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_drop}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_drop(&mut self, ctx: &TraversalMethod_dropContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_element}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_element(&mut self, ctx: &TraversalMethod_elementContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_elementMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_elementMap(
        &mut self,
        ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Empty(
        &mut self,
        ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Predicate(
        &mut self,
        ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Traversal(
        &mut self,
        ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fail_Empty(
        &mut self,
        ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fail_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fail_String(
        &mut self,
        ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_filter_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_filter_Predicate(
        &mut self,
        ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_filter_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_filter_Traversal(
        &mut self,
        ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_flatMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_flatMap(&mut self, ctx: &TraversalMethod_flatMapContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fold_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fold_Empty(
        &mut self,
        ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fold_Object_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fold_Object_BiFunction(
        &mut self,
        ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_format_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_format}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_format_String(
        &mut self,
        ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_from_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_from_String(
        &mut self,
        ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_from_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_from_Traversal(
        &mut self,
        ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_group_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_group_Empty(
        &mut self,
        ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_group_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_group_String(
        &mut self,
        ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_groupCount_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_groupCount_Empty(
        &mut self,
        ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_groupCount_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_groupCount_String(
        &mut self,
        ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String(
        &mut self,
        ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_T_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_T_Object(
        &mut self,
        ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_T_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_T_P(&mut self, ctx: &TraversalMethod_has_T_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasId_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasId_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasId_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasId_P(&mut self, ctx: &TraversalMethod_hasId_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasKey_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasKey_P(&mut self, ctx: &TraversalMethod_hasKey_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasKey_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasKey_String_String(
        &mut self,
        ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasLabel_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasLabel_P(
        &mut self,
        ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasLabel_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasLabel_String_String(
        &mut self,
        ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_hasNot}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasNot(&mut self, ctx: &TraversalMethod_hasNotContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasValue_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasValue_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasValue_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasValue_P(
        &mut self,
        ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_id}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_id(&mut self, ctx: &TraversalMethod_idContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_identity}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_identity(&mut self, ctx: &TraversalMethod_identityContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_in}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_in(&mut self, ctx: &TraversalMethod_inContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inE(&mut self, ctx: &TraversalMethod_inEContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_intersect_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_intersect}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_intersect_Object(
        &mut self,
        ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inV(&mut self, ctx: &TraversalMethod_inVContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_index}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_index(&mut self, ctx: &TraversalMethod_indexContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inject}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inject(&mut self, ctx: &TraversalMethod_injectContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_is_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_is_Object(&mut self, ctx: &TraversalMethod_is_ObjectContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_is_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_is_P(&mut self, ctx: &TraversalMethod_is_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_key}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_key(&mut self, ctx: &TraversalMethod_keyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_label}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_label(&mut self, ctx: &TraversalMethod_labelContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_length_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_length_Empty(
        &mut self,
        ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_length_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_length_Scope(
        &mut self,
        ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_limit_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_limit_Scope_long(
        &mut self,
        ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_limit_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_limit_long(
        &mut self,
        ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_local}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_local(&mut self, ctx: &TraversalMethod_localContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_loops_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_loops_Empty(
        &mut self,
        ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_loops_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_loops_String(
        &mut self,
        ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_lTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_lTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_lTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_lTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_map}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_map(&mut self, ctx: &TraversalMethod_mapContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_match}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_match(&mut self, ctx: &TraversalMethod_matchContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_math}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_math(&mut self, ctx: &TraversalMethod_mathContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_max_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_max_Empty(&mut self, ctx: &TraversalMethod_max_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_max_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_max_Scope(&mut self, ctx: &TraversalMethod_max_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mean_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mean_Empty(
        &mut self,
        ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mean_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mean_Scope(
        &mut self,
        ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_merge_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_merge}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_merge_Object(
        &mut self,
        ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_empty(
        &mut self,
        ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_empty(
        &mut self,
        ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_min_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_min_Empty(&mut self, ctx: &TraversalMethod_min_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_min_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_min_Scope(&mut self, ctx: &TraversalMethod_min_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_none_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_none}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_none_P(&mut self, ctx: &TraversalMethod_none_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_not}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_not(&mut self, ctx: &TraversalMethod_notContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Map(
        &mut self,
        ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Map_Cardinality}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Map_Cardinality(
        &mut self,
        ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Object_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Object_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_optional}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_optional(&mut self, ctx: &TraversalMethod_optionalContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_or}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_or(&mut self, ctx: &TraversalMethod_orContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_order_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_order_Empty(
        &mut self,
        ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_order_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_order_Scope(
        &mut self,
        ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_otherV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_otherV(&mut self, ctx: &TraversalMethod_otherVContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_out}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_out(&mut self, ctx: &TraversalMethod_outContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_outE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_outE(&mut self, ctx: &TraversalMethod_outEContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_outV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_outV(&mut self, ctx: &TraversalMethod_outVContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_pageRank_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_pageRank_Empty(
        &mut self,
        ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_pageRank_double}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_pageRank_double(
        &mut self,
        ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_path}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_path(&mut self, ctx: &TraversalMethod_pathContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_peerPressure}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_peerPressure(
        &mut self,
        ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_product_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_product}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_product_Object(
        &mut self,
        ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_profile_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_profile_Empty(
        &mut self,
        ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_profile_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_profile_String(
        &mut self,
        ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_project}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_project(&mut self, ctx: &TraversalMethod_projectContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_properties}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_properties(
        &mut self,
        ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Cardinality_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Cardinality_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Object(
        &mut self,
        ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_propertyMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_propertyMap(
        &mut self,
        ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_range_Scope_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_range_Scope_long_long(
        &mut self,
        ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_range_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_range_long_long(
        &mut self,
        ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_read}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_read(&mut self, ctx: &TraversalMethod_readContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_repeat_String_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_repeat_String_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_repeat_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_repeat_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_replace_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_replace_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_replace_Scope_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_replace_Scope_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_reverse_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_reverse}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_reverse_Empty(
        &mut self,
        ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_rTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_rTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_rTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_rTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sack_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sack_BiFunction(
        &mut self,
        ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sack_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sack_Empty(
        &mut self,
        ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sample_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sample_Scope_int(
        &mut self,
        ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sample_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sample_int(
        &mut self,
        ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Column}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Column(
        &mut self,
        ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_String(
        &mut self,
        ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_shortestPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_shortestPath(
        &mut self,
        ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_sideEffect}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sideEffect(
        &mut self,
        ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_simplePath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_simplePath(
        &mut self,
        ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_skip_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_skip_Scope_long(
        &mut self,
        ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_skip_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_skip_long(&mut self, ctx: &TraversalMethod_skip_longContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_split_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_split_String(
        &mut self,
        ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_split_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_split_Scope_String(
        &mut self,
        ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_subgraph}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_subgraph(&mut self, ctx: &TraversalMethod_subgraphContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_int(
        &mut self,
        ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_Scope_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_Scope_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_Scope_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sum_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sum_Empty(&mut self, ctx: &TraversalMethod_sum_EmptyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sum_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sum_Scope(&mut self, ctx: &TraversalMethod_sum_ScopeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Empty(
        &mut self,
        ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Scope(
        &mut self,
        ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Scope_long(
        &mut self,
        ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_long(&mut self, ctx: &TraversalMethod_tail_longContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_timeLimit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_timeLimit(&mut self, ctx: &TraversalMethod_timeLimitContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_times}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_times(&mut self, ctx: &TraversalMethod_timesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_Direction_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_Direction_String(
        &mut self,
        ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_String(&mut self, ctx: &TraversalMethod_to_StringContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_Traversal(
        &mut self,
        ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_toE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toE(&mut self, ctx: &TraversalMethod_toEContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toLower_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toLower_Empty(
        &mut self,
        ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toLower_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toLower_Scope(
        &mut self,
        ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toUpper_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toUpper_Empty(
        &mut self,
        ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toUpper_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toUpper_Scope(
        &mut self,
        ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_toV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toV(&mut self, ctx: &TraversalMethod_toVContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tree_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tree_Empty(
        &mut self,
        ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tree_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tree_String(
        &mut self,
        ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_trim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_trim_Empty(
        &mut self,
        ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_trim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_trim_Scope(
        &mut self,
        ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_unfold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_unfold(&mut self, ctx: &TraversalMethod_unfoldContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_union}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_union(&mut self, ctx: &TraversalMethod_unionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_until_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_until_Predicate(
        &mut self,
        ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_until_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_until_Traversal(
        &mut self,
        ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_value}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_value(&mut self, ctx: &TraversalMethod_valueContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_valueMap_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_valueMap_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_valueMap_boolean_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_valueMap_boolean_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_values}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_values(&mut self, ctx: &TraversalMethod_valuesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_P(&mut self, ctx: &TraversalMethod_where_PContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_String_P(
        &mut self,
        ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_Traversal(
        &mut self,
        ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_with_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_with_String(
        &mut self,
        ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_with_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_with_String_Object(
        &mut self,
        ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_write}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_write(&mut self, ctx: &TraversalMethod_writeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategy}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategy(&mut self, ctx: &TraversalStrategyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#configuration}.
     * @param ctx the parse tree
     */
    fn visit_configuration(&mut self, ctx: &ConfigurationContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalScope}.
     * @param ctx the parse tree
     */
    fn visit_traversalScope(&mut self, ctx: &TraversalScopeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalBarrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalBarrier(&mut self, ctx: &TraversalBarrierContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalT}.
     * @param ctx the parse tree
     */
    fn visit_traversalT(&mut self, ctx: &TraversalTContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTShort}.
     * @param ctx the parse tree
     */
    fn visit_traversalTShort(&mut self, ctx: &TraversalTShortContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTLong}.
     * @param ctx the parse tree
     */
    fn visit_traversalTLong(&mut self, ctx: &TraversalTLongContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMerge}.
     * @param ctx the parse tree
     */
    fn visit_traversalMerge(&mut self, ctx: &TraversalMergeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalOrder}.
     * @param ctx the parse tree
     */
    fn visit_traversalOrder(&mut self, ctx: &TraversalOrderContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirection}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirection(&mut self, ctx: &TraversalDirectionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirectionShort}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirectionShort(&mut self, ctx: &TraversalDirectionShortContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirectionLong}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirectionLong(&mut self, ctx: &TraversalDirectionLongContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalCardinality}.
     * @param ctx the parse tree
     */
    fn visit_traversalCardinality(&mut self, ctx: &TraversalCardinalityContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalColumn}.
     * @param ctx the parse tree
     */
    fn visit_traversalColumn(&mut self, ctx: &TraversalColumnContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPop}.
     * @param ctx the parse tree
     */
    fn visit_traversalPop(&mut self, ctx: &TraversalPopContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalOperator}.
     * @param ctx the parse tree
     */
    fn visit_traversalOperator(&mut self, ctx: &TraversalOperatorContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPick}.
     * @param ctx the parse tree
     */
    fn visit_traversalPick(&mut self, ctx: &TraversalPickContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDT}.
     * @param ctx the parse tree
     */
    fn visit_traversalDT(&mut self, ctx: &TraversalDTContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalGType}.
     * @param ctx the parse tree
     */
    fn visit_traversalGType(&mut self, ctx: &TraversalGTypeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate(&mut self, ctx: &TraversalPredicateContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod(&mut self, ctx: &TraversalTerminalMethodContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSackMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSackMethod(&mut self, ctx: &TraversalSackMethodContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalComparator}.
     * @param ctx the parse tree
     */
    fn visit_traversalComparator(&mut self, ctx: &TraversalComparatorContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalFunction}.
     * @param ctx the parse tree
     */
    fn visit_traversalFunction(&mut self, ctx: &TraversalFunctionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalBiFunction}.
     * @param ctx the parse tree
     */
    fn visit_traversalBiFunction(&mut self, ctx: &TraversalBiFunctionContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_eq}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_eq(&mut self, ctx: &TraversalPredicate_eqContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_neq}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_neq(&mut self, ctx: &TraversalPredicate_neqContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_typeOf}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_typeOf(&mut self, ctx: &TraversalPredicate_typeOfContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_lt}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_lt(&mut self, ctx: &TraversalPredicate_ltContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_lte}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_lte(&mut self, ctx: &TraversalPredicate_lteContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_gt}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_gt(&mut self, ctx: &TraversalPredicate_gtContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_gte}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_gte(&mut self, ctx: &TraversalPredicate_gteContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_inside}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_inside(&mut self, ctx: &TraversalPredicate_insideContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_outside}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_outside(
        &mut self,
        ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_between}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_between(
        &mut self,
        ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_within}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_within(&mut self, ctx: &TraversalPredicate_withinContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_without}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_without(
        &mut self,
        ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_not}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_not(&mut self, ctx: &TraversalPredicate_notContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_containing}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_containing(
        &mut self,
        ctx: &TraversalPredicate_containingContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notContaining}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notContaining(
        &mut self,
        ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_startingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_startingWith(
        &mut self,
        ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notStartingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notStartingWith(
        &mut self,
        ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_endingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_endingWith(
        &mut self,
        ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notEndingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notEndingWith(
        &mut self,
        ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_regex}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_regex(&mut self, ctx: &TraversalPredicate_regexContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notRegex}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notRegex(
        &mut self,
        ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_explain}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_explain(
        &mut self,
        ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_hasNext}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_hasNext(
        &mut self,
        ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_iterate}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_iterate(
        &mut self,
        ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_tryNext}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_tryNext(
        &mut self,
        ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_next}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_next(
        &mut self,
        ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toList}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toList(
        &mut self,
        ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toSet}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toBulkSet}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toBulkSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionKeys}.
     * @param ctx the parse tree
     */
    fn visit_withOptionKeys(&mut self, ctx: &WithOptionKeysContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants(
        &mut self,
        ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants(&mut self, ctx: &PageRankConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants(&mut self, ctx: &PeerPressureConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants(&mut self, ctx: &ShortestPathConstantsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsValues}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsValues(&mut self, ctx: &WithOptionsValuesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsKeys}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsKeys(&mut self, ctx: &IoOptionsKeysContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsValues}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsValues(&mut self, ctx: &IoOptionsValuesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_component}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_component(
        &mut self,
        ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_edges(
        &mut self,
        ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_propertyName(
        &mut self,
        ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_edges(&mut self, ctx: &PageRankConstants_edgesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_times}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_times(&mut self, ctx: &PageRankConstants_timesContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_propertyName(
        &mut self,
        ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_edges(
        &mut self,
        ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_times}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_times(
        &mut self,
        ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_propertyName(
        &mut self,
        ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_target}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_target(
        &mut self,
        ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_edges(
        &mut self,
        ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_distance}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_distance(
        &mut self,
        ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_maxDistance}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_maxDistance(
        &mut self,
        ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_includeEdges}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_includeEdges(
        &mut self,
        ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_tokens}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_tokens(
        &mut self,
        ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_none}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_none(&mut self, ctx: &WithOptionsConstants_noneContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_ids}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_ids(&mut self, ctx: &WithOptionsConstants_idsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_labels}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_labels(
        &mut self,
        ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_keys}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_keys(&mut self, ctx: &WithOptionsConstants_keysContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_values}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_values(
        &mut self,
        ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_all}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_all(&mut self, ctx: &WithOptionsConstants_allContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_indexer}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_indexer(
        &mut self,
        ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_list}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_list(&mut self, ctx: &WithOptionsConstants_listContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_map}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_map(&mut self, ctx: &WithOptionsConstants_mapContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_reader}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_reader(&mut self, ctx: &IoOptionsConstants_readerContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_writer}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_writer(&mut self, ctx: &IoOptionsConstants_writerContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_gryo}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_gryo(&mut self, ctx: &IoOptionsConstants_gryoContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphson}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_graphson(
        &mut self,
        ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphml}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_graphml(
        &mut self,
        ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentStringConstant(
        &mut self,
        ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_pageRankStringConstant(&mut self, ctx: &PageRankStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureStringConstant(
        &mut self,
        ctx: &PeerPressureStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathStringConstant(
        &mut self,
        ctx: &ShortestPathStringConstantContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsStringConstant(&mut self, ctx: &WithOptionsStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsStringConstant(&mut self, ctx: &IoOptionsStringConstantContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#booleanArgument}.
     * @param ctx the parse tree
     */
    fn visit_booleanArgument(&mut self, ctx: &BooleanArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#integerArgument}.
     * @param ctx the parse tree
     */
    fn visit_integerArgument(&mut self, ctx: &IntegerArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringArgument}.
     * @param ctx the parse tree
     */
    fn visit_stringArgument(&mut self, ctx: &StringArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableArgument}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableArgument(&mut self, ctx: &StringNullableArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableArgumentVarargs(
        &mut self,
        ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#dateArgument}.
     * @param ctx the parse tree
     */
    fn visit_dateArgument(&mut self, ctx: &DateArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericArgument(&mut self, ctx: &GenericArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn visit_genericArgumentVarargs(&mut self, ctx: &GenericArgumentVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericMapArgument(&mut self, ctx: &GenericMapArgumentContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapNullableArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericMapNullableArgument(
        &mut self,
        ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategyVarargs}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategyVarargs(&mut self, ctx: &TraversalStrategyVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategyExpr}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategyExpr(&mut self, ctx: &TraversalStrategyExprContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classTypeList}.
     * @param ctx the parse tree
     */
    fn visit_classTypeList(&mut self, ctx: &ClassTypeListContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classTypeExpr}.
     * @param ctx the parse tree
     */
    fn visit_classTypeExpr(&mut self, ctx: &ClassTypeExprContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversalList}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversalList(&mut self, ctx: &NestedTraversalListContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversalExpr}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversalExpr(&mut self, ctx: &NestedTraversalExprContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericCollectionLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericCollectionLiteral(&mut self, ctx: &GenericCollectionLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteralVarargs(&mut self, ctx: &GenericLiteralVarargsContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteralExpr}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteralExpr(&mut self, ctx: &GenericLiteralExprContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapNullableLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericMapNullableLiteral(&mut self, ctx: &GenericMapNullableLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericRangeLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericRangeLiteral(&mut self, ctx: &GenericRangeLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericSetLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericSetLiteral(&mut self, ctx: &GenericSetLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableLiteralVarargs(
        &mut self,
        ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteral(&mut self, ctx: &GenericLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericMapLiteral(&mut self, ctx: &GenericMapLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#mapKey}.
     * @param ctx the parse tree
     */
    fn visit_mapKey(&mut self, ctx: &MapKeyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#mapEntry}.
     * @param ctx the parse tree
     */
    fn visit_mapEntry(&mut self, ctx: &MapEntryContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringLiteral}.
     * @param ctx the parse tree
     */
    fn visit_stringLiteral(&mut self, ctx: &StringLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableLiteral}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableLiteral(&mut self, ctx: &StringNullableLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#integerLiteral}.
     * @param ctx the parse tree
     */
    fn visit_integerLiteral(&mut self, ctx: &IntegerLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#floatLiteral}.
     * @param ctx the parse tree
     */
    fn visit_floatLiteral(&mut self, ctx: &FloatLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#numericLiteral}.
     * @param ctx the parse tree
     */
    fn visit_numericLiteral(&mut self, ctx: &NumericLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#booleanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_booleanLiteral(&mut self, ctx: &BooleanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#dateLiteral}.
     * @param ctx the parse tree
     */
    fn visit_dateLiteral(&mut self, ctx: &DateLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nullLiteral}.
     * @param ctx the parse tree
     */
    fn visit_nullLiteral(&mut self, ctx: &NullLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_nanLiteral(&mut self, ctx: &NanLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#infLiteral}.
     * @param ctx the parse tree
     */
    fn visit_infLiteral(&mut self, ctx: &InfLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#uuidLiteral}.
     * @param ctx the parse tree
     */
    fn visit_uuidLiteral(&mut self, ctx: &UuidLiteralContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nakedKey}.
     * @param ctx the parse tree
     */
    fn visit_nakedKey(&mut self, ctx: &NakedKeyContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classType}.
     * @param ctx the parse tree
     */
    fn visit_classType(&mut self, ctx: &ClassTypeContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#variable}.
     * @param ctx the parse tree
     */
    fn visit_variable(&mut self, ctx: &VariableContext<'input>) {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#keyword}.
     * @param ctx the parse tree
     */
    fn visit_keyword(&mut self, ctx: &KeywordContext<'input>) {
        self.visit_children(ctx)
    }
}

pub trait GremlinVisitorCompat<'input>:
    ParseTreeVisitorCompat<'input, Node = GremlinParserContextType>
{
    /**
     * Visit a parse tree produced by {@link GremlinParser#queryList}.
     * @param ctx the parse tree
     */
    fn visit_queryList(&mut self, ctx: &QueryListContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#query}.
     * @param ctx the parse tree
     */
    fn visit_query(&mut self, ctx: &QueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#emptyQuery}.
     * @param ctx the parse tree
     */
    fn visit_emptyQuery(&mut self, ctx: &EmptyQueryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSource}.
     * @param ctx the parse tree
     */
    fn visit_traversalSource(&mut self, ctx: &TraversalSourceContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#transactionPart}.
     * @param ctx the parse tree
     */
    fn visit_transactionPart(&mut self, ctx: &TransactionPartContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#rootTraversal}.
     * @param ctx the parse tree
     */
    fn visit_rootTraversal(&mut self, ctx: &RootTraversalContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod(
        &mut self,
        ctx: &TraversalSourceSelfMethodContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withBulk}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withBulk(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withPath(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSack}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withSack(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSideEffect}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withSideEffect(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withStrategies}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withoutStrategies}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_withoutStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSelfMethod_with(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod(
        &mut self,
        ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_addE(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_addV(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_E}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_E(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_V}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_V(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_inject}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_inject(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_io}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_io(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_empty}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_empty(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_union}.
     * @param ctx the parse tree
     */
    fn visit_traversalSourceSpawnMethod_union(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#chainedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_chainedTraversal(&mut self, ctx: &ChainedTraversalContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversal(&mut self, ctx: &NestedTraversalContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#terminatedTraversal}.
     * @param ctx the parse tree
     */
    fn visit_terminatedTraversal(
        &mut self,
        ctx: &TerminatedTraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod(&mut self, ctx: &TraversalMethodContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_V}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_V(&mut self, ctx: &TraversalMethod_VContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_E}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_E(&mut self, ctx: &TraversalMethod_EContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addE_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addE_String(
        &mut self,
        ctx: &TraversalMethod_addE_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addE_Traversal(
        &mut self,
        ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_Empty(
        &mut self,
        ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_String(
        &mut self,
        ctx: &TraversalMethod_addV_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_addV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_addV_Traversal(
        &mut self,
        ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_aggregate_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_aggregate}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_aggregate_String(
        &mut self,
        ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_all_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_all}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_all_P(
        &mut self,
        ctx: &TraversalMethod_all_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_and}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_and(
        &mut self,
        ctx: &TraversalMethod_andContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_any_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_any}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_any_P(
        &mut self,
        ctx: &TraversalMethod_any_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_as}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_as(
        &mut self,
        ctx: &TraversalMethod_asContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_asBool}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asBool(
        &mut self,
        ctx: &TraversalMethod_asBoolContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_asDate}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asDate(
        &mut self,
        ctx: &TraversalMethod_asDateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asNumber_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asNumber_Empty(
        &mut self,
        ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asNumber_traversalGType}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asNumber_traversalGType(
        &mut self,
        ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asString_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asString_Empty(
        &mut self,
        ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_asString_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_asString_Scope(
        &mut self,
        ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_Consumer}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_Consumer(
        &mut self,
        ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_Empty(
        &mut self,
        ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_barrier_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_barrier_int(
        &mut self,
        ctx: &TraversalMethod_barrier_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_both}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_both(
        &mut self,
        ctx: &TraversalMethod_bothContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_bothE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_bothE(
        &mut self,
        ctx: &TraversalMethod_bothEContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_bothV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_bothV(
        &mut self,
        ctx: &TraversalMethod_bothVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_branch}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_branch(
        &mut self,
        ctx: &TraversalMethod_branchContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Empty(
        &mut self,
        ctx: &TraversalMethod_by_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Function(
        &mut self,
        ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Function_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Function_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Order}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Order(
        &mut self,
        ctx: &TraversalMethod_by_OrderContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_String(
        &mut self,
        ctx: &TraversalMethod_by_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_String_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_String_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_T}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_T(
        &mut self,
        ctx: &TraversalMethod_by_TContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Traversal(
        &mut self,
        ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_by_Traversal_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_by_Traversal_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string(
        &mut self,
        ctx: &TraversalMethod_call_stringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_map(
        &mut self,
        ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_cap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_cap(
        &mut self,
        ctx: &TraversalMethod_capContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Function(
        &mut self,
        ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Predicate_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_choose_Traversal_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_coalesce}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_coalesce(
        &mut self,
        ctx: &TraversalMethod_coalesceContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_coin}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_coin(
        &mut self,
        ctx: &TraversalMethod_coinContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_combine_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_combine}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_combine_Object(
        &mut self,
        ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_concat_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_concat_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_concat_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_concat_String(
        &mut self,
        ctx: &TraversalMethod_concat_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_conjoin_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_conjoin}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_conjoin_String(
        &mut self,
        ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_connectedComponent}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_connectedComponent(
        &mut self,
        ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_constant}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_constant(
        &mut self,
        ctx: &TraversalMethod_constantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_count_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_count_Empty(
        &mut self,
        ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_count_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_count_Scope(
        &mut self,
        ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_cyclicPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_cyclicPath(
        &mut self,
        ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_dateAdd}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateAdd(
        &mut self,
        ctx: &TraversalMethod_dateAddContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dateDiff_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateDiff_Traversal(
        &mut self,
        ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dateDiff_Date}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dateDiff_Date(
        &mut self,
        ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dedup_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dedup_Scope_String(
        &mut self,
        ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_dedup_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_dedup_String(
        &mut self,
        ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_difference_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_difference}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_difference_Object(
        &mut self,
        ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_discard}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_discard(
        &mut self,
        ctx: &TraversalMethod_discardContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_disjunct_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_disjunct}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_disjunct_Object(
        &mut self,
        ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_drop}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_drop(
        &mut self,
        ctx: &TraversalMethod_dropContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_element}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_element(
        &mut self,
        ctx: &TraversalMethod_elementContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_elementMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_elementMap(
        &mut self,
        ctx: &TraversalMethod_elementMapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Empty(
        &mut self,
        ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Predicate(
        &mut self,
        ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_emit_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_emit_Traversal(
        &mut self,
        ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fail_Empty(
        &mut self,
        ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fail_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fail_String(
        &mut self,
        ctx: &TraversalMethod_fail_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_filter_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_filter_Predicate(
        &mut self,
        ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_filter_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_filter_Traversal(
        &mut self,
        ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_flatMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_flatMap(
        &mut self,
        ctx: &TraversalMethod_flatMapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fold_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fold_Empty(
        &mut self,
        ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_fold_Object_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_fold_Object_BiFunction(
        &mut self,
        ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_format_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_format}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_format_String(
        &mut self,
        ctx: &TraversalMethod_format_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_from_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_from_String(
        &mut self,
        ctx: &TraversalMethod_from_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_from_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_from_Traversal(
        &mut self,
        ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_group_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_group_Empty(
        &mut self,
        ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_group_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_group_String(
        &mut self,
        ctx: &TraversalMethod_group_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_groupCount_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_groupCount_Empty(
        &mut self,
        ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_groupCount_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_groupCount_String(
        &mut self,
        ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String(
        &mut self,
        ctx: &TraversalMethod_has_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_String_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_String_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_T_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_T_Object(
        &mut self,
        ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_has_T_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_has_T_P(
        &mut self,
        ctx: &TraversalMethod_has_T_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasId_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasId_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasId_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasId_P(
        &mut self,
        ctx: &TraversalMethod_hasId_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasKey_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasKey_P(
        &mut self,
        ctx: &TraversalMethod_hasKey_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasKey_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasKey_String_String(
        &mut self,
        ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasLabel_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasLabel_P(
        &mut self,
        ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasLabel_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasLabel_String_String(
        &mut self,
        ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_hasNot}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasNot(
        &mut self,
        ctx: &TraversalMethod_hasNotContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasValue_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasValue_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_hasValue_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_hasValue_P(
        &mut self,
        ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_id}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_id(
        &mut self,
        ctx: &TraversalMethod_idContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_identity}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_identity(
        &mut self,
        ctx: &TraversalMethod_identityContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_in}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_in(
        &mut self,
        ctx: &TraversalMethod_inContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inE(
        &mut self,
        ctx: &TraversalMethod_inEContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_intersect_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_intersect}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_intersect_Object(
        &mut self,
        ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inV(
        &mut self,
        ctx: &TraversalMethod_inVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_index}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_index(
        &mut self,
        ctx: &TraversalMethod_indexContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_inject}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_inject(
        &mut self,
        ctx: &TraversalMethod_injectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_is_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_is_Object(
        &mut self,
        ctx: &TraversalMethod_is_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_is_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_is_P(
        &mut self,
        ctx: &TraversalMethod_is_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_key}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_key(
        &mut self,
        ctx: &TraversalMethod_keyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_label}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_label(
        &mut self,
        ctx: &TraversalMethod_labelContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_length_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_length_Empty(
        &mut self,
        ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_length_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_length_Scope(
        &mut self,
        ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_limit_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_limit_Scope_long(
        &mut self,
        ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_limit_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_limit_long(
        &mut self,
        ctx: &TraversalMethod_limit_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_local}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_local(
        &mut self,
        ctx: &TraversalMethod_localContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_loops_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_loops_Empty(
        &mut self,
        ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_loops_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_loops_String(
        &mut self,
        ctx: &TraversalMethod_loops_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_lTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_lTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_lTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_lTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_map}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_map(
        &mut self,
        ctx: &TraversalMethod_mapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_match}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_match(
        &mut self,
        ctx: &TraversalMethod_matchContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_math}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_math(
        &mut self,
        ctx: &TraversalMethod_mathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_max_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_max_Empty(
        &mut self,
        ctx: &TraversalMethod_max_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_max_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_max_Scope(
        &mut self,
        ctx: &TraversalMethod_max_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mean_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mean_Empty(
        &mut self,
        ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mean_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mean_Scope(
        &mut self,
        ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_merge_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_merge}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_merge_Object(
        &mut self,
        ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_empty(
        &mut self,
        ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_empty(
        &mut self,
        ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_min_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_min_Empty(
        &mut self,
        ctx: &TraversalMethod_min_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_min_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_min_Scope(
        &mut self,
        ctx: &TraversalMethod_min_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_none_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_none}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_none_P(
        &mut self,
        ctx: &TraversalMethod_none_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_not}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_not(
        &mut self,
        ctx: &TraversalMethod_notContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Map(
        &mut self,
        ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Map_Cardinality}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Map_Cardinality(
        &mut self,
        ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Merge_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Merge_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Object_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Object_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_option_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_option_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_optional}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_optional(
        &mut self,
        ctx: &TraversalMethod_optionalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_or}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_or(
        &mut self,
        ctx: &TraversalMethod_orContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_order_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_order_Empty(
        &mut self,
        ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_order_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_order_Scope(
        &mut self,
        ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_otherV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_otherV(
        &mut self,
        ctx: &TraversalMethod_otherVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_out}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_out(
        &mut self,
        ctx: &TraversalMethod_outContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_outE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_outE(
        &mut self,
        ctx: &TraversalMethod_outEContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_outV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_outV(
        &mut self,
        ctx: &TraversalMethod_outVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_pageRank_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_pageRank_Empty(
        &mut self,
        ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_pageRank_double}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_pageRank_double(
        &mut self,
        ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_path}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_path(
        &mut self,
        ctx: &TraversalMethod_pathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_peerPressure}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_peerPressure(
        &mut self,
        ctx: &TraversalMethod_peerPressureContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_product_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_product}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_product_Object(
        &mut self,
        ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_profile_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_profile_Empty(
        &mut self,
        ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_profile_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_profile_String(
        &mut self,
        ctx: &TraversalMethod_profile_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_project}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_project(
        &mut self,
        ctx: &TraversalMethod_projectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_properties}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_properties(
        &mut self,
        ctx: &TraversalMethod_propertiesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Cardinality_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Cardinality_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_property_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_property_Object(
        &mut self,
        ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_propertyMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_propertyMap(
        &mut self,
        ctx: &TraversalMethod_propertyMapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_range_Scope_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_range_Scope_long_long(
        &mut self,
        ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_range_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_range_long_long(
        &mut self,
        ctx: &TraversalMethod_range_long_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_read}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_read(
        &mut self,
        ctx: &TraversalMethod_readContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_repeat_String_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_repeat_String_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_repeat_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_repeat_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_replace_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_replace_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_replace_Scope_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_replace_Scope_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_reverse_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_reverse}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_reverse_Empty(
        &mut self,
        ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_rTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_rTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_rTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_rTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sack_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sack_BiFunction(
        &mut self,
        ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sack_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sack_Empty(
        &mut self,
        ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sample_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sample_Scope_int(
        &mut self,
        ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sample_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sample_int(
        &mut self,
        ctx: &TraversalMethod_sample_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Column}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Column(
        &mut self,
        ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Pop_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Pop_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_String(
        &mut self,
        ctx: &TraversalMethod_select_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_select_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_select_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_shortestPath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_shortestPath(
        &mut self,
        ctx: &TraversalMethod_shortestPathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_sideEffect}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sideEffect(
        &mut self,
        ctx: &TraversalMethod_sideEffectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_simplePath}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_simplePath(
        &mut self,
        ctx: &TraversalMethod_simplePathContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_skip_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_skip_Scope_long(
        &mut self,
        ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_skip_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_skip_long(
        &mut self,
        ctx: &TraversalMethod_skip_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_split_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_split_String(
        &mut self,
        ctx: &TraversalMethod_split_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_split_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_split_Scope_String(
        &mut self,
        ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_subgraph}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_subgraph(
        &mut self,
        ctx: &TraversalMethod_subgraphContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_int(
        &mut self,
        ctx: &TraversalMethod_substring_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_Scope_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_substring_Scope_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_substring_Scope_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sum_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sum_Empty(
        &mut self,
        ctx: &TraversalMethod_sum_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_sum_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_sum_Scope(
        &mut self,
        ctx: &TraversalMethod_sum_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Empty(
        &mut self,
        ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Scope(
        &mut self,
        ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_Scope_long(
        &mut self,
        ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tail_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tail_long(
        &mut self,
        ctx: &TraversalMethod_tail_longContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_timeLimit}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_timeLimit(
        &mut self,
        ctx: &TraversalMethod_timeLimitContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_times}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_times(
        &mut self,
        ctx: &TraversalMethod_timesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_Direction_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_Direction_String(
        &mut self,
        ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_String(
        &mut self,
        ctx: &TraversalMethod_to_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_to_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_to_Traversal(
        &mut self,
        ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_toE}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toE(
        &mut self,
        ctx: &TraversalMethod_toEContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toLower_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toLower_Empty(
        &mut self,
        ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toLower_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toLower_Scope(
        &mut self,
        ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toUpper_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toUpper_Empty(
        &mut self,
        ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_toUpper_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toUpper_Scope(
        &mut self,
        ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_toV}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_toV(
        &mut self,
        ctx: &TraversalMethod_toVContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tree_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tree_Empty(
        &mut self,
        ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_tree_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_tree_String(
        &mut self,
        ctx: &TraversalMethod_tree_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_trim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_trim_Empty(
        &mut self,
        ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_trim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_trim_Scope(
        &mut self,
        ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_unfold}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_unfold(
        &mut self,
        ctx: &TraversalMethod_unfoldContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_union}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_union(
        &mut self,
        ctx: &TraversalMethod_unionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_until_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_until_Predicate(
        &mut self,
        ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_until_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_until_Traversal(
        &mut self,
        ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_value}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_value(
        &mut self,
        ctx: &TraversalMethod_valueContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_valueMap_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_valueMap_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_valueMap_boolean_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_valueMap_boolean_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_values}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_values(
        &mut self,
        ctx: &TraversalMethod_valuesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_P(
        &mut self,
        ctx: &TraversalMethod_where_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_String_P(
        &mut self,
        ctx: &TraversalMethod_where_String_PContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_where_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_where_Traversal(
        &mut self,
        ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_with_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_with_String(
        &mut self,
        ctx: &TraversalMethod_with_StringContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by the {@code traversalMethod_with_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_with_String_Object(
        &mut self,
        ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMethod_write}.
     * @param ctx the parse tree
     */
    fn visit_traversalMethod_write(
        &mut self,
        ctx: &TraversalMethod_writeContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategy}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategy(&mut self, ctx: &TraversalStrategyContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#configuration}.
     * @param ctx the parse tree
     */
    fn visit_configuration(&mut self, ctx: &ConfigurationContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalScope}.
     * @param ctx the parse tree
     */
    fn visit_traversalScope(&mut self, ctx: &TraversalScopeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalBarrier}.
     * @param ctx the parse tree
     */
    fn visit_traversalBarrier(&mut self, ctx: &TraversalBarrierContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalT}.
     * @param ctx the parse tree
     */
    fn visit_traversalT(&mut self, ctx: &TraversalTContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTShort}.
     * @param ctx the parse tree
     */
    fn visit_traversalTShort(&mut self, ctx: &TraversalTShortContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTLong}.
     * @param ctx the parse tree
     */
    fn visit_traversalTLong(&mut self, ctx: &TraversalTLongContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalMerge}.
     * @param ctx the parse tree
     */
    fn visit_traversalMerge(&mut self, ctx: &TraversalMergeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalOrder}.
     * @param ctx the parse tree
     */
    fn visit_traversalOrder(&mut self, ctx: &TraversalOrderContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirection}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirection(
        &mut self,
        ctx: &TraversalDirectionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirectionShort}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirectionShort(
        &mut self,
        ctx: &TraversalDirectionShortContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDirectionLong}.
     * @param ctx the parse tree
     */
    fn visit_traversalDirectionLong(
        &mut self,
        ctx: &TraversalDirectionLongContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalCardinality}.
     * @param ctx the parse tree
     */
    fn visit_traversalCardinality(
        &mut self,
        ctx: &TraversalCardinalityContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalColumn}.
     * @param ctx the parse tree
     */
    fn visit_traversalColumn(&mut self, ctx: &TraversalColumnContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPop}.
     * @param ctx the parse tree
     */
    fn visit_traversalPop(&mut self, ctx: &TraversalPopContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalOperator}.
     * @param ctx the parse tree
     */
    fn visit_traversalOperator(&mut self, ctx: &TraversalOperatorContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPick}.
     * @param ctx the parse tree
     */
    fn visit_traversalPick(&mut self, ctx: &TraversalPickContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalDT}.
     * @param ctx the parse tree
     */
    fn visit_traversalDT(&mut self, ctx: &TraversalDTContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalGType}.
     * @param ctx the parse tree
     */
    fn visit_traversalGType(&mut self, ctx: &TraversalGTypeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate(
        &mut self,
        ctx: &TraversalPredicateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod(
        &mut self,
        ctx: &TraversalTerminalMethodContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalSackMethod}.
     * @param ctx the parse tree
     */
    fn visit_traversalSackMethod(
        &mut self,
        ctx: &TraversalSackMethodContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalComparator}.
     * @param ctx the parse tree
     */
    fn visit_traversalComparator(
        &mut self,
        ctx: &TraversalComparatorContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalFunction}.
     * @param ctx the parse tree
     */
    fn visit_traversalFunction(&mut self, ctx: &TraversalFunctionContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalBiFunction}.
     * @param ctx the parse tree
     */
    fn visit_traversalBiFunction(
        &mut self,
        ctx: &TraversalBiFunctionContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_eq}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_eq(
        &mut self,
        ctx: &TraversalPredicate_eqContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_neq}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_neq(
        &mut self,
        ctx: &TraversalPredicate_neqContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_typeOf}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_typeOf(
        &mut self,
        ctx: &TraversalPredicate_typeOfContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_lt}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_lt(
        &mut self,
        ctx: &TraversalPredicate_ltContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_lte}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_lte(
        &mut self,
        ctx: &TraversalPredicate_lteContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_gt}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_gt(
        &mut self,
        ctx: &TraversalPredicate_gtContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_gte}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_gte(
        &mut self,
        ctx: &TraversalPredicate_gteContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_inside}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_inside(
        &mut self,
        ctx: &TraversalPredicate_insideContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_outside}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_outside(
        &mut self,
        ctx: &TraversalPredicate_outsideContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_between}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_between(
        &mut self,
        ctx: &TraversalPredicate_betweenContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_within}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_within(
        &mut self,
        ctx: &TraversalPredicate_withinContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_without}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_without(
        &mut self,
        ctx: &TraversalPredicate_withoutContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_not}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_not(
        &mut self,
        ctx: &TraversalPredicate_notContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_containing}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_containing(
        &mut self,
        ctx: &TraversalPredicate_containingContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notContaining}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notContaining(
        &mut self,
        ctx: &TraversalPredicate_notContainingContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_startingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_startingWith(
        &mut self,
        ctx: &TraversalPredicate_startingWithContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notStartingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notStartingWith(
        &mut self,
        ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_endingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_endingWith(
        &mut self,
        ctx: &TraversalPredicate_endingWithContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notEndingWith}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notEndingWith(
        &mut self,
        ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_regex}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_regex(
        &mut self,
        ctx: &TraversalPredicate_regexContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalPredicate_notRegex}.
     * @param ctx the parse tree
     */
    fn visit_traversalPredicate_notRegex(
        &mut self,
        ctx: &TraversalPredicate_notRegexContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_explain}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_explain(
        &mut self,
        ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_hasNext}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_hasNext(
        &mut self,
        ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_iterate}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_iterate(
        &mut self,
        ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_tryNext}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_tryNext(
        &mut self,
        ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_next}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_next(
        &mut self,
        ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toList}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toList(
        &mut self,
        ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toSet}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toBulkSet}.
     * @param ctx the parse tree
     */
    fn visit_traversalTerminalMethod_toBulkSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionKeys}.
     * @param ctx the parse tree
     */
    fn visit_withOptionKeys(&mut self, ctx: &WithOptionKeysContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants(
        &mut self,
        ctx: &ConnectedComponentConstantsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants(&mut self, ctx: &PageRankConstantsContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants(
        &mut self,
        ctx: &PeerPressureConstantsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants(
        &mut self,
        ctx: &ShortestPathConstantsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsValues}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsValues(&mut self, ctx: &WithOptionsValuesContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsKeys}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsKeys(&mut self, ctx: &IoOptionsKeysContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsValues}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsValues(&mut self, ctx: &IoOptionsValuesContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_component}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_component(
        &mut self,
        ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_edges(
        &mut self,
        ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentConstants_propertyName(
        &mut self,
        ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_edges(
        &mut self,
        ctx: &PageRankConstants_edgesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_times}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_times(
        &mut self,
        ctx: &PageRankConstants_timesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_pageRankConstants_propertyName(
        &mut self,
        ctx: &PageRankConstants_propertyNameContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_edges(
        &mut self,
        ctx: &PeerPressureConstants_edgesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_times}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_times(
        &mut self,
        ctx: &PeerPressureConstants_timesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureConstants_propertyName(
        &mut self,
        ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_target}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_target(
        &mut self,
        ctx: &ShortestPathConstants_targetContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_edges}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_edges(
        &mut self,
        ctx: &ShortestPathConstants_edgesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_distance}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_distance(
        &mut self,
        ctx: &ShortestPathConstants_distanceContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_maxDistance}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_maxDistance(
        &mut self,
        ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathConstants_includeEdges}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathConstants_includeEdges(
        &mut self,
        ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_tokens}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_tokens(
        &mut self,
        ctx: &WithOptionsConstants_tokensContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_none}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_none(
        &mut self,
        ctx: &WithOptionsConstants_noneContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_ids}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_ids(
        &mut self,
        ctx: &WithOptionsConstants_idsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_labels}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_labels(
        &mut self,
        ctx: &WithOptionsConstants_labelsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_keys}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_keys(
        &mut self,
        ctx: &WithOptionsConstants_keysContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_values}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_values(
        &mut self,
        ctx: &WithOptionsConstants_valuesContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_all}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_all(
        &mut self,
        ctx: &WithOptionsConstants_allContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_indexer}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_indexer(
        &mut self,
        ctx: &WithOptionsConstants_indexerContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_list}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_list(
        &mut self,
        ctx: &WithOptionsConstants_listContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsConstants_map}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsConstants_map(
        &mut self,
        ctx: &WithOptionsConstants_mapContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_reader}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_reader(
        &mut self,
        ctx: &IoOptionsConstants_readerContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_writer}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_writer(
        &mut self,
        ctx: &IoOptionsConstants_writerContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_gryo}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_gryo(
        &mut self,
        ctx: &IoOptionsConstants_gryoContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphson}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_graphson(
        &mut self,
        ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphml}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsConstants_graphml(
        &mut self,
        ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#connectedComponentStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_connectedComponentStringConstant(
        &mut self,
        ctx: &ConnectedComponentStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#pageRankStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_pageRankStringConstant(
        &mut self,
        ctx: &PageRankStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#peerPressureStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_peerPressureStringConstant(
        &mut self,
        ctx: &PeerPressureStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#shortestPathStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_shortestPathStringConstant(
        &mut self,
        ctx: &ShortestPathStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#withOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_withOptionsStringConstant(
        &mut self,
        ctx: &WithOptionsStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#ioOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn visit_ioOptionsStringConstant(
        &mut self,
        ctx: &IoOptionsStringConstantContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#booleanArgument}.
     * @param ctx the parse tree
     */
    fn visit_booleanArgument(&mut self, ctx: &BooleanArgumentContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#integerArgument}.
     * @param ctx the parse tree
     */
    fn visit_integerArgument(&mut self, ctx: &IntegerArgumentContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringArgument}.
     * @param ctx the parse tree
     */
    fn visit_stringArgument(&mut self, ctx: &StringArgumentContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableArgument}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableArgument(
        &mut self,
        ctx: &StringNullableArgumentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableArgumentVarargs(
        &mut self,
        ctx: &StringNullableArgumentVarargsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#dateArgument}.
     * @param ctx the parse tree
     */
    fn visit_dateArgument(&mut self, ctx: &DateArgumentContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericArgument(&mut self, ctx: &GenericArgumentContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn visit_genericArgumentVarargs(
        &mut self,
        ctx: &GenericArgumentVarargsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericMapArgument(
        &mut self,
        ctx: &GenericMapArgumentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapNullableArgument}.
     * @param ctx the parse tree
     */
    fn visit_genericMapNullableArgument(
        &mut self,
        ctx: &GenericMapNullableArgumentContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategyVarargs}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategyVarargs(
        &mut self,
        ctx: &TraversalStrategyVarargsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#traversalStrategyExpr}.
     * @param ctx the parse tree
     */
    fn visit_traversalStrategyExpr(
        &mut self,
        ctx: &TraversalStrategyExprContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classTypeList}.
     * @param ctx the parse tree
     */
    fn visit_classTypeList(&mut self, ctx: &ClassTypeListContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classTypeExpr}.
     * @param ctx the parse tree
     */
    fn visit_classTypeExpr(&mut self, ctx: &ClassTypeExprContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversalList}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversalList(
        &mut self,
        ctx: &NestedTraversalListContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nestedTraversalExpr}.
     * @param ctx the parse tree
     */
    fn visit_nestedTraversalExpr(
        &mut self,
        ctx: &NestedTraversalExprContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericCollectionLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericCollectionLiteral(
        &mut self,
        ctx: &GenericCollectionLiteralContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteralVarargs(
        &mut self,
        ctx: &GenericLiteralVarargsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteralExpr}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteralExpr(
        &mut self,
        ctx: &GenericLiteralExprContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapNullableLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericMapNullableLiteral(
        &mut self,
        ctx: &GenericMapNullableLiteralContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericRangeLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericRangeLiteral(
        &mut self,
        ctx: &GenericRangeLiteralContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericSetLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericSetLiteral(&mut self, ctx: &GenericSetLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableLiteralVarargs(
        &mut self,
        ctx: &StringNullableLiteralVarargsContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericLiteral(&mut self, ctx: &GenericLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#genericMapLiteral}.
     * @param ctx the parse tree
     */
    fn visit_genericMapLiteral(&mut self, ctx: &GenericMapLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#mapKey}.
     * @param ctx the parse tree
     */
    fn visit_mapKey(&mut self, ctx: &MapKeyContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#mapEntry}.
     * @param ctx the parse tree
     */
    fn visit_mapEntry(&mut self, ctx: &MapEntryContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringLiteral}.
     * @param ctx the parse tree
     */
    fn visit_stringLiteral(&mut self, ctx: &StringLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#stringNullableLiteral}.
     * @param ctx the parse tree
     */
    fn visit_stringNullableLiteral(
        &mut self,
        ctx: &StringNullableLiteralContext<'input>,
    ) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#integerLiteral}.
     * @param ctx the parse tree
     */
    fn visit_integerLiteral(&mut self, ctx: &IntegerLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#floatLiteral}.
     * @param ctx the parse tree
     */
    fn visit_floatLiteral(&mut self, ctx: &FloatLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#numericLiteral}.
     * @param ctx the parse tree
     */
    fn visit_numericLiteral(&mut self, ctx: &NumericLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#booleanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_booleanLiteral(&mut self, ctx: &BooleanLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#dateLiteral}.
     * @param ctx the parse tree
     */
    fn visit_dateLiteral(&mut self, ctx: &DateLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nullLiteral}.
     * @param ctx the parse tree
     */
    fn visit_nullLiteral(&mut self, ctx: &NullLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nanLiteral}.
     * @param ctx the parse tree
     */
    fn visit_nanLiteral(&mut self, ctx: &NanLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#infLiteral}.
     * @param ctx the parse tree
     */
    fn visit_infLiteral(&mut self, ctx: &InfLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#uuidLiteral}.
     * @param ctx the parse tree
     */
    fn visit_uuidLiteral(&mut self, ctx: &UuidLiteralContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#nakedKey}.
     * @param ctx the parse tree
     */
    fn visit_nakedKey(&mut self, ctx: &NakedKeyContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#classType}.
     * @param ctx the parse tree
     */
    fn visit_classType(&mut self, ctx: &ClassTypeContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#variable}.
     * @param ctx the parse tree
     */
    fn visit_variable(&mut self, ctx: &VariableContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }

    /**
     * Visit a parse tree produced by {@link GremlinParser#keyword}.
     * @param ctx the parse tree
     */
    fn visit_keyword(&mut self, ctx: &KeywordContext<'input>) -> Self::Return {
        self.visit_children(ctx)
    }
}

impl<'input, T> GremlinVisitor<'input> for T
where
    T: GremlinVisitorCompat<'input>,
{
    fn visit_queryList(&mut self, ctx: &QueryListContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_queryList(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_query(&mut self, ctx: &QueryContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_query(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_emptyQuery(&mut self, ctx: &EmptyQueryContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_emptyQuery(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSource(&mut self, ctx: &TraversalSourceContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSource(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_transactionPart(&mut self, ctx: &TransactionPartContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_transactionPart(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_rootTraversal(&mut self, ctx: &RootTraversalContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_rootTraversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod(&mut self, ctx: &TraversalSourceSelfMethodContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withBulk(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withBulk(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withPath(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withPath(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withSack(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withSack(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withSideEffect(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withSideEffect(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withStrategies(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_withoutStrategies(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_withoutStrategies(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSelfMethod_with(
        &mut self,
        ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSelfMethod_with(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod(
        &mut self,
        ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_addE(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_addE(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_addV(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_addV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_E(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_E(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_V(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_V(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_inject(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_inject(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_io(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_io(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_mergeV_Map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_mergeV_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_mergeE_Map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_mergeE_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_call_empty(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_call_empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_call_string(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_call_string(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_call_string_map(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_call_string_map(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_call_string_traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_call_string_map_traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSourceSpawnMethod_union(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalSourceSpawnMethod_union(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_chainedTraversal(&mut self, ctx: &ChainedTraversalContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_chainedTraversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nestedTraversal(&mut self, ctx: &NestedTraversalContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nestedTraversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_terminatedTraversal(&mut self, ctx: &TerminatedTraversalContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_terminatedTraversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod(&mut self, ctx: &TraversalMethodContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_V(&mut self, ctx: &TraversalMethod_VContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_V(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_E(&mut self, ctx: &TraversalMethod_EContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_E(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_addE_String(
        &mut self,
        ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_addE_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_addE_Traversal(
        &mut self,
        ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_addE_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_addV_Empty(
        &mut self,
        ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_addV_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_addV_String(
        &mut self,
        ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_addV_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_addV_Traversal(
        &mut self,
        ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_addV_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_aggregate_String(
        &mut self,
        ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_aggregate_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_all_P(&mut self, ctx: &TraversalMethod_all_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_all_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_and(&mut self, ctx: &TraversalMethod_andContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_and(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_any_P(&mut self, ctx: &TraversalMethod_any_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_any_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_as(&mut self, ctx: &TraversalMethod_asContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_as(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asBool(&mut self, ctx: &TraversalMethod_asBoolContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_asBool(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asDate(&mut self, ctx: &TraversalMethod_asDateContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_asDate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asNumber_Empty(
        &mut self,
        ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_asNumber_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asNumber_traversalGType(
        &mut self,
        ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_asNumber_traversalGType(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asString_Empty(
        &mut self,
        ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_asString_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_asString_Scope(
        &mut self,
        ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_asString_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_barrier_Consumer(
        &mut self,
        ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_barrier_Consumer(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_barrier_Empty(
        &mut self,
        ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_barrier_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_barrier_int(
        &mut self,
        ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_barrier_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_both(&mut self, ctx: &TraversalMethod_bothContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_both(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_bothE(&mut self, ctx: &TraversalMethod_bothEContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_bothE(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_bothV(&mut self, ctx: &TraversalMethod_bothVContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_bothV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_branch(&mut self, ctx: &TraversalMethod_branchContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_branch(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Comparator(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Empty(&mut self, ctx: &TraversalMethod_by_EmptyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Function(
        &mut self,
        ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Function(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Function_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Function_Comparator(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Order(&mut self, ctx: &TraversalMethod_by_OrderContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Order(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_String(&mut self, ctx: &TraversalMethod_by_StringContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_String_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_by_String_Comparator(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_T(&mut self, ctx: &TraversalMethod_by_TContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_T(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Traversal(
        &mut self,
        ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_by_Traversal_Comparator(
        &mut self,
        ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_by_Traversal_Comparator(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_call_string(
        &mut self,
        ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_call_string(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_call_string_map(
        &mut self,
        ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_call_string_map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_call_string_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_call_string_traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_call_string_map_traversal(
        &mut self,
        ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_call_string_map_traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_cap(&mut self, ctx: &TraversalMethod_capContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_cap(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Function(
        &mut self,
        ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Function(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Predicate_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Predicate_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Predicate_Traversal_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Traversal_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_choose_Traversal_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_choose_Traversal_Traversal_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_coalesce(&mut self, ctx: &TraversalMethod_coalesceContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_coalesce(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_coin(&mut self, ctx: &TraversalMethod_coinContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_coin(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_combine_Object(
        &mut self,
        ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_combine_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_concat_Traversal_Traversal(
        &mut self,
        ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_concat_Traversal_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_concat_String(
        &mut self,
        ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_concat_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_conjoin_String(
        &mut self,
        ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_conjoin_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_connectedComponent(
        &mut self,
        ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_connectedComponent(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_constant(&mut self, ctx: &TraversalMethod_constantContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_constant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_count_Empty(
        &mut self,
        ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_count_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_count_Scope(
        &mut self,
        ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_count_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_cyclicPath(
        &mut self,
        ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_cyclicPath(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_dateAdd(&mut self, ctx: &TraversalMethod_dateAddContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_dateAdd(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_dateDiff_Traversal(
        &mut self,
        ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_dateDiff_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_dateDiff_Date(
        &mut self,
        ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_dateDiff_Date(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_dedup_Scope_String(
        &mut self,
        ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_dedup_Scope_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_dedup_String(
        &mut self,
        ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_dedup_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_difference_Object(
        &mut self,
        ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_difference_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_discard(&mut self, ctx: &TraversalMethod_discardContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_discard(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_disjunct_Object(
        &mut self,
        ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_disjunct_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_drop(&mut self, ctx: &TraversalMethod_dropContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_drop(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_element(&mut self, ctx: &TraversalMethod_elementContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_element(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_elementMap(
        &mut self,
        ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_elementMap(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_emit_Empty(
        &mut self,
        ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_emit_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_emit_Predicate(
        &mut self,
        ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_emit_Predicate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_emit_Traversal(
        &mut self,
        ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_emit_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_fail_Empty(
        &mut self,
        ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_fail_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_fail_String(
        &mut self,
        ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_fail_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_filter_Predicate(
        &mut self,
        ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_filter_Predicate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_filter_Traversal(
        &mut self,
        ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_filter_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_flatMap(&mut self, ctx: &TraversalMethod_flatMapContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_flatMap(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_fold_Empty(
        &mut self,
        ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_fold_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_fold_Object_BiFunction(
        &mut self,
        ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_fold_Object_BiFunction(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_format_String(
        &mut self,
        ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_format_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_from_String(
        &mut self,
        ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_from_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_from_Traversal(
        &mut self,
        ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_from_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_group_Empty(
        &mut self,
        ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_group_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_group_String(
        &mut self,
        ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_group_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_groupCount_Empty(
        &mut self,
        ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_groupCount_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_groupCount_String(
        &mut self,
        ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_groupCount_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_String(
        &mut self,
        ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_has_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_has_String_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_has_String_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_String_String_Object(
        &mut self,
        ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_has_String_String_Object(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_String_String_P(
        &mut self,
        ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_has_String_String_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_T_Object(
        &mut self,
        ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_has_T_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_has_T_P(&mut self, ctx: &TraversalMethod_has_T_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_has_T_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasId_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_hasId_Object_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasId_P(&mut self, ctx: &TraversalMethod_hasId_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_hasId_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasKey_P(&mut self, ctx: &TraversalMethod_hasKey_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_hasKey_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasKey_String_String(
        &mut self,
        ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_hasKey_String_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasLabel_P(
        &mut self,
        ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_hasLabel_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasLabel_String_String(
        &mut self,
        ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_hasLabel_String_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasNot(&mut self, ctx: &TraversalMethod_hasNotContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_hasNot(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasValue_Object_Object(
        &mut self,
        ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_hasValue_Object_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_hasValue_P(
        &mut self,
        ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_hasValue_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_id(&mut self, ctx: &TraversalMethod_idContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_id(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_identity(&mut self, ctx: &TraversalMethod_identityContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_identity(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_in(&mut self, ctx: &TraversalMethod_inContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_in(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_inE(&mut self, ctx: &TraversalMethod_inEContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_inE(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_intersect_Object(
        &mut self,
        ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_intersect_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_inV(&mut self, ctx: &TraversalMethod_inVContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_inV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_index(&mut self, ctx: &TraversalMethod_indexContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_index(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_inject(&mut self, ctx: &TraversalMethod_injectContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_inject(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_is_Object(&mut self, ctx: &TraversalMethod_is_ObjectContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_is_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_is_P(&mut self, ctx: &TraversalMethod_is_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_is_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_key(&mut self, ctx: &TraversalMethod_keyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_key(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_label(&mut self, ctx: &TraversalMethod_labelContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_label(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_length_Empty(
        &mut self,
        ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_length_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_length_Scope(
        &mut self,
        ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_length_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_limit_Scope_long(
        &mut self,
        ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_limit_Scope_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_limit_long(
        &mut self,
        ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_limit_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_local(&mut self, ctx: &TraversalMethod_localContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_local(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_loops_Empty(
        &mut self,
        ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_loops_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_loops_String(
        &mut self,
        ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_loops_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_lTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_lTrim_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_lTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_lTrim_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_map(&mut self, ctx: &TraversalMethod_mapContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_match(&mut self, ctx: &TraversalMethod_matchContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_match(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_math(&mut self, ctx: &TraversalMethod_mathContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_math(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_max_Empty(&mut self, ctx: &TraversalMethod_max_EmptyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_max_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_max_Scope(&mut self, ctx: &TraversalMethod_max_ScopeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_max_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mean_Empty(
        &mut self,
        ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mean_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mean_Scope(
        &mut self,
        ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mean_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_merge_Object(
        &mut self,
        ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_merge_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeV_empty(
        &mut self,
        ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeV_empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeV_Map(
        &mut self,
        ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeV_Map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeV_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeV_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeE_empty(
        &mut self,
        ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeE_empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeE_Map(
        &mut self,
        ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeE_Map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_mergeE_Traversal(
        &mut self,
        ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_mergeE_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_min_Empty(&mut self, ctx: &TraversalMethod_min_EmptyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_min_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_min_Scope(&mut self, ctx: &TraversalMethod_min_ScopeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_min_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_none_P(&mut self, ctx: &TraversalMethod_none_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_none_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_not(&mut self, ctx: &TraversalMethod_notContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_not(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Predicate_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Predicate_Traversal(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Merge_Map(
        &mut self,
        ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Merge_Map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Merge_Map_Cardinality(
        &mut self,
        ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Merge_Map_Cardinality(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Merge_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Merge_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Object_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Object_Traversal(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_option_Traversal(
        &mut self,
        ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_option_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_optional(&mut self, ctx: &TraversalMethod_optionalContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_optional(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_or(&mut self, ctx: &TraversalMethod_orContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_or(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_order_Empty(
        &mut self,
        ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_order_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_order_Scope(
        &mut self,
        ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_order_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_otherV(&mut self, ctx: &TraversalMethod_otherVContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_otherV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_out(&mut self, ctx: &TraversalMethod_outContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_out(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_outE(&mut self, ctx: &TraversalMethod_outEContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_outE(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_outV(&mut self, ctx: &TraversalMethod_outVContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_outV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_pageRank_Empty(
        &mut self,
        ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_pageRank_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_pageRank_double(
        &mut self,
        ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_pageRank_double(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_path(&mut self, ctx: &TraversalMethod_pathContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_path(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_peerPressure(
        &mut self,
        ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_peerPressure(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_product_Object(
        &mut self,
        ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_product_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_profile_Empty(
        &mut self,
        ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_profile_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_profile_String(
        &mut self,
        ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_profile_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_project(&mut self, ctx: &TraversalMethod_projectContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_project(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_properties(
        &mut self,
        ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_properties(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_property_Cardinality_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_property_Cardinality_Object_Object_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_property_Cardinality_Object(
        &mut self,
        ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_property_Cardinality_Object(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_property_Object_Object_Object(
        &mut self,
        ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_property_Object_Object_Object(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_property_Object(
        &mut self,
        ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_property_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_propertyMap(
        &mut self,
        ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_propertyMap(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_range_Scope_long_long(
        &mut self,
        ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_range_Scope_long_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_range_long_long(
        &mut self,
        ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_range_long_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_read(&mut self, ctx: &TraversalMethod_readContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_read(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_repeat_String_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_repeat_String_Traversal(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_repeat_Traversal(
        &mut self,
        ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_repeat_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_replace_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_replace_String_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_replace_Scope_String_String(
        &mut self,
        ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_replace_Scope_String_String(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_reverse_Empty(
        &mut self,
        ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_reverse_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_rTrim_Empty(
        &mut self,
        ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_rTrim_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_rTrim_Scope(
        &mut self,
        ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_rTrim_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sack_BiFunction(
        &mut self,
        ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_sack_BiFunction(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sack_Empty(
        &mut self,
        ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_sack_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sample_Scope_int(
        &mut self,
        ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_sample_Scope_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sample_int(
        &mut self,
        ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_sample_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_Column(
        &mut self,
        ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_select_Column(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_Pop_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_select_Pop_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_Pop_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_select_Pop_String_String_String(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_Pop_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_select_Pop_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_String(
        &mut self,
        ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_select_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_String_String_String(
        &mut self,
        ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_select_String_String_String(
                self, ctx,
            );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_select_Traversal(
        &mut self,
        ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_select_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_shortestPath(
        &mut self,
        ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_shortestPath(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sideEffect(
        &mut self,
        ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_sideEffect(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_simplePath(
        &mut self,
        ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_simplePath(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_skip_Scope_long(
        &mut self,
        ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_skip_Scope_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_skip_long(&mut self, ctx: &TraversalMethod_skip_longContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_skip_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_split_String(
        &mut self,
        ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_split_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_split_Scope_String(
        &mut self,
        ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_split_Scope_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_subgraph(&mut self, ctx: &TraversalMethod_subgraphContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_subgraph(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_substring_int(
        &mut self,
        ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_substring_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_substring_Scope_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_substring_Scope_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_substring_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_substring_int_int(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_substring_Scope_int_int(
        &mut self,
        ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_substring_Scope_int_int(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sum_Empty(&mut self, ctx: &TraversalMethod_sum_EmptyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_sum_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_sum_Scope(&mut self, ctx: &TraversalMethod_sum_ScopeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_sum_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tail_Empty(
        &mut self,
        ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_tail_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tail_Scope(
        &mut self,
        ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_tail_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tail_Scope_long(
        &mut self,
        ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_tail_Scope_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tail_long(&mut self, ctx: &TraversalMethod_tail_longContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_tail_long(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_timeLimit(&mut self, ctx: &TraversalMethod_timeLimitContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_timeLimit(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_times(&mut self, ctx: &TraversalMethod_timesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_times(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_to_Direction_String(
        &mut self,
        ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_to_Direction_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_to_String(&mut self, ctx: &TraversalMethod_to_StringContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_to_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_to_Traversal(
        &mut self,
        ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_to_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toE(&mut self, ctx: &TraversalMethod_toEContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toE(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toLower_Empty(
        &mut self,
        ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toLower_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toLower_Scope(
        &mut self,
        ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toLower_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toUpper_Empty(
        &mut self,
        ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toUpper_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toUpper_Scope(
        &mut self,
        ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toUpper_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_toV(&mut self, ctx: &TraversalMethod_toVContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_toV(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tree_Empty(
        &mut self,
        ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_tree_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_tree_String(
        &mut self,
        ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_tree_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_trim_Empty(
        &mut self,
        ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_trim_Empty(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_trim_Scope(
        &mut self,
        ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_trim_Scope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_unfold(&mut self, ctx: &TraversalMethod_unfoldContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_unfold(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_union(&mut self, ctx: &TraversalMethod_unionContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_union(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_until_Predicate(
        &mut self,
        ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_until_Predicate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_until_Traversal(
        &mut self,
        ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_until_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_value(&mut self, ctx: &TraversalMethod_valueContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_value(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_valueMap_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_valueMap_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_valueMap_boolean_String(
        &mut self,
        ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_valueMap_boolean_String(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_values(&mut self, ctx: &TraversalMethod_valuesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_values(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_where_P(&mut self, ctx: &TraversalMethod_where_PContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_where_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_where_String_P(
        &mut self,
        ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_where_String_P(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_where_Traversal(
        &mut self,
        ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_where_Traversal(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_with_String(
        &mut self,
        ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_with_String(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_with_String_Object(
        &mut self,
        ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalMethod_with_String_Object(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMethod_write(&mut self, ctx: &TraversalMethod_writeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMethod_write(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalStrategy(&mut self, ctx: &TraversalStrategyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalStrategy(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_configuration(&mut self, ctx: &ConfigurationContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_configuration(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalScope(&mut self, ctx: &TraversalScopeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalScope(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalBarrier(&mut self, ctx: &TraversalBarrierContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalBarrier(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalT(&mut self, ctx: &TraversalTContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalT(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTShort(&mut self, ctx: &TraversalTShortContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalTShort(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTLong(&mut self, ctx: &TraversalTLongContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalTLong(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalMerge(&mut self, ctx: &TraversalMergeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalMerge(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalOrder(&mut self, ctx: &TraversalOrderContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalOrder(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalDirection(&mut self, ctx: &TraversalDirectionContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalDirection(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalDirectionShort(&mut self, ctx: &TraversalDirectionShortContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalDirectionShort(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalDirectionLong(&mut self, ctx: &TraversalDirectionLongContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalDirectionLong(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalCardinality(&mut self, ctx: &TraversalCardinalityContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalCardinality(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalColumn(&mut self, ctx: &TraversalColumnContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalColumn(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPop(&mut self, ctx: &TraversalPopContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPop(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalOperator(&mut self, ctx: &TraversalOperatorContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalOperator(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPick(&mut self, ctx: &TraversalPickContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPick(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalDT(&mut self, ctx: &TraversalDTContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalDT(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalGType(&mut self, ctx: &TraversalGTypeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalGType(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate(&mut self, ctx: &TraversalPredicateContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod(&mut self, ctx: &TraversalTerminalMethodContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalSackMethod(&mut self, ctx: &TraversalSackMethodContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalSackMethod(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalComparator(&mut self, ctx: &TraversalComparatorContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalComparator(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalFunction(&mut self, ctx: &TraversalFunctionContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalFunction(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalBiFunction(&mut self, ctx: &TraversalBiFunctionContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalBiFunction(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_eq(&mut self, ctx: &TraversalPredicate_eqContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_eq(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_neq(&mut self, ctx: &TraversalPredicate_neqContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_neq(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_typeOf(&mut self, ctx: &TraversalPredicate_typeOfContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_typeOf(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_lt(&mut self, ctx: &TraversalPredicate_ltContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_lt(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_lte(&mut self, ctx: &TraversalPredicate_lteContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_lte(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_gt(&mut self, ctx: &TraversalPredicate_gtContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_gt(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_gte(&mut self, ctx: &TraversalPredicate_gteContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_gte(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_inside(&mut self, ctx: &TraversalPredicate_insideContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_inside(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_outside(
        &mut self,
        ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_outside(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_between(
        &mut self,
        ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_between(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_within(&mut self, ctx: &TraversalPredicate_withinContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_within(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_without(
        &mut self,
        ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_without(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_not(&mut self, ctx: &TraversalPredicate_notContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_not(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_containing(
        &mut self,
        ctx: &TraversalPredicate_containingContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_containing(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_notContaining(
        &mut self,
        ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalPredicate_notContaining(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_startingWith(
        &mut self,
        ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalPredicate_startingWith(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_notStartingWith(
        &mut self,
        ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalPredicate_notStartingWith(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_endingWith(
        &mut self,
        ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_endingWith(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_notEndingWith(
        &mut self,
        ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalPredicate_notEndingWith(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_regex(&mut self, ctx: &TraversalPredicate_regexContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_regex(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalPredicate_notRegex(
        &mut self,
        ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalPredicate_notRegex(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_explain(
        &mut self,
        ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_explain(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_hasNext(
        &mut self,
        ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_hasNext(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_iterate(
        &mut self,
        ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_iterate(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_tryNext(
        &mut self,
        ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_tryNext(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_next(
        &mut self,
        ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_next(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_toList(
        &mut self,
        ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_toList(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_toSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_toSet(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalTerminalMethod_toBulkSet(
        &mut self,
        ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_traversalTerminalMethod_toBulkSet(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionKeys(&mut self, ctx: &WithOptionKeysContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionKeys(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_connectedComponentConstants(
        &mut self,
        ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_connectedComponentConstants(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_pageRankConstants(&mut self, ctx: &PageRankConstantsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_pageRankConstants(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_peerPressureConstants(&mut self, ctx: &PeerPressureConstantsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_peerPressureConstants(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants(&mut self, ctx: &ShortestPathConstantsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_shortestPathConstants(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsValues(&mut self, ctx: &WithOptionsValuesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsValues(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsKeys(&mut self, ctx: &IoOptionsKeysContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsKeys(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsValues(&mut self, ctx: &IoOptionsValuesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsValues(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_connectedComponentConstants_component(
        &mut self,
        ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_connectedComponentConstants_component(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_connectedComponentConstants_edges(
        &mut self,
        ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_connectedComponentConstants_edges(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_connectedComponentConstants_propertyName(
        &mut self,
        ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_connectedComponentConstants_propertyName(
            self, ctx,
        );
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_pageRankConstants_edges(&mut self, ctx: &PageRankConstants_edgesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_pageRankConstants_edges(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_pageRankConstants_times(&mut self, ctx: &PageRankConstants_timesContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_pageRankConstants_times(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_pageRankConstants_propertyName(
        &mut self,
        ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_pageRankConstants_propertyName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_peerPressureConstants_edges(
        &mut self,
        ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_peerPressureConstants_edges(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_peerPressureConstants_times(
        &mut self,
        ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_peerPressureConstants_times(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_peerPressureConstants_propertyName(
        &mut self,
        ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_peerPressureConstants_propertyName(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants_target(
        &mut self,
        ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_shortestPathConstants_target(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants_edges(
        &mut self,
        ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_shortestPathConstants_edges(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants_distance(
        &mut self,
        ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_shortestPathConstants_distance(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants_maxDistance(
        &mut self,
        ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_shortestPathConstants_maxDistance(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathConstants_includeEdges(
        &mut self,
        ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_shortestPathConstants_includeEdges(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_tokens(
        &mut self,
        ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_tokens(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_none(&mut self, ctx: &WithOptionsConstants_noneContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_none(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_ids(&mut self, ctx: &WithOptionsConstants_idsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_ids(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_labels(
        &mut self,
        ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_labels(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_keys(&mut self, ctx: &WithOptionsConstants_keysContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_keys(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_values(
        &mut self,
        ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_values(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_all(&mut self, ctx: &WithOptionsConstants_allContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_all(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_indexer(
        &mut self,
        ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_indexer(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_list(&mut self, ctx: &WithOptionsConstants_listContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_list(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsConstants_map(&mut self, ctx: &WithOptionsConstants_mapContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsConstants_map(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsConstants_reader(&mut self, ctx: &IoOptionsConstants_readerContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsConstants_reader(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsConstants_writer(&mut self, ctx: &IoOptionsConstants_writerContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsConstants_writer(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsConstants_gryo(&mut self, ctx: &IoOptionsConstants_gryoContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsConstants_gryo(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsConstants_graphson(
        &mut self,
        ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsConstants_graphson(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsConstants_graphml(
        &mut self,
        ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsConstants_graphml(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_connectedComponentStringConstant(
        &mut self,
        ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
        let result =
            <Self as GremlinVisitorCompat>::visit_connectedComponentStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_pageRankStringConstant(&mut self, ctx: &PageRankStringConstantContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_pageRankStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_peerPressureStringConstant(
        &mut self,
        ctx: &PeerPressureStringConstantContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_peerPressureStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_shortestPathStringConstant(
        &mut self,
        ctx: &ShortestPathStringConstantContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_shortestPathStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_withOptionsStringConstant(&mut self, ctx: &WithOptionsStringConstantContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_withOptionsStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_ioOptionsStringConstant(&mut self, ctx: &IoOptionsStringConstantContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_ioOptionsStringConstant(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_booleanArgument(&mut self, ctx: &BooleanArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_booleanArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_integerArgument(&mut self, ctx: &IntegerArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_integerArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringArgument(&mut self, ctx: &StringArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_stringArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringNullableArgument(&mut self, ctx: &StringNullableArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_stringNullableArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringNullableArgumentVarargs(
        &mut self,
        ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_stringNullableArgumentVarargs(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_dateArgument(&mut self, ctx: &DateArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_dateArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericArgument(&mut self, ctx: &GenericArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericArgumentVarargs(&mut self, ctx: &GenericArgumentVarargsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericArgumentVarargs(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericMapArgument(&mut self, ctx: &GenericMapArgumentContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericMapArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericMapNullableArgument(
        &mut self,
        ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_genericMapNullableArgument(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalStrategyVarargs(&mut self, ctx: &TraversalStrategyVarargsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalStrategyVarargs(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_traversalStrategyExpr(&mut self, ctx: &TraversalStrategyExprContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_traversalStrategyExpr(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_classTypeList(&mut self, ctx: &ClassTypeListContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_classTypeList(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_classTypeExpr(&mut self, ctx: &ClassTypeExprContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_classTypeExpr(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nestedTraversalList(&mut self, ctx: &NestedTraversalListContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nestedTraversalList(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nestedTraversalExpr(&mut self, ctx: &NestedTraversalExprContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nestedTraversalExpr(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericCollectionLiteral(&mut self, ctx: &GenericCollectionLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericCollectionLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericLiteralVarargs(&mut self, ctx: &GenericLiteralVarargsContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericLiteralVarargs(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericLiteralExpr(&mut self, ctx: &GenericLiteralExprContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericLiteralExpr(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericMapNullableLiteral(&mut self, ctx: &GenericMapNullableLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericMapNullableLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericRangeLiteral(&mut self, ctx: &GenericRangeLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericRangeLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericSetLiteral(&mut self, ctx: &GenericSetLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericSetLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringNullableLiteralVarargs(
        &mut self,
        ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
        let result = <Self as GremlinVisitorCompat>::visit_stringNullableLiteralVarargs(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericLiteral(&mut self, ctx: &GenericLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_genericMapLiteral(&mut self, ctx: &GenericMapLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_genericMapLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_mapKey(&mut self, ctx: &MapKeyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_mapKey(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_mapEntry(&mut self, ctx: &MapEntryContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_mapEntry(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringLiteral(&mut self, ctx: &StringLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_stringLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_stringNullableLiteral(&mut self, ctx: &StringNullableLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_stringNullableLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_integerLiteral(&mut self, ctx: &IntegerLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_integerLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_floatLiteral(&mut self, ctx: &FloatLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_floatLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_numericLiteral(&mut self, ctx: &NumericLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_numericLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_booleanLiteral(&mut self, ctx: &BooleanLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_booleanLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_dateLiteral(&mut self, ctx: &DateLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_dateLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nullLiteral(&mut self, ctx: &NullLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nullLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nanLiteral(&mut self, ctx: &NanLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nanLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_infLiteral(&mut self, ctx: &InfLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_infLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_uuidLiteral(&mut self, ctx: &UuidLiteralContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_uuidLiteral(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_nakedKey(&mut self, ctx: &NakedKeyContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_nakedKey(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_classType(&mut self, ctx: &ClassTypeContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_classType(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_variable(&mut self, ctx: &VariableContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_variable(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }

    fn visit_keyword(&mut self, ctx: &KeywordContext<'input>) {
        let result = <Self as GremlinVisitorCompat>::visit_keyword(self, ctx);
        *<Self as ParseTreeVisitorCompat>::temp_result(self) = result;
    }
}
