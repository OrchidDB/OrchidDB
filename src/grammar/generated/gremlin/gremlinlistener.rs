#![allow(nonstandard_style)]
// Generated from languages/gremlin/Gremlin.g4 by ANTLR 4.13.2
use super::gremlinparser::*;
use antlr4rust::tree::ParseTreeListener;

pub trait GremlinListener<'input>: ParseTreeListener<'input, GremlinParserContextType> {
    /**
     * Enter a parse tree produced by {@link GremlinParser#queryList}.
     * @param ctx the parse tree
     */
    fn enter_queryList(&mut self, _ctx: &QueryListContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#queryList}.
     * @param ctx the parse tree
     */
    fn exit_queryList(&mut self, _ctx: &QueryListContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#query}.
     * @param ctx the parse tree
     */
    fn enter_query(&mut self, _ctx: &QueryContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#query}.
     * @param ctx the parse tree
     */
    fn exit_query(&mut self, _ctx: &QueryContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#emptyQuery}.
     * @param ctx the parse tree
     */
    fn enter_emptyQuery(&mut self, _ctx: &EmptyQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#emptyQuery}.
     * @param ctx the parse tree
     */
    fn exit_emptyQuery(&mut self, _ctx: &EmptyQueryContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSource}.
     * @param ctx the parse tree
     */
    fn enter_traversalSource(&mut self, _ctx: &TraversalSourceContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSource}.
     * @param ctx the parse tree
     */
    fn exit_traversalSource(&mut self, _ctx: &TraversalSourceContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#transactionPart}.
     * @param ctx the parse tree
     */
    fn enter_transactionPart(&mut self, _ctx: &TransactionPartContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#transactionPart}.
     * @param ctx the parse tree
     */
    fn exit_transactionPart(&mut self, _ctx: &TransactionPartContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#rootTraversal}.
     * @param ctx the parse tree
     */
    fn enter_rootTraversal(&mut self, _ctx: &RootTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#rootTraversal}.
     * @param ctx the parse tree
     */
    fn exit_rootTraversal(&mut self, _ctx: &RootTraversalContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod(&mut self, _ctx: &TraversalSourceSelfMethodContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod(&mut self, _ctx: &TraversalSourceSelfMethodContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withBulk}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withBulk(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withBulk}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withBulk(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withPath}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withPath(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withPath}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withPath(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSack}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withSack(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSack}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withSack(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSideEffect}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withSideEffect(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withSideEffect}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withSideEffect(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withStrategies}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withStrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withStrategies}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withStrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withoutStrategies}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_withoutStrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_withoutStrategies}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_withoutStrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_with}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSelfMethod_with(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSelfMethod_with}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSelfMethod_with(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod(
        &mut self,
        _ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod(
        &mut self,
        _ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addE}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_addE(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addE}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_addE(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addV}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_addV(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_addV}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_addV(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_E}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_E(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_E}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_E(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_V}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_V(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_V}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_V(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_inject}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_inject(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_inject}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_inject(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_io}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_io(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_io}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_io(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_mergeV_Map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_mergeV_Map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_mergeV_Traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_mergeV_Traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_mergeE_Map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_mergeE_Map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_mergeE_Traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_mergeE_Traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_call_empty}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_call_empty(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_call_empty}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_call_empty(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_call_string(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_call_string(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_call_string_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_call_string_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalSourceSpawnMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalSourceSpawnMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_union}.
     * @param ctx the parse tree
     */
    fn enter_traversalSourceSpawnMethod_union(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSourceSpawnMethod_union}.
     * @param ctx the parse tree
     */
    fn exit_traversalSourceSpawnMethod_union(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#chainedTraversal}.
     * @param ctx the parse tree
     */
    fn enter_chainedTraversal(&mut self, _ctx: &ChainedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#chainedTraversal}.
     * @param ctx the parse tree
     */
    fn exit_chainedTraversal(&mut self, _ctx: &ChainedTraversalContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nestedTraversal}.
     * @param ctx the parse tree
     */
    fn enter_nestedTraversal(&mut self, _ctx: &NestedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nestedTraversal}.
     * @param ctx the parse tree
     */
    fn exit_nestedTraversal(&mut self, _ctx: &NestedTraversalContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#terminatedTraversal}.
     * @param ctx the parse tree
     */
    fn enter_terminatedTraversal(&mut self, _ctx: &TerminatedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#terminatedTraversal}.
     * @param ctx the parse tree
     */
    fn exit_terminatedTraversal(&mut self, _ctx: &TerminatedTraversalContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod(&mut self, _ctx: &TraversalMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod(&mut self, _ctx: &TraversalMethodContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_V}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_V(&mut self, _ctx: &TraversalMethod_VContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_V}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_V(&mut self, _ctx: &TraversalMethod_VContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_E}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_E(&mut self, _ctx: &TraversalMethod_EContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_E}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_E(&mut self, _ctx: &TraversalMethod_EContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_addE_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_addE_String(
        &mut self,
        _ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_addE_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_addE_String(
        &mut self,
        _ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_addE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_addE_Traversal(
        &mut self,
        _ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_addE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_addE_Traversal(
        &mut self,
        _ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_addV_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_addV_Empty(
        &mut self,
        _ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_addV_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_addV_Empty(
        &mut self,
        _ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_addV_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_addV_String(
        &mut self,
        _ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_addV_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_addV_String(
        &mut self,
        _ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_addV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_addV_Traversal(
        &mut self,
        _ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_addV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_addV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_addV_Traversal(
        &mut self,
        _ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_aggregate_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_aggregate}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_aggregate_String(
        &mut self,
        _ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_aggregate_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_aggregate}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_aggregate_String(
        &mut self,
        _ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_all_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_all}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_all_P(&mut self, _ctx: &TraversalMethod_all_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_all_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_all}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_all_P(&mut self, _ctx: &TraversalMethod_all_PContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_and}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_and(&mut self, _ctx: &TraversalMethod_andContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_and}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_and(&mut self, _ctx: &TraversalMethod_andContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_any_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_any}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_any_P(&mut self, _ctx: &TraversalMethod_any_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_any_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_any}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_any_P(&mut self, _ctx: &TraversalMethod_any_PContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_as}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_as(&mut self, _ctx: &TraversalMethod_asContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_as}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_as(&mut self, _ctx: &TraversalMethod_asContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_asBool}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asBool(&mut self, _ctx: &TraversalMethod_asBoolContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_asBool}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asBool(&mut self, _ctx: &TraversalMethod_asBoolContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_asDate}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asDate(&mut self, _ctx: &TraversalMethod_asDateContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_asDate}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asDate(&mut self, _ctx: &TraversalMethod_asDateContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_asNumber_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asNumber_Empty(
        &mut self,
        _ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_asNumber_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asNumber_Empty(
        &mut self,
        _ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_asNumber_traversalGType}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asNumber_traversalGType(
        &mut self,
        _ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_asNumber_traversalGType}
     * labeled alternative in {@link GremlinParser#traversalMethod_asNumber}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asNumber_traversalGType(
        &mut self,
        _ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_asString_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asString_Empty(
        &mut self,
        _ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_asString_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asString_Empty(
        &mut self,
        _ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_asString_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_asString_Scope(
        &mut self,
        _ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_asString_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_asString}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_asString_Scope(
        &mut self,
        _ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_barrier_Consumer}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_barrier_Consumer(
        &mut self,
        _ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_barrier_Consumer}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_barrier_Consumer(
        &mut self,
        _ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_barrier_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_barrier_Empty(
        &mut self,
        _ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_barrier_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_barrier_Empty(
        &mut self,
        _ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_barrier_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_barrier_int(
        &mut self,
        _ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_barrier_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_barrier}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_barrier_int(
        &mut self,
        _ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_both}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_both(&mut self, _ctx: &TraversalMethod_bothContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_both}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_both(&mut self, _ctx: &TraversalMethod_bothContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_bothE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_bothE(&mut self, _ctx: &TraversalMethod_bothEContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_bothE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_bothE(&mut self, _ctx: &TraversalMethod_bothEContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_bothV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_bothV(&mut self, _ctx: &TraversalMethod_bothVContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_bothV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_bothV(&mut self, _ctx: &TraversalMethod_bothVContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_branch}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_branch(&mut self, _ctx: &TraversalMethod_branchContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_branch}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_branch(&mut self, _ctx: &TraversalMethod_branchContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Empty(&mut self, _ctx: &TraversalMethod_by_EmptyContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Empty(&mut self, _ctx: &TraversalMethod_by_EmptyContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Function(
        &mut self,
        _ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Function(
        &mut self,
        _ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Function_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Function_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Function_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Function_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Order}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Order(&mut self, _ctx: &TraversalMethod_by_OrderContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Order}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Order(&mut self, _ctx: &TraversalMethod_by_OrderContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_String(&mut self, _ctx: &TraversalMethod_by_StringContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_String(&mut self, _ctx: &TraversalMethod_by_StringContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_String_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_String_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_String_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_String_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_T}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_T(&mut self, _ctx: &TraversalMethod_by_TContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_T}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_T(&mut self, _ctx: &TraversalMethod_by_TContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Traversal(
        &mut self,
        _ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Traversal(
        &mut self,
        _ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_by_Traversal_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_by_Traversal_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_by_Traversal_Comparator}
     * labeled alternative in {@link GremlinParser#traversalMethod_by}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_by_Traversal_Comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_call_string(
        &mut self,
        _ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_call_string}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_call_string(
        &mut self,
        _ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_call_string_map(
        &mut self,
        _ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_call_string_map}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_call_string_map(
        &mut self,
        _ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_call_string_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_call_string_map_traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_call}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_cap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_cap(&mut self, _ctx: &TraversalMethod_capContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_cap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_cap(&mut self, _ctx: &TraversalMethod_capContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Function(
        &mut self,
        _ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Function}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Function(
        &mut self,
        _ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Predicate_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Predicate_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Predicate_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Predicate_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Predicate_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_choose_Traversal_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_choose_Traversal_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_choose}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_choose_Traversal_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_coalesce}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_coalesce(&mut self, _ctx: &TraversalMethod_coalesceContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_coalesce}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_coalesce(&mut self, _ctx: &TraversalMethod_coalesceContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_coin}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_coin(&mut self, _ctx: &TraversalMethod_coinContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_coin}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_coin(&mut self, _ctx: &TraversalMethod_coinContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_combine_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_combine}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_combine_Object(
        &mut self,
        _ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_combine_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_combine}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_combine_Object(
        &mut self,
        _ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_concat_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_concat_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_concat_Traversal_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_concat_Traversal_Traversal(
        &mut self,
        _ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_concat_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_concat_String(
        &mut self,
        _ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_concat_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_concat}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_concat_String(
        &mut self,
        _ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_conjoin_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_conjoin}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_conjoin_String(
        &mut self,
        _ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_conjoin_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_conjoin}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_conjoin_String(
        &mut self,
        _ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_connectedComponent}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_connectedComponent(
        &mut self,
        _ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_connectedComponent}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_connectedComponent(
        &mut self,
        _ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_constant}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_constant(&mut self, _ctx: &TraversalMethod_constantContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_constant}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_constant(&mut self, _ctx: &TraversalMethod_constantContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_count_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_count_Empty(
        &mut self,
        _ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_count_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_count_Empty(
        &mut self,
        _ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_count_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_count_Scope(
        &mut self,
        _ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_count_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_count}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_count_Scope(
        &mut self,
        _ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_cyclicPath}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_cyclicPath(
        &mut self,
        _ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_cyclicPath}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_cyclicPath(
        &mut self,
        _ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_dateAdd}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_dateAdd(&mut self, _ctx: &TraversalMethod_dateAddContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_dateAdd}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_dateAdd(&mut self, _ctx: &TraversalMethod_dateAddContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_dateDiff_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_dateDiff_Traversal(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_dateDiff_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_dateDiff_Traversal(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_dateDiff_Date}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_dateDiff_Date(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_dateDiff_Date}
     * labeled alternative in {@link GremlinParser#traversalMethod_dateDiff}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_dateDiff_Date(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_dedup_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_dedup_Scope_String(
        &mut self,
        _ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_dedup_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_dedup_Scope_String(
        &mut self,
        _ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_dedup_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_dedup_String(
        &mut self,
        _ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_dedup_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_dedup}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_dedup_String(
        &mut self,
        _ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_difference_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_difference}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_difference_Object(
        &mut self,
        _ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_difference_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_difference}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_difference_Object(
        &mut self,
        _ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_discard}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_discard(&mut self, _ctx: &TraversalMethod_discardContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_discard}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_discard(&mut self, _ctx: &TraversalMethod_discardContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_disjunct_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_disjunct}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_disjunct_Object(
        &mut self,
        _ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_disjunct_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_disjunct}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_disjunct_Object(
        &mut self,
        _ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_drop}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_drop(&mut self, _ctx: &TraversalMethod_dropContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_drop}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_drop(&mut self, _ctx: &TraversalMethod_dropContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_element}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_element(&mut self, _ctx: &TraversalMethod_elementContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_element}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_element(&mut self, _ctx: &TraversalMethod_elementContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_elementMap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_elementMap(
        &mut self,
        _ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_elementMap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_elementMap(
        &mut self,
        _ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_emit_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_emit_Empty(
        &mut self,
        _ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_emit_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_emit_Empty(
        &mut self,
        _ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_emit_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_emit_Predicate(
        &mut self,
        _ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_emit_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_emit_Predicate(
        &mut self,
        _ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_emit_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_emit_Traversal(
        &mut self,
        _ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_emit_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_emit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_emit_Traversal(
        &mut self,
        _ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_fail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_fail_Empty(
        &mut self,
        _ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_fail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_fail_Empty(
        &mut self,
        _ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_fail_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_fail_String(
        &mut self,
        _ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_fail_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_fail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_fail_String(
        &mut self,
        _ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_filter_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_filter_Predicate(
        &mut self,
        _ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_filter_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_filter_Predicate(
        &mut self,
        _ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_filter_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_filter_Traversal(
        &mut self,
        _ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_filter_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_filter}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_filter_Traversal(
        &mut self,
        _ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_flatMap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_flatMap(&mut self, _ctx: &TraversalMethod_flatMapContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_flatMap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_flatMap(&mut self, _ctx: &TraversalMethod_flatMapContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_fold_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_fold_Empty(
        &mut self,
        _ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_fold_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_fold_Empty(
        &mut self,
        _ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_fold_Object_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_fold_Object_BiFunction(
        &mut self,
        _ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_fold_Object_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_fold}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_fold_Object_BiFunction(
        &mut self,
        _ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_format_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_format}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_format_String(
        &mut self,
        _ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_format_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_format}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_format_String(
        &mut self,
        _ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_from_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_from_String(
        &mut self,
        _ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_from_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_from_String(
        &mut self,
        _ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_from_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_from_Traversal(
        &mut self,
        _ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_from_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_from}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_from_Traversal(
        &mut self,
        _ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_group_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_group_Empty(
        &mut self,
        _ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_group_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_group_Empty(
        &mut self,
        _ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_group_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_group_String(
        &mut self,
        _ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_group_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_group}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_group_String(
        &mut self,
        _ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_groupCount_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_groupCount_Empty(
        &mut self,
        _ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_groupCount_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_groupCount_Empty(
        &mut self,
        _ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_groupCount_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_groupCount_String(
        &mut self,
        _ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_groupCount_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_groupCount}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_groupCount_String(
        &mut self,
        _ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_String(
        &mut self,
        _ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_String(
        &mut self,
        _ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_String_Object(
        &mut self,
        _ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_String_Object(
        &mut self,
        _ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_String_P(
        &mut self,
        _ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_String_P(
        &mut self,
        _ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_String_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_String_String_Object(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_String_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_String_String_Object(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_String_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_String_String_P(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_String_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_String_String_P(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_T_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_T_Object(
        &mut self,
        _ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_T_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_T_Object(
        &mut self,
        _ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_has_T_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_has_T_P(&mut self, _ctx: &TraversalMethod_has_T_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_has_T_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_has}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_has_T_P(&mut self, _ctx: &TraversalMethod_has_T_PContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasId_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasId_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasId_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasId_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasId_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasId_P(&mut self, _ctx: &TraversalMethod_hasId_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasId_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasId}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasId_P(&mut self, _ctx: &TraversalMethod_hasId_PContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasKey_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasKey_P(&mut self, _ctx: &TraversalMethod_hasKey_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasKey_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasKey_P(&mut self, _ctx: &TraversalMethod_hasKey_PContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasKey_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasKey_String_String(
        &mut self,
        _ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasKey_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasKey}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasKey_String_String(
        &mut self,
        _ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasLabel_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasLabel_P(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasLabel_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasLabel_P(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasLabel_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasLabel_String_String(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasLabel_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasLabel}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasLabel_String_String(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_hasNot}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasNot(&mut self, _ctx: &TraversalMethod_hasNotContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_hasNot}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasNot(&mut self, _ctx: &TraversalMethod_hasNotContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasValue_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasValue_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasValue_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasValue_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_hasValue_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_hasValue_P(
        &mut self,
        _ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_hasValue_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_hasValue}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_hasValue_P(
        &mut self,
        _ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_id}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_id(&mut self, _ctx: &TraversalMethod_idContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_id}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_id(&mut self, _ctx: &TraversalMethod_idContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_identity}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_identity(&mut self, _ctx: &TraversalMethod_identityContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_identity}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_identity(&mut self, _ctx: &TraversalMethod_identityContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_in}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_in(&mut self, _ctx: &TraversalMethod_inContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_in}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_in(&mut self, _ctx: &TraversalMethod_inContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_inE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_inE(&mut self, _ctx: &TraversalMethod_inEContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_inE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_inE(&mut self, _ctx: &TraversalMethod_inEContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_intersect_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_intersect}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_intersect_Object(
        &mut self,
        _ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_intersect_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_intersect}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_intersect_Object(
        &mut self,
        _ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_inV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_inV(&mut self, _ctx: &TraversalMethod_inVContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_inV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_inV(&mut self, _ctx: &TraversalMethod_inVContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_index}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_index(&mut self, _ctx: &TraversalMethod_indexContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_index}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_index(&mut self, _ctx: &TraversalMethod_indexContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_inject}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_inject(&mut self, _ctx: &TraversalMethod_injectContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_inject}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_inject(&mut self, _ctx: &TraversalMethod_injectContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_is_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_is_Object(&mut self, _ctx: &TraversalMethod_is_ObjectContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_is_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_is_Object(&mut self, _ctx: &TraversalMethod_is_ObjectContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_is_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_is_P(&mut self, _ctx: &TraversalMethod_is_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_is_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_is}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_is_P(&mut self, _ctx: &TraversalMethod_is_PContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_key}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_key(&mut self, _ctx: &TraversalMethod_keyContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_key}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_key(&mut self, _ctx: &TraversalMethod_keyContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_label}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_label(&mut self, _ctx: &TraversalMethod_labelContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_label}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_label(&mut self, _ctx: &TraversalMethod_labelContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_length_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_length_Empty(
        &mut self,
        _ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_length_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_length_Empty(
        &mut self,
        _ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_length_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_length_Scope(
        &mut self,
        _ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_length_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_length}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_length_Scope(
        &mut self,
        _ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_limit_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_limit_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_limit_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_limit_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_limit_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_limit_long(
        &mut self,
        _ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_limit_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_limit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_limit_long(
        &mut self,
        _ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_local}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_local(&mut self, _ctx: &TraversalMethod_localContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_local}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_local(&mut self, _ctx: &TraversalMethod_localContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_loops_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_loops_Empty(
        &mut self,
        _ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_loops_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_loops_Empty(
        &mut self,
        _ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_loops_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_loops_String(
        &mut self,
        _ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_loops_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_loops}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_loops_String(
        &mut self,
        _ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_lTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_lTrim_Empty(
        &mut self,
        _ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_lTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_lTrim_Empty(
        &mut self,
        _ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_lTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_lTrim_Scope(
        &mut self,
        _ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_lTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_lTrim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_lTrim_Scope(
        &mut self,
        _ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_map}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_map(&mut self, _ctx: &TraversalMethod_mapContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_map}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_map(&mut self, _ctx: &TraversalMethod_mapContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_match}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_match(&mut self, _ctx: &TraversalMethod_matchContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_match}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_match(&mut self, _ctx: &TraversalMethod_matchContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_math}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_math(&mut self, _ctx: &TraversalMethod_mathContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_math}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_math(&mut self, _ctx: &TraversalMethod_mathContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_max_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_max_Empty(&mut self, _ctx: &TraversalMethod_max_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_max_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_max_Empty(&mut self, _ctx: &TraversalMethod_max_EmptyContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_max_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_max_Scope(&mut self, _ctx: &TraversalMethod_max_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_max_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_max}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_max_Scope(&mut self, _ctx: &TraversalMethod_max_ScopeContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mean_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mean_Empty(
        &mut self,
        _ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mean_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mean_Empty(
        &mut self,
        _ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mean_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mean_Scope(
        &mut self,
        _ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mean_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_mean}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mean_Scope(
        &mut self,
        _ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_merge_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_merge}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_merge_Object(
        &mut self,
        _ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_merge_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_merge}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_merge_Object(
        &mut self,
        _ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeV_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeV_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeV_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeV_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeV_Map(
        &mut self,
        _ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeV_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeV_Map(
        &mut self,
        _ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeV_Traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeV_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeV_Traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeE_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeE_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeE_empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeE_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeE_Map(
        &mut self,
        _ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeE_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeE_Map(
        &mut self,
        _ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_mergeE_Traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_mergeE_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_mergeE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_mergeE_Traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_min_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_min_Empty(&mut self, _ctx: &TraversalMethod_min_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_min_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_min_Empty(&mut self, _ctx: &TraversalMethod_min_EmptyContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_min_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_min_Scope(&mut self, _ctx: &TraversalMethod_min_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_min_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_min}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_min_Scope(&mut self, _ctx: &TraversalMethod_min_ScopeContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_none_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_none}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_none_P(&mut self, _ctx: &TraversalMethod_none_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_none_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_none}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_none_P(&mut self, _ctx: &TraversalMethod_none_PContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_not}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_not(&mut self, _ctx: &TraversalMethod_notContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_not}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_not(&mut self, _ctx: &TraversalMethod_notContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Predicate_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Predicate_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Predicate_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Merge_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Merge_Map(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Merge_Map}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Merge_Map(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Merge_Map_Cardinality}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Merge_Map_Cardinality(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Merge_Map_Cardinality}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Merge_Map_Cardinality(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Merge_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Merge_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Merge_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Merge_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Object_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Object_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Object_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Object_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_option_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_option_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_option_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_option}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_option_Traversal(
        &mut self,
        _ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_optional}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_optional(&mut self, _ctx: &TraversalMethod_optionalContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_optional}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_optional(&mut self, _ctx: &TraversalMethod_optionalContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_or}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_or(&mut self, _ctx: &TraversalMethod_orContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_or}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_or(&mut self, _ctx: &TraversalMethod_orContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_order_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_order_Empty(
        &mut self,
        _ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_order_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_order_Empty(
        &mut self,
        _ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_order_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_order_Scope(
        &mut self,
        _ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_order_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_order}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_order_Scope(
        &mut self,
        _ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_otherV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_otherV(&mut self, _ctx: &TraversalMethod_otherVContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_otherV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_otherV(&mut self, _ctx: &TraversalMethod_otherVContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_out}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_out(&mut self, _ctx: &TraversalMethod_outContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_out}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_out(&mut self, _ctx: &TraversalMethod_outContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_outE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_outE(&mut self, _ctx: &TraversalMethod_outEContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_outE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_outE(&mut self, _ctx: &TraversalMethod_outEContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_outV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_outV(&mut self, _ctx: &TraversalMethod_outVContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_outV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_outV(&mut self, _ctx: &TraversalMethod_outVContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_pageRank_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_pageRank_Empty(
        &mut self,
        _ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_pageRank_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_pageRank_Empty(
        &mut self,
        _ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_pageRank_double}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_pageRank_double(
        &mut self,
        _ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_pageRank_double}
     * labeled alternative in {@link GremlinParser#traversalMethod_pageRank}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_pageRank_double(
        &mut self,
        _ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_path}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_path(&mut self, _ctx: &TraversalMethod_pathContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_path}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_path(&mut self, _ctx: &TraversalMethod_pathContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_peerPressure}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_peerPressure(
        &mut self,
        _ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_peerPressure}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_peerPressure(
        &mut self,
        _ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_product_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_product}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_product_Object(
        &mut self,
        _ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_product_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_product}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_product_Object(
        &mut self,
        _ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_profile_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_profile_Empty(
        &mut self,
        _ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_profile_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_profile_Empty(
        &mut self,
        _ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_profile_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_profile_String(
        &mut self,
        _ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_profile_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_profile}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_profile_String(
        &mut self,
        _ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_project}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_project(&mut self, _ctx: &TraversalMethod_projectContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_project}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_project(&mut self, _ctx: &TraversalMethod_projectContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_properties}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_properties(
        &mut self,
        _ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_properties}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_properties(
        &mut self,
        _ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_property_Cardinality_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_property_Cardinality_Object_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_property_Cardinality_Object_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_property_Cardinality_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_property_Cardinality_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_property_Cardinality_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_property_Cardinality_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_property_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_property_Object_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_property_Object_Object_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_property_Object_Object_Object(
        &mut self,
        _ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_property_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_property_Object(
        &mut self,
        _ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_property_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_property}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_property_Object(
        &mut self,
        _ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_propertyMap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_propertyMap(
        &mut self,
        _ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_propertyMap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_propertyMap(
        &mut self,
        _ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_range_Scope_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_range_Scope_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_range_Scope_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_range_Scope_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_range_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_range_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_range_long_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_range}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_range_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_read}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_read(&mut self, _ctx: &TraversalMethod_readContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_read}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_read(&mut self, _ctx: &TraversalMethod_readContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_repeat_String_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_repeat_String_Traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_repeat_String_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_repeat_String_Traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_repeat_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_repeat_Traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_repeat_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_repeat}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_repeat_Traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_replace_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_replace_String_String(
        &mut self,
        _ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_replace_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_replace_String_String(
        &mut self,
        _ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_replace_Scope_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_replace_Scope_String_String(
        &mut self,
        _ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_replace_Scope_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_replace}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_replace_Scope_String_String(
        &mut self,
        _ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_reverse_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_reverse}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_reverse_Empty(
        &mut self,
        _ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_reverse_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_reverse}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_reverse_Empty(
        &mut self,
        _ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_rTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_rTrim_Empty(
        &mut self,
        _ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_rTrim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_rTrim_Empty(
        &mut self,
        _ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_rTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_rTrim_Scope(
        &mut self,
        _ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_rTrim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_rTrim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_rTrim_Scope(
        &mut self,
        _ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sack_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sack_BiFunction(
        &mut self,
        _ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sack_BiFunction}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sack_BiFunction(
        &mut self,
        _ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sack_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sack_Empty(
        &mut self,
        _ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sack_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sack}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sack_Empty(
        &mut self,
        _ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sample_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sample_Scope_int(
        &mut self,
        _ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sample_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sample_Scope_int(
        &mut self,
        _ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sample_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sample_int(
        &mut self,
        _ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sample_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_sample}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sample_int(
        &mut self,
        _ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_Column}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_Column(
        &mut self,
        _ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_Column}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_Column(
        &mut self,
        _ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_Pop_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_Pop_String(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_Pop_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_Pop_String(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_Pop_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_Pop_String_String_String(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_Pop_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_Pop_String_String_String(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_Pop_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_Pop_Traversal(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_Pop_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_Pop_Traversal(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_String(
        &mut self,
        _ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_String(
        &mut self,
        _ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_String_String_String(
        &mut self,
        _ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_String_String_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_String_String_String(
        &mut self,
        _ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_select_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_select_Traversal(
        &mut self,
        _ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_select_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_select}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_select_Traversal(
        &mut self,
        _ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_shortestPath}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_shortestPath(
        &mut self,
        _ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_shortestPath}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_shortestPath(
        &mut self,
        _ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_sideEffect}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sideEffect(
        &mut self,
        _ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_sideEffect}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sideEffect(
        &mut self,
        _ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_simplePath}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_simplePath(
        &mut self,
        _ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_simplePath}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_simplePath(
        &mut self,
        _ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_skip_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_skip_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_skip_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_skip_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_skip_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_skip_long(&mut self, _ctx: &TraversalMethod_skip_longContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_skip_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_skip}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_skip_long(&mut self, _ctx: &TraversalMethod_skip_longContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_split_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_split_String(
        &mut self,
        _ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_split_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_split_String(
        &mut self,
        _ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_split_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_split_Scope_String(
        &mut self,
        _ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_split_Scope_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_split}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_split_Scope_String(
        &mut self,
        _ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_subgraph}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_subgraph(&mut self, _ctx: &TraversalMethod_subgraphContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_subgraph}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_subgraph(&mut self, _ctx: &TraversalMethod_subgraphContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_substring_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_substring_int(
        &mut self,
        _ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_substring_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_substring_int(
        &mut self,
        _ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_substring_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_substring_Scope_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_substring_Scope_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_substring_Scope_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_substring_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_substring_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_substring_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_substring_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_substring_Scope_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_substring_Scope_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_substring_Scope_int_int}
     * labeled alternative in {@link GremlinParser#traversalMethod_substring}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_substring_Scope_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sum_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sum_Empty(&mut self, _ctx: &TraversalMethod_sum_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sum_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sum_Empty(&mut self, _ctx: &TraversalMethod_sum_EmptyContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_sum_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_sum_Scope(&mut self, _ctx: &TraversalMethod_sum_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_sum_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_sum}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_sum_Scope(&mut self, _ctx: &TraversalMethod_sum_ScopeContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tail_Empty(
        &mut self,
        _ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tail_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tail_Empty(
        &mut self,
        _ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tail_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tail_Scope(
        &mut self,
        _ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tail_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tail_Scope(
        &mut self,
        _ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tail_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tail_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tail_Scope_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tail_Scope_long(
        &mut self,
        _ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tail_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tail_long(&mut self, _ctx: &TraversalMethod_tail_longContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tail_long}
     * labeled alternative in {@link GremlinParser#traversalMethod_tail}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tail_long(&mut self, _ctx: &TraversalMethod_tail_longContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_timeLimit}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_timeLimit(&mut self, _ctx: &TraversalMethod_timeLimitContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_timeLimit}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_timeLimit(&mut self, _ctx: &TraversalMethod_timeLimitContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_times}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_times(&mut self, _ctx: &TraversalMethod_timesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_times}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_times(&mut self, _ctx: &TraversalMethod_timesContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_to_Direction_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_to_Direction_String(
        &mut self,
        _ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_to_Direction_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_to_Direction_String(
        &mut self,
        _ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_to_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_to_String(&mut self, _ctx: &TraversalMethod_to_StringContext<'input>) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_to_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_to_String(&mut self, _ctx: &TraversalMethod_to_StringContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_to_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_to_Traversal(
        &mut self,
        _ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_to_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_to}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_to_Traversal(
        &mut self,
        _ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_toE}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toE(&mut self, _ctx: &TraversalMethod_toEContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_toE}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toE(&mut self, _ctx: &TraversalMethod_toEContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_toLower_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toLower_Empty(
        &mut self,
        _ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_toLower_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toLower_Empty(
        &mut self,
        _ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_toLower_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toLower_Scope(
        &mut self,
        _ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_toLower_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toLower}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toLower_Scope(
        &mut self,
        _ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_toUpper_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toUpper_Empty(
        &mut self,
        _ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_toUpper_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toUpper_Empty(
        &mut self,
        _ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_toUpper_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toUpper_Scope(
        &mut self,
        _ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_toUpper_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_toUpper}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toUpper_Scope(
        &mut self,
        _ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_toV}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_toV(&mut self, _ctx: &TraversalMethod_toVContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_toV}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_toV(&mut self, _ctx: &TraversalMethod_toVContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tree_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tree_Empty(
        &mut self,
        _ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tree_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tree_Empty(
        &mut self,
        _ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_tree_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_tree_String(
        &mut self,
        _ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_tree_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_tree}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_tree_String(
        &mut self,
        _ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_trim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_trim_Empty(
        &mut self,
        _ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_trim_Empty}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_trim_Empty(
        &mut self,
        _ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_trim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_trim_Scope(
        &mut self,
        _ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_trim_Scope}
     * labeled alternative in {@link GremlinParser#traversalMethod_trim}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_trim_Scope(
        &mut self,
        _ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_unfold}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_unfold(&mut self, _ctx: &TraversalMethod_unfoldContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_unfold}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_unfold(&mut self, _ctx: &TraversalMethod_unfoldContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_union}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_union(&mut self, _ctx: &TraversalMethod_unionContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_union}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_union(&mut self, _ctx: &TraversalMethod_unionContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_until_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_until_Predicate(
        &mut self,
        _ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_until_Predicate}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_until_Predicate(
        &mut self,
        _ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_until_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_until_Traversal(
        &mut self,
        _ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_until_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_until}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_until_Traversal(
        &mut self,
        _ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_value}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_value(&mut self, _ctx: &TraversalMethod_valueContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_value}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_value(&mut self, _ctx: &TraversalMethod_valueContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_valueMap_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_valueMap_String(
        &mut self,
        _ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_valueMap_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_valueMap_String(
        &mut self,
        _ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_valueMap_boolean_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_valueMap_boolean_String(
        &mut self,
        _ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_valueMap_boolean_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_valueMap}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_valueMap_boolean_String(
        &mut self,
        _ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_values}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_values(&mut self, _ctx: &TraversalMethod_valuesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_values}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_values(&mut self, _ctx: &TraversalMethod_valuesContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_where_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_where_P(&mut self, _ctx: &TraversalMethod_where_PContext<'input>) {}
    /**
     * Exit a parse tree produced by the {@code traversalMethod_where_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_where_P(&mut self, _ctx: &TraversalMethod_where_PContext<'input>) {}
    /**
     * Enter a parse tree produced by the {@code traversalMethod_where_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_where_String_P(
        &mut self,
        _ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_where_String_P}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_where_String_P(
        &mut self,
        _ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_where_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_where_Traversal(
        &mut self,
        _ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_where_Traversal}
     * labeled alternative in {@link GremlinParser#traversalMethod_where}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_where_Traversal(
        &mut self,
        _ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_with_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_with_String(
        &mut self,
        _ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_with_String}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_with_String(
        &mut self,
        _ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by the {@code traversalMethod_with_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_with_String_Object(
        &mut self,
        _ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by the {@code traversalMethod_with_String_Object}
     * labeled alternative in {@link GremlinParser#traversalMethod_with}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_with_String_Object(
        &mut self,
        _ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMethod_write}.
     * @param ctx the parse tree
     */
    fn enter_traversalMethod_write(&mut self, _ctx: &TraversalMethod_writeContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMethod_write}.
     * @param ctx the parse tree
     */
    fn exit_traversalMethod_write(&mut self, _ctx: &TraversalMethod_writeContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalStrategy}.
     * @param ctx the parse tree
     */
    fn enter_traversalStrategy(&mut self, _ctx: &TraversalStrategyContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalStrategy}.
     * @param ctx the parse tree
     */
    fn exit_traversalStrategy(&mut self, _ctx: &TraversalStrategyContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#configuration}.
     * @param ctx the parse tree
     */
    fn enter_configuration(&mut self, _ctx: &ConfigurationContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#configuration}.
     * @param ctx the parse tree
     */
    fn exit_configuration(&mut self, _ctx: &ConfigurationContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalScope}.
     * @param ctx the parse tree
     */
    fn enter_traversalScope(&mut self, _ctx: &TraversalScopeContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalScope}.
     * @param ctx the parse tree
     */
    fn exit_traversalScope(&mut self, _ctx: &TraversalScopeContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalBarrier}.
     * @param ctx the parse tree
     */
    fn enter_traversalBarrier(&mut self, _ctx: &TraversalBarrierContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalBarrier}.
     * @param ctx the parse tree
     */
    fn exit_traversalBarrier(&mut self, _ctx: &TraversalBarrierContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalT}.
     * @param ctx the parse tree
     */
    fn enter_traversalT(&mut self, _ctx: &TraversalTContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalT}.
     * @param ctx the parse tree
     */
    fn exit_traversalT(&mut self, _ctx: &TraversalTContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTShort}.
     * @param ctx the parse tree
     */
    fn enter_traversalTShort(&mut self, _ctx: &TraversalTShortContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTShort}.
     * @param ctx the parse tree
     */
    fn exit_traversalTShort(&mut self, _ctx: &TraversalTShortContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTLong}.
     * @param ctx the parse tree
     */
    fn enter_traversalTLong(&mut self, _ctx: &TraversalTLongContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTLong}.
     * @param ctx the parse tree
     */
    fn exit_traversalTLong(&mut self, _ctx: &TraversalTLongContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalMerge}.
     * @param ctx the parse tree
     */
    fn enter_traversalMerge(&mut self, _ctx: &TraversalMergeContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalMerge}.
     * @param ctx the parse tree
     */
    fn exit_traversalMerge(&mut self, _ctx: &TraversalMergeContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalOrder}.
     * @param ctx the parse tree
     */
    fn enter_traversalOrder(&mut self, _ctx: &TraversalOrderContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalOrder}.
     * @param ctx the parse tree
     */
    fn exit_traversalOrder(&mut self, _ctx: &TraversalOrderContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalDirection}.
     * @param ctx the parse tree
     */
    fn enter_traversalDirection(&mut self, _ctx: &TraversalDirectionContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalDirection}.
     * @param ctx the parse tree
     */
    fn exit_traversalDirection(&mut self, _ctx: &TraversalDirectionContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalDirectionShort}.
     * @param ctx the parse tree
     */
    fn enter_traversalDirectionShort(&mut self, _ctx: &TraversalDirectionShortContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalDirectionShort}.
     * @param ctx the parse tree
     */
    fn exit_traversalDirectionShort(&mut self, _ctx: &TraversalDirectionShortContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalDirectionLong}.
     * @param ctx the parse tree
     */
    fn enter_traversalDirectionLong(&mut self, _ctx: &TraversalDirectionLongContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalDirectionLong}.
     * @param ctx the parse tree
     */
    fn exit_traversalDirectionLong(&mut self, _ctx: &TraversalDirectionLongContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalCardinality}.
     * @param ctx the parse tree
     */
    fn enter_traversalCardinality(&mut self, _ctx: &TraversalCardinalityContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalCardinality}.
     * @param ctx the parse tree
     */
    fn exit_traversalCardinality(&mut self, _ctx: &TraversalCardinalityContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalColumn}.
     * @param ctx the parse tree
     */
    fn enter_traversalColumn(&mut self, _ctx: &TraversalColumnContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalColumn}.
     * @param ctx the parse tree
     */
    fn exit_traversalColumn(&mut self, _ctx: &TraversalColumnContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPop}.
     * @param ctx the parse tree
     */
    fn enter_traversalPop(&mut self, _ctx: &TraversalPopContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPop}.
     * @param ctx the parse tree
     */
    fn exit_traversalPop(&mut self, _ctx: &TraversalPopContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalOperator}.
     * @param ctx the parse tree
     */
    fn enter_traversalOperator(&mut self, _ctx: &TraversalOperatorContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalOperator}.
     * @param ctx the parse tree
     */
    fn exit_traversalOperator(&mut self, _ctx: &TraversalOperatorContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPick}.
     * @param ctx the parse tree
     */
    fn enter_traversalPick(&mut self, _ctx: &TraversalPickContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPick}.
     * @param ctx the parse tree
     */
    fn exit_traversalPick(&mut self, _ctx: &TraversalPickContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalDT}.
     * @param ctx the parse tree
     */
    fn enter_traversalDT(&mut self, _ctx: &TraversalDTContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalDT}.
     * @param ctx the parse tree
     */
    fn exit_traversalDT(&mut self, _ctx: &TraversalDTContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalGType}.
     * @param ctx the parse tree
     */
    fn enter_traversalGType(&mut self, _ctx: &TraversalGTypeContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalGType}.
     * @param ctx the parse tree
     */
    fn exit_traversalGType(&mut self, _ctx: &TraversalGTypeContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate(&mut self, _ctx: &TraversalPredicateContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate(&mut self, _ctx: &TraversalPredicateContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod(&mut self, _ctx: &TraversalTerminalMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod(&mut self, _ctx: &TraversalTerminalMethodContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalSackMethod}.
     * @param ctx the parse tree
     */
    fn enter_traversalSackMethod(&mut self, _ctx: &TraversalSackMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalSackMethod}.
     * @param ctx the parse tree
     */
    fn exit_traversalSackMethod(&mut self, _ctx: &TraversalSackMethodContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalComparator}.
     * @param ctx the parse tree
     */
    fn enter_traversalComparator(&mut self, _ctx: &TraversalComparatorContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalComparator}.
     * @param ctx the parse tree
     */
    fn exit_traversalComparator(&mut self, _ctx: &TraversalComparatorContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalFunction}.
     * @param ctx the parse tree
     */
    fn enter_traversalFunction(&mut self, _ctx: &TraversalFunctionContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalFunction}.
     * @param ctx the parse tree
     */
    fn exit_traversalFunction(&mut self, _ctx: &TraversalFunctionContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalBiFunction}.
     * @param ctx the parse tree
     */
    fn enter_traversalBiFunction(&mut self, _ctx: &TraversalBiFunctionContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalBiFunction}.
     * @param ctx the parse tree
     */
    fn exit_traversalBiFunction(&mut self, _ctx: &TraversalBiFunctionContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_eq}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_eq(&mut self, _ctx: &TraversalPredicate_eqContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_eq}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_eq(&mut self, _ctx: &TraversalPredicate_eqContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_neq}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_neq(&mut self, _ctx: &TraversalPredicate_neqContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_neq}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_neq(&mut self, _ctx: &TraversalPredicate_neqContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_typeOf}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_typeOf(&mut self, _ctx: &TraversalPredicate_typeOfContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_typeOf}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_typeOf(&mut self, _ctx: &TraversalPredicate_typeOfContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_lt}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_lt(&mut self, _ctx: &TraversalPredicate_ltContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_lt}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_lt(&mut self, _ctx: &TraversalPredicate_ltContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_lte}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_lte(&mut self, _ctx: &TraversalPredicate_lteContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_lte}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_lte(&mut self, _ctx: &TraversalPredicate_lteContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_gt}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_gt(&mut self, _ctx: &TraversalPredicate_gtContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_gt}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_gt(&mut self, _ctx: &TraversalPredicate_gtContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_gte}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_gte(&mut self, _ctx: &TraversalPredicate_gteContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_gte}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_gte(&mut self, _ctx: &TraversalPredicate_gteContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_inside}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_inside(&mut self, _ctx: &TraversalPredicate_insideContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_inside}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_inside(&mut self, _ctx: &TraversalPredicate_insideContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_outside}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_outside(
        &mut self,
        _ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_outside}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_outside(
        &mut self,
        _ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_between}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_between(
        &mut self,
        _ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_between}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_between(
        &mut self,
        _ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_within}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_within(&mut self, _ctx: &TraversalPredicate_withinContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_within}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_within(&mut self, _ctx: &TraversalPredicate_withinContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_without}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_without(
        &mut self,
        _ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_without}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_without(
        &mut self,
        _ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_not}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_not(&mut self, _ctx: &TraversalPredicate_notContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_not}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_not(&mut self, _ctx: &TraversalPredicate_notContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_containing}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_containing(
        &mut self,
        _ctx: &TraversalPredicate_containingContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_containing}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_containing(
        &mut self,
        _ctx: &TraversalPredicate_containingContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_notContaining}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_notContaining(
        &mut self,
        _ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_notContaining}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_notContaining(
        &mut self,
        _ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_startingWith}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_startingWith(
        &mut self,
        _ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_startingWith}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_startingWith(
        &mut self,
        _ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_notStartingWith}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_notStartingWith(
        &mut self,
        _ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_notStartingWith}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_notStartingWith(
        &mut self,
        _ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_endingWith}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_endingWith(
        &mut self,
        _ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_endingWith}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_endingWith(
        &mut self,
        _ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_notEndingWith}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_notEndingWith(
        &mut self,
        _ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_notEndingWith}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_notEndingWith(
        &mut self,
        _ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_regex}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_regex(&mut self, _ctx: &TraversalPredicate_regexContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_regex}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_regex(&mut self, _ctx: &TraversalPredicate_regexContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalPredicate_notRegex}.
     * @param ctx the parse tree
     */
    fn enter_traversalPredicate_notRegex(
        &mut self,
        _ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalPredicate_notRegex}.
     * @param ctx the parse tree
     */
    fn exit_traversalPredicate_notRegex(
        &mut self,
        _ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_explain}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_explain(
        &mut self,
        _ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_explain}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_explain(
        &mut self,
        _ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_hasNext}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_hasNext(
        &mut self,
        _ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_hasNext}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_hasNext(
        &mut self,
        _ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_iterate}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_iterate(
        &mut self,
        _ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_iterate}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_iterate(
        &mut self,
        _ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_tryNext}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_tryNext(
        &mut self,
        _ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_tryNext}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_tryNext(
        &mut self,
        _ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_next}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_next(
        &mut self,
        _ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_next}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_next(
        &mut self,
        _ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toList}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_toList(
        &mut self,
        _ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toList}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_toList(
        &mut self,
        _ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toSet}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_toSet(
        &mut self,
        _ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toSet}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_toSet(
        &mut self,
        _ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toBulkSet}.
     * @param ctx the parse tree
     */
    fn enter_traversalTerminalMethod_toBulkSet(
        &mut self,
        _ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalTerminalMethod_toBulkSet}.
     * @param ctx the parse tree
     */
    fn exit_traversalTerminalMethod_toBulkSet(
        &mut self,
        _ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionKeys}.
     * @param ctx the parse tree
     */
    fn enter_withOptionKeys(&mut self, _ctx: &WithOptionKeysContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionKeys}.
     * @param ctx the parse tree
     */
    fn exit_withOptionKeys(&mut self, _ctx: &WithOptionKeysContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#connectedComponentConstants}.
     * @param ctx the parse tree
     */
    fn enter_connectedComponentConstants(
        &mut self,
        _ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#connectedComponentConstants}.
     * @param ctx the parse tree
     */
    fn exit_connectedComponentConstants(
        &mut self,
        _ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#pageRankConstants}.
     * @param ctx the parse tree
     */
    fn enter_pageRankConstants(&mut self, _ctx: &PageRankConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#pageRankConstants}.
     * @param ctx the parse tree
     */
    fn exit_pageRankConstants(&mut self, _ctx: &PageRankConstantsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#peerPressureConstants}.
     * @param ctx the parse tree
     */
    fn enter_peerPressureConstants(&mut self, _ctx: &PeerPressureConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#peerPressureConstants}.
     * @param ctx the parse tree
     */
    fn exit_peerPressureConstants(&mut self, _ctx: &PeerPressureConstantsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants(&mut self, _ctx: &ShortestPathConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants(&mut self, _ctx: &ShortestPathConstantsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsValues}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsValues(&mut self, _ctx: &WithOptionsValuesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsValues}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsValues(&mut self, _ctx: &WithOptionsValuesContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsKeys}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsKeys(&mut self, _ctx: &IoOptionsKeysContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsKeys}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsKeys(&mut self, _ctx: &IoOptionsKeysContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsValues}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsValues(&mut self, _ctx: &IoOptionsValuesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsValues}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsValues(&mut self, _ctx: &IoOptionsValuesContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#connectedComponentConstants_component}.
     * @param ctx the parse tree
     */
    fn enter_connectedComponentConstants_component(
        &mut self,
        _ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#connectedComponentConstants_component}.
     * @param ctx the parse tree
     */
    fn exit_connectedComponentConstants_component(
        &mut self,
        _ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#connectedComponentConstants_edges}.
     * @param ctx the parse tree
     */
    fn enter_connectedComponentConstants_edges(
        &mut self,
        _ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#connectedComponentConstants_edges}.
     * @param ctx the parse tree
     */
    fn exit_connectedComponentConstants_edges(
        &mut self,
        _ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#connectedComponentConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn enter_connectedComponentConstants_propertyName(
        &mut self,
        _ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#connectedComponentConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn exit_connectedComponentConstants_propertyName(
        &mut self,
        _ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#pageRankConstants_edges}.
     * @param ctx the parse tree
     */
    fn enter_pageRankConstants_edges(&mut self, _ctx: &PageRankConstants_edgesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#pageRankConstants_edges}.
     * @param ctx the parse tree
     */
    fn exit_pageRankConstants_edges(&mut self, _ctx: &PageRankConstants_edgesContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#pageRankConstants_times}.
     * @param ctx the parse tree
     */
    fn enter_pageRankConstants_times(&mut self, _ctx: &PageRankConstants_timesContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#pageRankConstants_times}.
     * @param ctx the parse tree
     */
    fn exit_pageRankConstants_times(&mut self, _ctx: &PageRankConstants_timesContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#pageRankConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn enter_pageRankConstants_propertyName(
        &mut self,
        _ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#pageRankConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn exit_pageRankConstants_propertyName(
        &mut self,
        _ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#peerPressureConstants_edges}.
     * @param ctx the parse tree
     */
    fn enter_peerPressureConstants_edges(
        &mut self,
        _ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#peerPressureConstants_edges}.
     * @param ctx the parse tree
     */
    fn exit_peerPressureConstants_edges(
        &mut self,
        _ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#peerPressureConstants_times}.
     * @param ctx the parse tree
     */
    fn enter_peerPressureConstants_times(
        &mut self,
        _ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#peerPressureConstants_times}.
     * @param ctx the parse tree
     */
    fn exit_peerPressureConstants_times(
        &mut self,
        _ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#peerPressureConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn enter_peerPressureConstants_propertyName(
        &mut self,
        _ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#peerPressureConstants_propertyName}.
     * @param ctx the parse tree
     */
    fn exit_peerPressureConstants_propertyName(
        &mut self,
        _ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants_target}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants_target(
        &mut self,
        _ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants_target}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants_target(
        &mut self,
        _ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants_edges}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants_edges(
        &mut self,
        _ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants_edges}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants_edges(
        &mut self,
        _ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants_distance}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants_distance(
        &mut self,
        _ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants_distance}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants_distance(
        &mut self,
        _ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants_maxDistance}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants_maxDistance(
        &mut self,
        _ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants_maxDistance}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants_maxDistance(
        &mut self,
        _ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathConstants_includeEdges}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathConstants_includeEdges(
        &mut self,
        _ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathConstants_includeEdges}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathConstants_includeEdges(
        &mut self,
        _ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_tokens}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_tokens(
        &mut self,
        _ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_tokens}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_tokens(
        &mut self,
        _ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_none}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_none(&mut self, _ctx: &WithOptionsConstants_noneContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_none}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_none(&mut self, _ctx: &WithOptionsConstants_noneContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_ids}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_ids(&mut self, _ctx: &WithOptionsConstants_idsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_ids}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_ids(&mut self, _ctx: &WithOptionsConstants_idsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_labels}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_labels(
        &mut self,
        _ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_labels}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_labels(
        &mut self,
        _ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_keys}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_keys(&mut self, _ctx: &WithOptionsConstants_keysContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_keys}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_keys(&mut self, _ctx: &WithOptionsConstants_keysContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_values}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_values(
        &mut self,
        _ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_values}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_values(
        &mut self,
        _ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_all}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_all(&mut self, _ctx: &WithOptionsConstants_allContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_all}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_all(&mut self, _ctx: &WithOptionsConstants_allContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_indexer}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_indexer(
        &mut self,
        _ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_indexer}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_indexer(
        &mut self,
        _ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_list}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_list(&mut self, _ctx: &WithOptionsConstants_listContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_list}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_list(&mut self, _ctx: &WithOptionsConstants_listContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsConstants_map}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsConstants_map(&mut self, _ctx: &WithOptionsConstants_mapContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsConstants_map}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsConstants_map(&mut self, _ctx: &WithOptionsConstants_mapContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsConstants_reader}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsConstants_reader(&mut self, _ctx: &IoOptionsConstants_readerContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsConstants_reader}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsConstants_reader(&mut self, _ctx: &IoOptionsConstants_readerContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsConstants_writer}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsConstants_writer(&mut self, _ctx: &IoOptionsConstants_writerContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsConstants_writer}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsConstants_writer(&mut self, _ctx: &IoOptionsConstants_writerContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsConstants_gryo}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsConstants_gryo(&mut self, _ctx: &IoOptionsConstants_gryoContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsConstants_gryo}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsConstants_gryo(&mut self, _ctx: &IoOptionsConstants_gryoContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphson}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsConstants_graphson(
        &mut self,
        _ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphson}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsConstants_graphson(
        &mut self,
        _ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphml}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsConstants_graphml(
        &mut self,
        _ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsConstants_graphml}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsConstants_graphml(
        &mut self,
        _ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#connectedComponentStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_connectedComponentStringConstant(
        &mut self,
        _ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#connectedComponentStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_connectedComponentStringConstant(
        &mut self,
        _ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#pageRankStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_pageRankStringConstant(&mut self, _ctx: &PageRankStringConstantContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#pageRankStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_pageRankStringConstant(&mut self, _ctx: &PageRankStringConstantContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#peerPressureStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_peerPressureStringConstant(
        &mut self,
        _ctx: &PeerPressureStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#peerPressureStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_peerPressureStringConstant(
        &mut self,
        _ctx: &PeerPressureStringConstantContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#shortestPathStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_shortestPathStringConstant(
        &mut self,
        _ctx: &ShortestPathStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#shortestPathStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_shortestPathStringConstant(
        &mut self,
        _ctx: &ShortestPathStringConstantContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#withOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_withOptionsStringConstant(&mut self, _ctx: &WithOptionsStringConstantContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#withOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_withOptionsStringConstant(&mut self, _ctx: &WithOptionsStringConstantContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#ioOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn enter_ioOptionsStringConstant(&mut self, _ctx: &IoOptionsStringConstantContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#ioOptionsStringConstant}.
     * @param ctx the parse tree
     */
    fn exit_ioOptionsStringConstant(&mut self, _ctx: &IoOptionsStringConstantContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#booleanArgument}.
     * @param ctx the parse tree
     */
    fn enter_booleanArgument(&mut self, _ctx: &BooleanArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#booleanArgument}.
     * @param ctx the parse tree
     */
    fn exit_booleanArgument(&mut self, _ctx: &BooleanArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#integerArgument}.
     * @param ctx the parse tree
     */
    fn enter_integerArgument(&mut self, _ctx: &IntegerArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#integerArgument}.
     * @param ctx the parse tree
     */
    fn exit_integerArgument(&mut self, _ctx: &IntegerArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringArgument}.
     * @param ctx the parse tree
     */
    fn enter_stringArgument(&mut self, _ctx: &StringArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringArgument}.
     * @param ctx the parse tree
     */
    fn exit_stringArgument(&mut self, _ctx: &StringArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringNullableArgument}.
     * @param ctx the parse tree
     */
    fn enter_stringNullableArgument(&mut self, _ctx: &StringNullableArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringNullableArgument}.
     * @param ctx the parse tree
     */
    fn exit_stringNullableArgument(&mut self, _ctx: &StringNullableArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringNullableArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn enter_stringNullableArgumentVarargs(
        &mut self,
        _ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringNullableArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn exit_stringNullableArgumentVarargs(
        &mut self,
        _ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#dateArgument}.
     * @param ctx the parse tree
     */
    fn enter_dateArgument(&mut self, _ctx: &DateArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#dateArgument}.
     * @param ctx the parse tree
     */
    fn exit_dateArgument(&mut self, _ctx: &DateArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericArgument}.
     * @param ctx the parse tree
     */
    fn enter_genericArgument(&mut self, _ctx: &GenericArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericArgument}.
     * @param ctx the parse tree
     */
    fn exit_genericArgument(&mut self, _ctx: &GenericArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn enter_genericArgumentVarargs(&mut self, _ctx: &GenericArgumentVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericArgumentVarargs}.
     * @param ctx the parse tree
     */
    fn exit_genericArgumentVarargs(&mut self, _ctx: &GenericArgumentVarargsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericMapArgument}.
     * @param ctx the parse tree
     */
    fn enter_genericMapArgument(&mut self, _ctx: &GenericMapArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericMapArgument}.
     * @param ctx the parse tree
     */
    fn exit_genericMapArgument(&mut self, _ctx: &GenericMapArgumentContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericMapNullableArgument}.
     * @param ctx the parse tree
     */
    fn enter_genericMapNullableArgument(
        &mut self,
        _ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericMapNullableArgument}.
     * @param ctx the parse tree
     */
    fn exit_genericMapNullableArgument(
        &mut self,
        _ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalStrategyVarargs}.
     * @param ctx the parse tree
     */
    fn enter_traversalStrategyVarargs(&mut self, _ctx: &TraversalStrategyVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalStrategyVarargs}.
     * @param ctx the parse tree
     */
    fn exit_traversalStrategyVarargs(&mut self, _ctx: &TraversalStrategyVarargsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#traversalStrategyExpr}.
     * @param ctx the parse tree
     */
    fn enter_traversalStrategyExpr(&mut self, _ctx: &TraversalStrategyExprContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#traversalStrategyExpr}.
     * @param ctx the parse tree
     */
    fn exit_traversalStrategyExpr(&mut self, _ctx: &TraversalStrategyExprContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#classTypeList}.
     * @param ctx the parse tree
     */
    fn enter_classTypeList(&mut self, _ctx: &ClassTypeListContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#classTypeList}.
     * @param ctx the parse tree
     */
    fn exit_classTypeList(&mut self, _ctx: &ClassTypeListContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#classTypeExpr}.
     * @param ctx the parse tree
     */
    fn enter_classTypeExpr(&mut self, _ctx: &ClassTypeExprContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#classTypeExpr}.
     * @param ctx the parse tree
     */
    fn exit_classTypeExpr(&mut self, _ctx: &ClassTypeExprContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nestedTraversalList}.
     * @param ctx the parse tree
     */
    fn enter_nestedTraversalList(&mut self, _ctx: &NestedTraversalListContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nestedTraversalList}.
     * @param ctx the parse tree
     */
    fn exit_nestedTraversalList(&mut self, _ctx: &NestedTraversalListContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nestedTraversalExpr}.
     * @param ctx the parse tree
     */
    fn enter_nestedTraversalExpr(&mut self, _ctx: &NestedTraversalExprContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nestedTraversalExpr}.
     * @param ctx the parse tree
     */
    fn exit_nestedTraversalExpr(&mut self, _ctx: &NestedTraversalExprContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericCollectionLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericCollectionLiteral(&mut self, _ctx: &GenericCollectionLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericCollectionLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericCollectionLiteral(&mut self, _ctx: &GenericCollectionLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn enter_genericLiteralVarargs(&mut self, _ctx: &GenericLiteralVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn exit_genericLiteralVarargs(&mut self, _ctx: &GenericLiteralVarargsContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericLiteralExpr}.
     * @param ctx the parse tree
     */
    fn enter_genericLiteralExpr(&mut self, _ctx: &GenericLiteralExprContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericLiteralExpr}.
     * @param ctx the parse tree
     */
    fn exit_genericLiteralExpr(&mut self, _ctx: &GenericLiteralExprContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericMapNullableLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericMapNullableLiteral(&mut self, _ctx: &GenericMapNullableLiteralContext<'input>) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericMapNullableLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericMapNullableLiteral(&mut self, _ctx: &GenericMapNullableLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericRangeLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericRangeLiteral(&mut self, _ctx: &GenericRangeLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericRangeLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericRangeLiteral(&mut self, _ctx: &GenericRangeLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericSetLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericSetLiteral(&mut self, _ctx: &GenericSetLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericSetLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericSetLiteral(&mut self, _ctx: &GenericSetLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringNullableLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn enter_stringNullableLiteralVarargs(
        &mut self,
        _ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringNullableLiteralVarargs}.
     * @param ctx the parse tree
     */
    fn exit_stringNullableLiteralVarargs(
        &mut self,
        _ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
    }
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericLiteral(&mut self, _ctx: &GenericLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericLiteral(&mut self, _ctx: &GenericLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#genericMapLiteral}.
     * @param ctx the parse tree
     */
    fn enter_genericMapLiteral(&mut self, _ctx: &GenericMapLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#genericMapLiteral}.
     * @param ctx the parse tree
     */
    fn exit_genericMapLiteral(&mut self, _ctx: &GenericMapLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#mapKey}.
     * @param ctx the parse tree
     */
    fn enter_mapKey(&mut self, _ctx: &MapKeyContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#mapKey}.
     * @param ctx the parse tree
     */
    fn exit_mapKey(&mut self, _ctx: &MapKeyContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#mapEntry}.
     * @param ctx the parse tree
     */
    fn enter_mapEntry(&mut self, _ctx: &MapEntryContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#mapEntry}.
     * @param ctx the parse tree
     */
    fn exit_mapEntry(&mut self, _ctx: &MapEntryContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringLiteral}.
     * @param ctx the parse tree
     */
    fn enter_stringLiteral(&mut self, _ctx: &StringLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringLiteral}.
     * @param ctx the parse tree
     */
    fn exit_stringLiteral(&mut self, _ctx: &StringLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#stringNullableLiteral}.
     * @param ctx the parse tree
     */
    fn enter_stringNullableLiteral(&mut self, _ctx: &StringNullableLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#stringNullableLiteral}.
     * @param ctx the parse tree
     */
    fn exit_stringNullableLiteral(&mut self, _ctx: &StringNullableLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#integerLiteral}.
     * @param ctx the parse tree
     */
    fn enter_integerLiteral(&mut self, _ctx: &IntegerLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#integerLiteral}.
     * @param ctx the parse tree
     */
    fn exit_integerLiteral(&mut self, _ctx: &IntegerLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#floatLiteral}.
     * @param ctx the parse tree
     */
    fn enter_floatLiteral(&mut self, _ctx: &FloatLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#floatLiteral}.
     * @param ctx the parse tree
     */
    fn exit_floatLiteral(&mut self, _ctx: &FloatLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#numericLiteral}.
     * @param ctx the parse tree
     */
    fn enter_numericLiteral(&mut self, _ctx: &NumericLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#numericLiteral}.
     * @param ctx the parse tree
     */
    fn exit_numericLiteral(&mut self, _ctx: &NumericLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#booleanLiteral}.
     * @param ctx the parse tree
     */
    fn enter_booleanLiteral(&mut self, _ctx: &BooleanLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#booleanLiteral}.
     * @param ctx the parse tree
     */
    fn exit_booleanLiteral(&mut self, _ctx: &BooleanLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#dateLiteral}.
     * @param ctx the parse tree
     */
    fn enter_dateLiteral(&mut self, _ctx: &DateLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#dateLiteral}.
     * @param ctx the parse tree
     */
    fn exit_dateLiteral(&mut self, _ctx: &DateLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nullLiteral}.
     * @param ctx the parse tree
     */
    fn enter_nullLiteral(&mut self, _ctx: &NullLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nullLiteral}.
     * @param ctx the parse tree
     */
    fn exit_nullLiteral(&mut self, _ctx: &NullLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nanLiteral}.
     * @param ctx the parse tree
     */
    fn enter_nanLiteral(&mut self, _ctx: &NanLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nanLiteral}.
     * @param ctx the parse tree
     */
    fn exit_nanLiteral(&mut self, _ctx: &NanLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#infLiteral}.
     * @param ctx the parse tree
     */
    fn enter_infLiteral(&mut self, _ctx: &InfLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#infLiteral}.
     * @param ctx the parse tree
     */
    fn exit_infLiteral(&mut self, _ctx: &InfLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#uuidLiteral}.
     * @param ctx the parse tree
     */
    fn enter_uuidLiteral(&mut self, _ctx: &UuidLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#uuidLiteral}.
     * @param ctx the parse tree
     */
    fn exit_uuidLiteral(&mut self, _ctx: &UuidLiteralContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#nakedKey}.
     * @param ctx the parse tree
     */
    fn enter_nakedKey(&mut self, _ctx: &NakedKeyContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#nakedKey}.
     * @param ctx the parse tree
     */
    fn exit_nakedKey(&mut self, _ctx: &NakedKeyContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#classType}.
     * @param ctx the parse tree
     */
    fn enter_classType(&mut self, _ctx: &ClassTypeContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#classType}.
     * @param ctx the parse tree
     */
    fn exit_classType(&mut self, _ctx: &ClassTypeContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#variable}.
     * @param ctx the parse tree
     */
    fn enter_variable(&mut self, _ctx: &VariableContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#variable}.
     * @param ctx the parse tree
     */
    fn exit_variable(&mut self, _ctx: &VariableContext<'input>) {}
    /**
     * Enter a parse tree produced by {@link GremlinParser#keyword}.
     * @param ctx the parse tree
     */
    fn enter_keyword(&mut self, _ctx: &KeywordContext<'input>) {}
    /**
     * Exit a parse tree produced by {@link GremlinParser#keyword}.
     * @param ctx the parse tree
     */
    fn exit_keyword(&mut self, _ctx: &KeywordContext<'input>) {}
}

antlr4rust::coerce_from! { 'input : GremlinListener<'input> }
