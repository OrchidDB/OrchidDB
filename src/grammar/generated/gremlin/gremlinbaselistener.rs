// Generated from languages/gremlin/Gremlin.g4 by ANTLR 4.13.2

use super::gremlinparser::*;
use antlr4rust::tree::ParseTreeListener;

// A complete Visitor for a parse tree produced by GremlinParser.

pub trait GremlinBaseListener<'input>: ParseTreeListener<'input, GremlinParserContextType> {
    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_querylist(&mut self, _ctx: &QueryListContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_querylist(&mut self, _ctx: &QueryListContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_query(&mut self, _ctx: &QueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_query(&mut self, _ctx: &QueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_emptyquery(&mut self, _ctx: &EmptyQueryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_emptyquery(&mut self, _ctx: &EmptyQueryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsource(&mut self, _ctx: &TraversalSourceContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsource(&mut self, _ctx: &TraversalSourceContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_transactionpart(&mut self, _ctx: &TransactionPartContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_transactionpart(&mut self, _ctx: &TransactionPartContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_roottraversal(&mut self, _ctx: &RootTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_roottraversal(&mut self, _ctx: &RootTraversalContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod(&mut self, _ctx: &TraversalSourceSelfMethodContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod(&mut self, _ctx: &TraversalSourceSelfMethodContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withbulk(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withbulk(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withBulkContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withpath(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withpath(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withPathContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withsack(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withsack(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSackContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withsideeffect(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withsideeffect(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withSideEffectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withstrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withstrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withStrategiesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_withoutstrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_withoutstrategies(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withoutStrategiesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourceselfmethod_with(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourceselfmethod_with(
        &mut self,
        _ctx: &TraversalSourceSelfMethod_withContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod(
        &mut self,
        _ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod(
        &mut self,
        _ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_adde(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_adde(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addEContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_addv(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_addv(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_addVContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_e(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_e(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_v(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_v(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_inject(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_inject(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_io(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_io(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_ioContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_mergev_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_mergev_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_MapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_mergev_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_mergev_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeV_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_mergee_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_mergee_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_MapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_mergee_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_mergee_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_mergeE_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_call_empty(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_call_empty(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_emptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_call_string(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_call_string(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_stringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_call_string_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_call_string_map(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_mapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_traversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_call_string_map_traversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsourcespawnmethod_union(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsourcespawnmethod_union(
        &mut self,
        _ctx: &TraversalSourceSpawnMethod_unionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_chainedtraversal(&mut self, _ctx: &ChainedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_chainedtraversal(&mut self, _ctx: &ChainedTraversalContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nestedtraversal(&mut self, _ctx: &NestedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nestedtraversal(&mut self, _ctx: &NestedTraversalContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_terminatedtraversal(&mut self, _ctx: &TerminatedTraversalContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_terminatedtraversal(&mut self, _ctx: &TerminatedTraversalContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod(&mut self, _ctx: &TraversalMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod(&mut self, _ctx: &TraversalMethodContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_v(&mut self, _ctx: &TraversalMethod_VContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_v(&mut self, _ctx: &TraversalMethod_VContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_e(&mut self, _ctx: &TraversalMethod_EContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_e(&mut self, _ctx: &TraversalMethod_EContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_adde_string(
        &mut self,
        _ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_adde_string(
        &mut self,
        _ctx: &TraversalMethod_addE_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_adde_traversal(
        &mut self,
        _ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_adde_traversal(
        &mut self,
        _ctx: &TraversalMethod_addE_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_addv_empty(
        &mut self,
        _ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_addv_empty(
        &mut self,
        _ctx: &TraversalMethod_addV_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_addv_string(
        &mut self,
        _ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_addv_string(
        &mut self,
        _ctx: &TraversalMethod_addV_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_addv_traversal(
        &mut self,
        _ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_addv_traversal(
        &mut self,
        _ctx: &TraversalMethod_addV_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_aggregate_string(
        &mut self,
        _ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_aggregate_string(
        &mut self,
        _ctx: &TraversalMethod_aggregate_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_all_p(&mut self, _ctx: &TraversalMethod_all_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_all_p(&mut self, _ctx: &TraversalMethod_all_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_and(&mut self, _ctx: &TraversalMethod_andContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_and(&mut self, _ctx: &TraversalMethod_andContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_any_p(&mut self, _ctx: &TraversalMethod_any_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_any_p(&mut self, _ctx: &TraversalMethod_any_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_as(&mut self, _ctx: &TraversalMethod_asContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_as(&mut self, _ctx: &TraversalMethod_asContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asbool(&mut self, _ctx: &TraversalMethod_asBoolContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asbool(&mut self, _ctx: &TraversalMethod_asBoolContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asdate(&mut self, _ctx: &TraversalMethod_asDateContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asdate(&mut self, _ctx: &TraversalMethod_asDateContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asnumber_empty(
        &mut self,
        _ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asnumber_empty(
        &mut self,
        _ctx: &TraversalMethod_asNumber_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asnumber_traversalgtype(
        &mut self,
        _ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asnumber_traversalgtype(
        &mut self,
        _ctx: &TraversalMethod_asNumber_traversalGTypeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asstring_empty(
        &mut self,
        _ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asstring_empty(
        &mut self,
        _ctx: &TraversalMethod_asString_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_asstring_scope(
        &mut self,
        _ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_asstring_scope(
        &mut self,
        _ctx: &TraversalMethod_asString_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_barrier_consumer(
        &mut self,
        _ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_barrier_consumer(
        &mut self,
        _ctx: &TraversalMethod_barrier_ConsumerContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_barrier_empty(
        &mut self,
        _ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_barrier_empty(
        &mut self,
        _ctx: &TraversalMethod_barrier_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_barrier_int(
        &mut self,
        _ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_barrier_int(
        &mut self,
        _ctx: &TraversalMethod_barrier_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_both(&mut self, _ctx: &TraversalMethod_bothContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_both(&mut self, _ctx: &TraversalMethod_bothContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_bothe(&mut self, _ctx: &TraversalMethod_bothEContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_bothe(&mut self, _ctx: &TraversalMethod_bothEContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_bothv(&mut self, _ctx: &TraversalMethod_bothVContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_bothv(&mut self, _ctx: &TraversalMethod_bothVContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_branch(&mut self, _ctx: &TraversalMethod_branchContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_branch(&mut self, _ctx: &TraversalMethod_branchContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_ComparatorContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_empty(&mut self, _ctx: &TraversalMethod_by_EmptyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_empty(&mut self, _ctx: &TraversalMethod_by_EmptyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_function(
        &mut self,
        _ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_function(
        &mut self,
        _ctx: &TraversalMethod_by_FunctionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_function_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_function_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Function_ComparatorContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_order(&mut self, _ctx: &TraversalMethod_by_OrderContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_order(&mut self, _ctx: &TraversalMethod_by_OrderContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_string(&mut self, _ctx: &TraversalMethod_by_StringContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_string(&mut self, _ctx: &TraversalMethod_by_StringContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_string_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_string_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_String_ComparatorContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_t(&mut self, _ctx: &TraversalMethod_by_TContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_t(&mut self, _ctx: &TraversalMethod_by_TContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_traversal(
        &mut self,
        _ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_traversal(
        &mut self,
        _ctx: &TraversalMethod_by_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_by_traversal_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_by_traversal_comparator(
        &mut self,
        _ctx: &TraversalMethod_by_Traversal_ComparatorContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_call_string(
        &mut self,
        _ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_call_string(
        &mut self,
        _ctx: &TraversalMethod_call_stringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_call_string_map(
        &mut self,
        _ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_call_string_map(
        &mut self,
        _ctx: &TraversalMethod_call_string_mapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_call_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_traversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_call_string_map_traversal(
        &mut self,
        _ctx: &TraversalMethod_call_string_map_traversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_cap(&mut self, _ctx: &TraversalMethod_capContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_cap(&mut self, _ctx: &TraversalMethod_capContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_function(
        &mut self,
        _ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_function(
        &mut self,
        _ctx: &TraversalMethod_choose_FunctionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_predicate_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_predicate_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_predicate_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_predicate_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Predicate_Traversal_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_choose_traversal_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_choose_traversal_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_choose_Traversal_Traversal_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_coalesce(&mut self, _ctx: &TraversalMethod_coalesceContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_coalesce(&mut self, _ctx: &TraversalMethod_coalesceContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_coin(&mut self, _ctx: &TraversalMethod_coinContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_coin(&mut self, _ctx: &TraversalMethod_coinContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_combine_object(
        &mut self,
        _ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_combine_object(
        &mut self,
        _ctx: &TraversalMethod_combine_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_concat_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_concat_traversal_traversal(
        &mut self,
        _ctx: &TraversalMethod_concat_Traversal_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_concat_string(
        &mut self,
        _ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_concat_string(
        &mut self,
        _ctx: &TraversalMethod_concat_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_conjoin_string(
        &mut self,
        _ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_conjoin_string(
        &mut self,
        _ctx: &TraversalMethod_conjoin_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_connectedcomponent(
        &mut self,
        _ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_connectedcomponent(
        &mut self,
        _ctx: &TraversalMethod_connectedComponentContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_constant(&mut self, _ctx: &TraversalMethod_constantContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_constant(&mut self, _ctx: &TraversalMethod_constantContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_count_empty(
        &mut self,
        _ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_count_empty(
        &mut self,
        _ctx: &TraversalMethod_count_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_count_scope(
        &mut self,
        _ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_count_scope(
        &mut self,
        _ctx: &TraversalMethod_count_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_cyclicpath(
        &mut self,
        _ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_cyclicpath(
        &mut self,
        _ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_dateadd(&mut self, _ctx: &TraversalMethod_dateAddContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_dateadd(&mut self, _ctx: &TraversalMethod_dateAddContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_datediff_traversal(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_datediff_traversal(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_datediff_date(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_datediff_date(
        &mut self,
        _ctx: &TraversalMethod_dateDiff_DateContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_dedup_scope_string(
        &mut self,
        _ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_dedup_scope_string(
        &mut self,
        _ctx: &TraversalMethod_dedup_Scope_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_dedup_string(
        &mut self,
        _ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_dedup_string(
        &mut self,
        _ctx: &TraversalMethod_dedup_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_difference_object(
        &mut self,
        _ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_difference_object(
        &mut self,
        _ctx: &TraversalMethod_difference_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_discard(&mut self, _ctx: &TraversalMethod_discardContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_discard(&mut self, _ctx: &TraversalMethod_discardContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_disjunct_object(
        &mut self,
        _ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_disjunct_object(
        &mut self,
        _ctx: &TraversalMethod_disjunct_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_drop(&mut self, _ctx: &TraversalMethod_dropContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_drop(&mut self, _ctx: &TraversalMethod_dropContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_element(&mut self, _ctx: &TraversalMethod_elementContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_element(&mut self, _ctx: &TraversalMethod_elementContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_elementmap(
        &mut self,
        _ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_elementmap(
        &mut self,
        _ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_emit_empty(
        &mut self,
        _ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_emit_empty(
        &mut self,
        _ctx: &TraversalMethod_emit_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_emit_predicate(
        &mut self,
        _ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_emit_predicate(
        &mut self,
        _ctx: &TraversalMethod_emit_PredicateContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_emit_traversal(
        &mut self,
        _ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_emit_traversal(
        &mut self,
        _ctx: &TraversalMethod_emit_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_fail_empty(
        &mut self,
        _ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_fail_empty(
        &mut self,
        _ctx: &TraversalMethod_fail_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_fail_string(
        &mut self,
        _ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_fail_string(
        &mut self,
        _ctx: &TraversalMethod_fail_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_filter_predicate(
        &mut self,
        _ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_filter_predicate(
        &mut self,
        _ctx: &TraversalMethod_filter_PredicateContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_filter_traversal(
        &mut self,
        _ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_filter_traversal(
        &mut self,
        _ctx: &TraversalMethod_filter_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_flatmap(&mut self, _ctx: &TraversalMethod_flatMapContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_flatmap(&mut self, _ctx: &TraversalMethod_flatMapContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_fold_empty(
        &mut self,
        _ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_fold_empty(
        &mut self,
        _ctx: &TraversalMethod_fold_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_fold_object_bifunction(
        &mut self,
        _ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_fold_object_bifunction(
        &mut self,
        _ctx: &TraversalMethod_fold_Object_BiFunctionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_format_string(
        &mut self,
        _ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_format_string(
        &mut self,
        _ctx: &TraversalMethod_format_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_from_string(
        &mut self,
        _ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_from_string(
        &mut self,
        _ctx: &TraversalMethod_from_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_from_traversal(
        &mut self,
        _ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_from_traversal(
        &mut self,
        _ctx: &TraversalMethod_from_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_group_empty(
        &mut self,
        _ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_group_empty(
        &mut self,
        _ctx: &TraversalMethod_group_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_group_string(
        &mut self,
        _ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_group_string(
        &mut self,
        _ctx: &TraversalMethod_group_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_groupcount_empty(
        &mut self,
        _ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_groupcount_empty(
        &mut self,
        _ctx: &TraversalMethod_groupCount_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_groupcount_string(
        &mut self,
        _ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_groupcount_string(
        &mut self,
        _ctx: &TraversalMethod_groupCount_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_string(
        &mut self,
        _ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_string(
        &mut self,
        _ctx: &TraversalMethod_has_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_string_object(
        &mut self,
        _ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_string_object(
        &mut self,
        _ctx: &TraversalMethod_has_String_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_string_p(
        &mut self,
        _ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_string_p(
        &mut self,
        _ctx: &TraversalMethod_has_String_PContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_string_string_object(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_string_string_object(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_string_string_p(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_string_string_p(
        &mut self,
        _ctx: &TraversalMethod_has_String_String_PContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_t_object(
        &mut self,
        _ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_t_object(
        &mut self,
        _ctx: &TraversalMethod_has_T_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_has_t_p(&mut self, _ctx: &TraversalMethod_has_T_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_has_t_p(&mut self, _ctx: &TraversalMethod_has_T_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_hasid_object_object(
        &mut self,
        _ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_hasid_object_object(
        &mut self,
        _ctx: &TraversalMethod_hasId_Object_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_hasid_p(&mut self, _ctx: &TraversalMethod_hasId_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_hasid_p(&mut self, _ctx: &TraversalMethod_hasId_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_haskey_p(&mut self, _ctx: &TraversalMethod_hasKey_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_haskey_p(&mut self, _ctx: &TraversalMethod_hasKey_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_haskey_string_string(
        &mut self,
        _ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_haskey_string_string(
        &mut self,
        _ctx: &TraversalMethod_hasKey_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_haslabel_p(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_haslabel_p(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_PContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_haslabel_string_string(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_haslabel_string_string(
        &mut self,
        _ctx: &TraversalMethod_hasLabel_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_hasnot(&mut self, _ctx: &TraversalMethod_hasNotContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_hasnot(&mut self, _ctx: &TraversalMethod_hasNotContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_hasvalue_object_object(
        &mut self,
        _ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_hasvalue_object_object(
        &mut self,
        _ctx: &TraversalMethod_hasValue_Object_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_hasvalue_p(
        &mut self,
        _ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_hasvalue_p(
        &mut self,
        _ctx: &TraversalMethod_hasValue_PContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_id(&mut self, _ctx: &TraversalMethod_idContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_id(&mut self, _ctx: &TraversalMethod_idContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_identity(&mut self, _ctx: &TraversalMethod_identityContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_identity(&mut self, _ctx: &TraversalMethod_identityContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_in(&mut self, _ctx: &TraversalMethod_inContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_in(&mut self, _ctx: &TraversalMethod_inContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_ine(&mut self, _ctx: &TraversalMethod_inEContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_ine(&mut self, _ctx: &TraversalMethod_inEContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_intersect_object(
        &mut self,
        _ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_intersect_object(
        &mut self,
        _ctx: &TraversalMethod_intersect_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_inv(&mut self, _ctx: &TraversalMethod_inVContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_inv(&mut self, _ctx: &TraversalMethod_inVContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_index(&mut self, _ctx: &TraversalMethod_indexContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_index(&mut self, _ctx: &TraversalMethod_indexContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_inject(&mut self, _ctx: &TraversalMethod_injectContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_inject(&mut self, _ctx: &TraversalMethod_injectContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_is_object(&mut self, _ctx: &TraversalMethod_is_ObjectContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_is_object(&mut self, _ctx: &TraversalMethod_is_ObjectContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_is_p(&mut self, _ctx: &TraversalMethod_is_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_is_p(&mut self, _ctx: &TraversalMethod_is_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_key(&mut self, _ctx: &TraversalMethod_keyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_key(&mut self, _ctx: &TraversalMethod_keyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_label(&mut self, _ctx: &TraversalMethod_labelContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_label(&mut self, _ctx: &TraversalMethod_labelContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_length_empty(
        &mut self,
        _ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_length_empty(
        &mut self,
        _ctx: &TraversalMethod_length_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_length_scope(
        &mut self,
        _ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_length_scope(
        &mut self,
        _ctx: &TraversalMethod_length_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_limit_scope_long(
        &mut self,
        _ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_limit_scope_long(
        &mut self,
        _ctx: &TraversalMethod_limit_Scope_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_limit_long(
        &mut self,
        _ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_limit_long(
        &mut self,
        _ctx: &TraversalMethod_limit_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_local(&mut self, _ctx: &TraversalMethod_localContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_local(&mut self, _ctx: &TraversalMethod_localContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_loops_empty(
        &mut self,
        _ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_loops_empty(
        &mut self,
        _ctx: &TraversalMethod_loops_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_loops_string(
        &mut self,
        _ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_loops_string(
        &mut self,
        _ctx: &TraversalMethod_loops_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_ltrim_empty(
        &mut self,
        _ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_ltrim_empty(
        &mut self,
        _ctx: &TraversalMethod_lTrim_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_ltrim_scope(
        &mut self,
        _ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_ltrim_scope(
        &mut self,
        _ctx: &TraversalMethod_lTrim_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_map(&mut self, _ctx: &TraversalMethod_mapContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_map(&mut self, _ctx: &TraversalMethod_mapContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_match(&mut self, _ctx: &TraversalMethod_matchContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_match(&mut self, _ctx: &TraversalMethod_matchContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_math(&mut self, _ctx: &TraversalMethod_mathContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_math(&mut self, _ctx: &TraversalMethod_mathContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_max_empty(&mut self, _ctx: &TraversalMethod_max_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_max_empty(&mut self, _ctx: &TraversalMethod_max_EmptyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_max_scope(&mut self, _ctx: &TraversalMethod_max_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_max_scope(&mut self, _ctx: &TraversalMethod_max_ScopeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mean_empty(
        &mut self,
        _ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mean_empty(
        &mut self,
        _ctx: &TraversalMethod_mean_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mean_scope(
        &mut self,
        _ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mean_scope(
        &mut self,
        _ctx: &TraversalMethod_mean_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_merge_object(
        &mut self,
        _ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_merge_object(
        &mut self,
        _ctx: &TraversalMethod_merge_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergev_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergev_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeV_emptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergev_map(
        &mut self,
        _ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergev_map(
        &mut self,
        _ctx: &TraversalMethod_mergeV_MapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergev_traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergev_traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeV_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergee_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergee_empty(
        &mut self,
        _ctx: &TraversalMethod_mergeE_emptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergee_map(
        &mut self,
        _ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergee_map(
        &mut self,
        _ctx: &TraversalMethod_mergeE_MapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_mergee_traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_mergee_traversal(
        &mut self,
        _ctx: &TraversalMethod_mergeE_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_min_empty(&mut self, _ctx: &TraversalMethod_min_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_min_empty(&mut self, _ctx: &TraversalMethod_min_EmptyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_min_scope(&mut self, _ctx: &TraversalMethod_min_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_min_scope(&mut self, _ctx: &TraversalMethod_min_ScopeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_none_p(&mut self, _ctx: &TraversalMethod_none_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_none_p(&mut self, _ctx: &TraversalMethod_none_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_not(&mut self, _ctx: &TraversalMethod_notContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_not(&mut self, _ctx: &TraversalMethod_notContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_predicate_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_predicate_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Predicate_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_merge_map(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_merge_map(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_MapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_merge_map_cardinality(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_merge_map_cardinality(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_Map_CardinalityContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_merge_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_merge_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Merge_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_object_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_object_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_Object_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_option_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_option_traversal(
        &mut self,
        _ctx: &TraversalMethod_option_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_optional(&mut self, _ctx: &TraversalMethod_optionalContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_optional(&mut self, _ctx: &TraversalMethod_optionalContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_or(&mut self, _ctx: &TraversalMethod_orContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_or(&mut self, _ctx: &TraversalMethod_orContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_order_empty(
        &mut self,
        _ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_order_empty(
        &mut self,
        _ctx: &TraversalMethod_order_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_order_scope(
        &mut self,
        _ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_order_scope(
        &mut self,
        _ctx: &TraversalMethod_order_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_otherv(&mut self, _ctx: &TraversalMethod_otherVContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_otherv(&mut self, _ctx: &TraversalMethod_otherVContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_out(&mut self, _ctx: &TraversalMethod_outContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_out(&mut self, _ctx: &TraversalMethod_outContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_oute(&mut self, _ctx: &TraversalMethod_outEContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_oute(&mut self, _ctx: &TraversalMethod_outEContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_outv(&mut self, _ctx: &TraversalMethod_outVContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_outv(&mut self, _ctx: &TraversalMethod_outVContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_pagerank_empty(
        &mut self,
        _ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_pagerank_empty(
        &mut self,
        _ctx: &TraversalMethod_pageRank_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_pagerank_double(
        &mut self,
        _ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_pagerank_double(
        &mut self,
        _ctx: &TraversalMethod_pageRank_doubleContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_path(&mut self, _ctx: &TraversalMethod_pathContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_path(&mut self, _ctx: &TraversalMethod_pathContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_peerpressure(
        &mut self,
        _ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_peerpressure(
        &mut self,
        _ctx: &TraversalMethod_peerPressureContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_product_object(
        &mut self,
        _ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_product_object(
        &mut self,
        _ctx: &TraversalMethod_product_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_profile_empty(
        &mut self,
        _ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_profile_empty(
        &mut self,
        _ctx: &TraversalMethod_profile_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_profile_string(
        &mut self,
        _ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_profile_string(
        &mut self,
        _ctx: &TraversalMethod_profile_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_project(&mut self, _ctx: &TraversalMethod_projectContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_project(&mut self, _ctx: &TraversalMethod_projectContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_properties(
        &mut self,
        _ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_properties(
        &mut self,
        _ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_property_cardinality_object_object_object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_property_cardinality_object_object_object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_Object_Object_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_property_cardinality_object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_property_cardinality_object(
        &mut self,
        _ctx: &TraversalMethod_property_Cardinality_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_property_object_object_object(
        &mut self,
        _ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_property_object_object_object(
        &mut self,
        _ctx: &TraversalMethod_property_Object_Object_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_property_object(
        &mut self,
        _ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_property_object(
        &mut self,
        _ctx: &TraversalMethod_property_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_propertymap(
        &mut self,
        _ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_propertymap(
        &mut self,
        _ctx: &TraversalMethod_propertyMapContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_range_scope_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_range_scope_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_Scope_long_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_range_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_range_long_long(
        &mut self,
        _ctx: &TraversalMethod_range_long_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_read(&mut self, _ctx: &TraversalMethod_readContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_read(&mut self, _ctx: &TraversalMethod_readContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_repeat_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_repeat_string_traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_String_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_repeat_traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_repeat_traversal(
        &mut self,
        _ctx: &TraversalMethod_repeat_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_replace_string_string(
        &mut self,
        _ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_replace_string_string(
        &mut self,
        _ctx: &TraversalMethod_replace_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_replace_scope_string_string(
        &mut self,
        _ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_replace_scope_string_string(
        &mut self,
        _ctx: &TraversalMethod_replace_Scope_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_reverse_empty(
        &mut self,
        _ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_reverse_empty(
        &mut self,
        _ctx: &TraversalMethod_reverse_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_rtrim_empty(
        &mut self,
        _ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_rtrim_empty(
        &mut self,
        _ctx: &TraversalMethod_rTrim_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_rtrim_scope(
        &mut self,
        _ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_rtrim_scope(
        &mut self,
        _ctx: &TraversalMethod_rTrim_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sack_bifunction(
        &mut self,
        _ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sack_bifunction(
        &mut self,
        _ctx: &TraversalMethod_sack_BiFunctionContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sack_empty(
        &mut self,
        _ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sack_empty(
        &mut self,
        _ctx: &TraversalMethod_sack_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sample_scope_int(
        &mut self,
        _ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sample_scope_int(
        &mut self,
        _ctx: &TraversalMethod_sample_Scope_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sample_int(
        &mut self,
        _ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sample_int(
        &mut self,
        _ctx: &TraversalMethod_sample_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_column(
        &mut self,
        _ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_column(
        &mut self,
        _ctx: &TraversalMethod_select_ColumnContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_pop_string(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_pop_string(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_pop_string_string_string(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_pop_string_string_string(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_String_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_pop_traversal(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_pop_traversal(
        &mut self,
        _ctx: &TraversalMethod_select_Pop_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_string(
        &mut self,
        _ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_string(
        &mut self,
        _ctx: &TraversalMethod_select_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_string_string_string(
        &mut self,
        _ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_string_string_string(
        &mut self,
        _ctx: &TraversalMethod_select_String_String_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_select_traversal(
        &mut self,
        _ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_select_traversal(
        &mut self,
        _ctx: &TraversalMethod_select_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_shortestpath(
        &mut self,
        _ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_shortestpath(
        &mut self,
        _ctx: &TraversalMethod_shortestPathContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sideeffect(
        &mut self,
        _ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sideeffect(
        &mut self,
        _ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_simplepath(
        &mut self,
        _ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_simplepath(
        &mut self,
        _ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_skip_scope_long(
        &mut self,
        _ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_skip_scope_long(
        &mut self,
        _ctx: &TraversalMethod_skip_Scope_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_skip_long(&mut self, _ctx: &TraversalMethod_skip_longContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_skip_long(&mut self, _ctx: &TraversalMethod_skip_longContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_split_string(
        &mut self,
        _ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_split_string(
        &mut self,
        _ctx: &TraversalMethod_split_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_split_scope_string(
        &mut self,
        _ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_split_scope_string(
        &mut self,
        _ctx: &TraversalMethod_split_Scope_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_subgraph(&mut self, _ctx: &TraversalMethod_subgraphContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_subgraph(&mut self, _ctx: &TraversalMethod_subgraphContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_substring_int(
        &mut self,
        _ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_substring_int(
        &mut self,
        _ctx: &TraversalMethod_substring_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_substring_scope_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_substring_scope_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_substring_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_substring_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_int_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_substring_scope_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_substring_scope_int_int(
        &mut self,
        _ctx: &TraversalMethod_substring_Scope_int_intContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sum_empty(&mut self, _ctx: &TraversalMethod_sum_EmptyContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sum_empty(&mut self, _ctx: &TraversalMethod_sum_EmptyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_sum_scope(&mut self, _ctx: &TraversalMethod_sum_ScopeContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_sum_scope(&mut self, _ctx: &TraversalMethod_sum_ScopeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tail_empty(
        &mut self,
        _ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tail_empty(
        &mut self,
        _ctx: &TraversalMethod_tail_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tail_scope(
        &mut self,
        _ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tail_scope(
        &mut self,
        _ctx: &TraversalMethod_tail_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tail_scope_long(
        &mut self,
        _ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tail_scope_long(
        &mut self,
        _ctx: &TraversalMethod_tail_Scope_longContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tail_long(&mut self, _ctx: &TraversalMethod_tail_longContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tail_long(&mut self, _ctx: &TraversalMethod_tail_longContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_timelimit(&mut self, _ctx: &TraversalMethod_timeLimitContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_timelimit(&mut self, _ctx: &TraversalMethod_timeLimitContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_times(&mut self, _ctx: &TraversalMethod_timesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_times(&mut self, _ctx: &TraversalMethod_timesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_to_direction_string(
        &mut self,
        _ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_to_direction_string(
        &mut self,
        _ctx: &TraversalMethod_to_Direction_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_to_string(&mut self, _ctx: &TraversalMethod_to_StringContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_to_string(&mut self, _ctx: &TraversalMethod_to_StringContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_to_traversal(
        &mut self,
        _ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_to_traversal(
        &mut self,
        _ctx: &TraversalMethod_to_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_toe(&mut self, _ctx: &TraversalMethod_toEContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_toe(&mut self, _ctx: &TraversalMethod_toEContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tolower_empty(
        &mut self,
        _ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tolower_empty(
        &mut self,
        _ctx: &TraversalMethod_toLower_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tolower_scope(
        &mut self,
        _ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tolower_scope(
        &mut self,
        _ctx: &TraversalMethod_toLower_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_toupper_empty(
        &mut self,
        _ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_toupper_empty(
        &mut self,
        _ctx: &TraversalMethod_toUpper_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_toupper_scope(
        &mut self,
        _ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_toupper_scope(
        &mut self,
        _ctx: &TraversalMethod_toUpper_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tov(&mut self, _ctx: &TraversalMethod_toVContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tov(&mut self, _ctx: &TraversalMethod_toVContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tree_empty(
        &mut self,
        _ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tree_empty(
        &mut self,
        _ctx: &TraversalMethod_tree_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_tree_string(
        &mut self,
        _ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_tree_string(
        &mut self,
        _ctx: &TraversalMethod_tree_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_trim_empty(
        &mut self,
        _ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_trim_empty(
        &mut self,
        _ctx: &TraversalMethod_trim_EmptyContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_trim_scope(
        &mut self,
        _ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_trim_scope(
        &mut self,
        _ctx: &TraversalMethod_trim_ScopeContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_unfold(&mut self, _ctx: &TraversalMethod_unfoldContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_unfold(&mut self, _ctx: &TraversalMethod_unfoldContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_union(&mut self, _ctx: &TraversalMethod_unionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_union(&mut self, _ctx: &TraversalMethod_unionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_until_predicate(
        &mut self,
        _ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_until_predicate(
        &mut self,
        _ctx: &TraversalMethod_until_PredicateContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_until_traversal(
        &mut self,
        _ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_until_traversal(
        &mut self,
        _ctx: &TraversalMethod_until_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_value(&mut self, _ctx: &TraversalMethod_valueContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_value(&mut self, _ctx: &TraversalMethod_valueContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_valuemap_string(
        &mut self,
        _ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_valuemap_string(
        &mut self,
        _ctx: &TraversalMethod_valueMap_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_valuemap_boolean_string(
        &mut self,
        _ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_valuemap_boolean_string(
        &mut self,
        _ctx: &TraversalMethod_valueMap_boolean_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_values(&mut self, _ctx: &TraversalMethod_valuesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_values(&mut self, _ctx: &TraversalMethod_valuesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_where_p(&mut self, _ctx: &TraversalMethod_where_PContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_where_p(&mut self, _ctx: &TraversalMethod_where_PContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_where_string_p(
        &mut self,
        _ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_where_string_p(
        &mut self,
        _ctx: &TraversalMethod_where_String_PContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_where_traversal(
        &mut self,
        _ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_where_traversal(
        &mut self,
        _ctx: &TraversalMethod_where_TraversalContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_with_string(
        &mut self,
        _ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_with_string(
        &mut self,
        _ctx: &TraversalMethod_with_StringContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_with_string_object(
        &mut self,
        _ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_with_string_object(
        &mut self,
        _ctx: &TraversalMethod_with_String_ObjectContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmethod_write(&mut self, _ctx: &TraversalMethod_writeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmethod_write(&mut self, _ctx: &TraversalMethod_writeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalstrategy(&mut self, _ctx: &TraversalStrategyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalstrategy(&mut self, _ctx: &TraversalStrategyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_configuration(&mut self, _ctx: &ConfigurationContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_configuration(&mut self, _ctx: &ConfigurationContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalscope(&mut self, _ctx: &TraversalScopeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalscope(&mut self, _ctx: &TraversalScopeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalbarrier(&mut self, _ctx: &TraversalBarrierContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalbarrier(&mut self, _ctx: &TraversalBarrierContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalt(&mut self, _ctx: &TraversalTContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalt(&mut self, _ctx: &TraversalTContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaltshort(&mut self, _ctx: &TraversalTShortContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaltshort(&mut self, _ctx: &TraversalTShortContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaltlong(&mut self, _ctx: &TraversalTLongContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaltlong(&mut self, _ctx: &TraversalTLongContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalmerge(&mut self, _ctx: &TraversalMergeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalmerge(&mut self, _ctx: &TraversalMergeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalorder(&mut self, _ctx: &TraversalOrderContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalorder(&mut self, _ctx: &TraversalOrderContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaldirection(&mut self, _ctx: &TraversalDirectionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaldirection(&mut self, _ctx: &TraversalDirectionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaldirectionshort(&mut self, _ctx: &TraversalDirectionShortContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaldirectionshort(&mut self, _ctx: &TraversalDirectionShortContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaldirectionlong(&mut self, _ctx: &TraversalDirectionLongContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaldirectionlong(&mut self, _ctx: &TraversalDirectionLongContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalcardinality(&mut self, _ctx: &TraversalCardinalityContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalcardinality(&mut self, _ctx: &TraversalCardinalityContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalcolumn(&mut self, _ctx: &TraversalColumnContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalcolumn(&mut self, _ctx: &TraversalColumnContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpop(&mut self, _ctx: &TraversalPopContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpop(&mut self, _ctx: &TraversalPopContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaloperator(&mut self, _ctx: &TraversalOperatorContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaloperator(&mut self, _ctx: &TraversalOperatorContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpick(&mut self, _ctx: &TraversalPickContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpick(&mut self, _ctx: &TraversalPickContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversaldt(&mut self, _ctx: &TraversalDTContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversaldt(&mut self, _ctx: &TraversalDTContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalgtype(&mut self, _ctx: &TraversalGTypeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalgtype(&mut self, _ctx: &TraversalGTypeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate(&mut self, _ctx: &TraversalPredicateContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate(&mut self, _ctx: &TraversalPredicateContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod(&mut self, _ctx: &TraversalTerminalMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod(&mut self, _ctx: &TraversalTerminalMethodContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalsackmethod(&mut self, _ctx: &TraversalSackMethodContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalsackmethod(&mut self, _ctx: &TraversalSackMethodContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalcomparator(&mut self, _ctx: &TraversalComparatorContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalcomparator(&mut self, _ctx: &TraversalComparatorContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalfunction(&mut self, _ctx: &TraversalFunctionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalfunction(&mut self, _ctx: &TraversalFunctionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalbifunction(&mut self, _ctx: &TraversalBiFunctionContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalbifunction(&mut self, _ctx: &TraversalBiFunctionContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_eq(&mut self, _ctx: &TraversalPredicate_eqContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_eq(&mut self, _ctx: &TraversalPredicate_eqContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_neq(&mut self, _ctx: &TraversalPredicate_neqContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_neq(&mut self, _ctx: &TraversalPredicate_neqContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_typeof(&mut self, _ctx: &TraversalPredicate_typeOfContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_typeof(&mut self, _ctx: &TraversalPredicate_typeOfContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_lt(&mut self, _ctx: &TraversalPredicate_ltContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_lt(&mut self, _ctx: &TraversalPredicate_ltContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_lte(&mut self, _ctx: &TraversalPredicate_lteContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_lte(&mut self, _ctx: &TraversalPredicate_lteContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_gt(&mut self, _ctx: &TraversalPredicate_gtContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_gt(&mut self, _ctx: &TraversalPredicate_gtContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_gte(&mut self, _ctx: &TraversalPredicate_gteContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_gte(&mut self, _ctx: &TraversalPredicate_gteContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_inside(&mut self, _ctx: &TraversalPredicate_insideContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_inside(&mut self, _ctx: &TraversalPredicate_insideContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_outside(
        &mut self,
        _ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_outside(
        &mut self,
        _ctx: &TraversalPredicate_outsideContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_between(
        &mut self,
        _ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_between(
        &mut self,
        _ctx: &TraversalPredicate_betweenContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_within(&mut self, _ctx: &TraversalPredicate_withinContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_within(&mut self, _ctx: &TraversalPredicate_withinContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_without(
        &mut self,
        _ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_without(
        &mut self,
        _ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_not(&mut self, _ctx: &TraversalPredicate_notContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_not(&mut self, _ctx: &TraversalPredicate_notContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_containing(
        &mut self,
        _ctx: &TraversalPredicate_containingContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_containing(
        &mut self,
        _ctx: &TraversalPredicate_containingContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_notcontaining(
        &mut self,
        _ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_notcontaining(
        &mut self,
        _ctx: &TraversalPredicate_notContainingContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_startingwith(
        &mut self,
        _ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_startingwith(
        &mut self,
        _ctx: &TraversalPredicate_startingWithContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_notstartingwith(
        &mut self,
        _ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_notstartingwith(
        &mut self,
        _ctx: &TraversalPredicate_notStartingWithContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_endingwith(
        &mut self,
        _ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_endingwith(
        &mut self,
        _ctx: &TraversalPredicate_endingWithContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_notendingwith(
        &mut self,
        _ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_notendingwith(
        &mut self,
        _ctx: &TraversalPredicate_notEndingWithContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_regex(&mut self, _ctx: &TraversalPredicate_regexContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_regex(&mut self, _ctx: &TraversalPredicate_regexContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalpredicate_notregex(
        &mut self,
        _ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalpredicate_notregex(
        &mut self,
        _ctx: &TraversalPredicate_notRegexContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_explain(
        &mut self,
        _ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_explain(
        &mut self,
        _ctx: &TraversalTerminalMethod_explainContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_hasnext(
        &mut self,
        _ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_hasnext(
        &mut self,
        _ctx: &TraversalTerminalMethod_hasNextContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_iterate(
        &mut self,
        _ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_iterate(
        &mut self,
        _ctx: &TraversalTerminalMethod_iterateContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_trynext(
        &mut self,
        _ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_trynext(
        &mut self,
        _ctx: &TraversalTerminalMethod_tryNextContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_next(
        &mut self,
        _ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_next(
        &mut self,
        _ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_tolist(
        &mut self,
        _ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_tolist(
        &mut self,
        _ctx: &TraversalTerminalMethod_toListContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_toset(
        &mut self,
        _ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_toset(
        &mut self,
        _ctx: &TraversalTerminalMethod_toSetContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalterminalmethod_tobulkset(
        &mut self,
        _ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalterminalmethod_tobulkset(
        &mut self,
        _ctx: &TraversalTerminalMethod_toBulkSetContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionkeys(&mut self, _ctx: &WithOptionKeysContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionkeys(&mut self, _ctx: &WithOptionKeysContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_connectedcomponentconstants(
        &mut self,
        _ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_connectedcomponentconstants(
        &mut self,
        _ctx: &ConnectedComponentConstantsContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_pagerankconstants(&mut self, _ctx: &PageRankConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_pagerankconstants(&mut self, _ctx: &PageRankConstantsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_peerpressureconstants(&mut self, _ctx: &PeerPressureConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_peerpressureconstants(&mut self, _ctx: &PeerPressureConstantsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants(&mut self, _ctx: &ShortestPathConstantsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants(&mut self, _ctx: &ShortestPathConstantsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsvalues(&mut self, _ctx: &WithOptionsValuesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsvalues(&mut self, _ctx: &WithOptionsValuesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionskeys(&mut self, _ctx: &IoOptionsKeysContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionskeys(&mut self, _ctx: &IoOptionsKeysContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsvalues(&mut self, _ctx: &IoOptionsValuesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsvalues(&mut self, _ctx: &IoOptionsValuesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_connectedcomponentconstants_component(
        &mut self,
        _ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_connectedcomponentconstants_component(
        &mut self,
        _ctx: &ConnectedComponentConstants_componentContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_connectedcomponentconstants_edges(
        &mut self,
        _ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_connectedcomponentconstants_edges(
        &mut self,
        _ctx: &ConnectedComponentConstants_edgesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_connectedcomponentconstants_propertyname(
        &mut self,
        _ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_connectedcomponentconstants_propertyname(
        &mut self,
        _ctx: &ConnectedComponentConstants_propertyNameContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_pagerankconstants_edges(&mut self, _ctx: &PageRankConstants_edgesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_pagerankconstants_edges(&mut self, _ctx: &PageRankConstants_edgesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_pagerankconstants_times(&mut self, _ctx: &PageRankConstants_timesContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_pagerankconstants_times(&mut self, _ctx: &PageRankConstants_timesContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_pagerankconstants_propertyname(
        &mut self,
        _ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_pagerankconstants_propertyname(
        &mut self,
        _ctx: &PageRankConstants_propertyNameContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_peerpressureconstants_edges(
        &mut self,
        _ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_peerpressureconstants_edges(
        &mut self,
        _ctx: &PeerPressureConstants_edgesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_peerpressureconstants_times(
        &mut self,
        _ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_peerpressureconstants_times(
        &mut self,
        _ctx: &PeerPressureConstants_timesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_peerpressureconstants_propertyname(
        &mut self,
        _ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_peerpressureconstants_propertyname(
        &mut self,
        _ctx: &PeerPressureConstants_propertyNameContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants_target(
        &mut self,
        _ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants_target(
        &mut self,
        _ctx: &ShortestPathConstants_targetContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants_edges(
        &mut self,
        _ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants_edges(
        &mut self,
        _ctx: &ShortestPathConstants_edgesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants_distance(
        &mut self,
        _ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants_distance(
        &mut self,
        _ctx: &ShortestPathConstants_distanceContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants_maxdistance(
        &mut self,
        _ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants_maxdistance(
        &mut self,
        _ctx: &ShortestPathConstants_maxDistanceContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathconstants_includeedges(
        &mut self,
        _ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathconstants_includeedges(
        &mut self,
        _ctx: &ShortestPathConstants_includeEdgesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_tokens(
        &mut self,
        _ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_tokens(
        &mut self,
        _ctx: &WithOptionsConstants_tokensContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_none(&mut self, _ctx: &WithOptionsConstants_noneContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_none(&mut self, _ctx: &WithOptionsConstants_noneContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_ids(&mut self, _ctx: &WithOptionsConstants_idsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_ids(&mut self, _ctx: &WithOptionsConstants_idsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_labels(
        &mut self,
        _ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_labels(
        &mut self,
        _ctx: &WithOptionsConstants_labelsContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_keys(&mut self, _ctx: &WithOptionsConstants_keysContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_keys(&mut self, _ctx: &WithOptionsConstants_keysContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_values(
        &mut self,
        _ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_values(
        &mut self,
        _ctx: &WithOptionsConstants_valuesContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_all(&mut self, _ctx: &WithOptionsConstants_allContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_all(&mut self, _ctx: &WithOptionsConstants_allContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_indexer(
        &mut self,
        _ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_indexer(
        &mut self,
        _ctx: &WithOptionsConstants_indexerContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_list(&mut self, _ctx: &WithOptionsConstants_listContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_list(&mut self, _ctx: &WithOptionsConstants_listContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsconstants_map(&mut self, _ctx: &WithOptionsConstants_mapContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsconstants_map(&mut self, _ctx: &WithOptionsConstants_mapContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsconstants_reader(&mut self, _ctx: &IoOptionsConstants_readerContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsconstants_reader(&mut self, _ctx: &IoOptionsConstants_readerContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsconstants_writer(&mut self, _ctx: &IoOptionsConstants_writerContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsconstants_writer(&mut self, _ctx: &IoOptionsConstants_writerContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsconstants_gryo(&mut self, _ctx: &IoOptionsConstants_gryoContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsconstants_gryo(&mut self, _ctx: &IoOptionsConstants_gryoContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsconstants_graphson(
        &mut self,
        _ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsconstants_graphson(
        &mut self,
        _ctx: &IoOptionsConstants_graphsonContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsconstants_graphml(
        &mut self,
        _ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsconstants_graphml(
        &mut self,
        _ctx: &IoOptionsConstants_graphmlContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_connectedcomponentstringconstant(
        &mut self,
        _ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_connectedcomponentstringconstant(
        &mut self,
        _ctx: &ConnectedComponentStringConstantContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_pagerankstringconstant(&mut self, _ctx: &PageRankStringConstantContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_pagerankstringconstant(&mut self, _ctx: &PageRankStringConstantContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_peerpressurestringconstant(
        &mut self,
        _ctx: &PeerPressureStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_peerpressurestringconstant(
        &mut self,
        _ctx: &PeerPressureStringConstantContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_shortestpathstringconstant(
        &mut self,
        _ctx: &ShortestPathStringConstantContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_shortestpathstringconstant(
        &mut self,
        _ctx: &ShortestPathStringConstantContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_withoptionsstringconstant(&mut self, _ctx: &WithOptionsStringConstantContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_withoptionsstringconstant(&mut self, _ctx: &WithOptionsStringConstantContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_iooptionsstringconstant(&mut self, _ctx: &IoOptionsStringConstantContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_iooptionsstringconstant(&mut self, _ctx: &IoOptionsStringConstantContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_booleanargument(&mut self, _ctx: &BooleanArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_booleanargument(&mut self, _ctx: &BooleanArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_integerargument(&mut self, _ctx: &IntegerArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_integerargument(&mut self, _ctx: &IntegerArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringargument(&mut self, _ctx: &StringArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringargument(&mut self, _ctx: &StringArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringnullableargument(&mut self, _ctx: &StringNullableArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringnullableargument(&mut self, _ctx: &StringNullableArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringnullableargumentvarargs(
        &mut self,
        _ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringnullableargumentvarargs(
        &mut self,
        _ctx: &StringNullableArgumentVarargsContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_dateargument(&mut self, _ctx: &DateArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_dateargument(&mut self, _ctx: &DateArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericargument(&mut self, _ctx: &GenericArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericargument(&mut self, _ctx: &GenericArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericargumentvarargs(&mut self, _ctx: &GenericArgumentVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericargumentvarargs(&mut self, _ctx: &GenericArgumentVarargsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericmapargument(&mut self, _ctx: &GenericMapArgumentContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericmapargument(&mut self, _ctx: &GenericMapArgumentContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericmapnullableargument(
        &mut self,
        _ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericmapnullableargument(
        &mut self,
        _ctx: &GenericMapNullableArgumentContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalstrategyvarargs(&mut self, _ctx: &TraversalStrategyVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalstrategyvarargs(&mut self, _ctx: &TraversalStrategyVarargsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_traversalstrategyexpr(&mut self, _ctx: &TraversalStrategyExprContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_traversalstrategyexpr(&mut self, _ctx: &TraversalStrategyExprContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_classtypelist(&mut self, _ctx: &ClassTypeListContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_classtypelist(&mut self, _ctx: &ClassTypeListContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_classtypeexpr(&mut self, _ctx: &ClassTypeExprContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_classtypeexpr(&mut self, _ctx: &ClassTypeExprContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nestedtraversallist(&mut self, _ctx: &NestedTraversalListContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nestedtraversallist(&mut self, _ctx: &NestedTraversalListContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nestedtraversalexpr(&mut self, _ctx: &NestedTraversalExprContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nestedtraversalexpr(&mut self, _ctx: &NestedTraversalExprContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericcollectionliteral(&mut self, _ctx: &GenericCollectionLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericcollectionliteral(&mut self, _ctx: &GenericCollectionLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericliteralvarargs(&mut self, _ctx: &GenericLiteralVarargsContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericliteralvarargs(&mut self, _ctx: &GenericLiteralVarargsContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericliteralexpr(&mut self, _ctx: &GenericLiteralExprContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericliteralexpr(&mut self, _ctx: &GenericLiteralExprContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericmapnullableliteral(&mut self, _ctx: &GenericMapNullableLiteralContext<'input>) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericmapnullableliteral(&mut self, _ctx: &GenericMapNullableLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericrangeliteral(&mut self, _ctx: &GenericRangeLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericrangeliteral(&mut self, _ctx: &GenericRangeLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericsetliteral(&mut self, _ctx: &GenericSetLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericsetliteral(&mut self, _ctx: &GenericSetLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringnullableliteralvarargs(
        &mut self,
        _ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
    }
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringnullableliteralvarargs(
        &mut self,
        _ctx: &StringNullableLiteralVarargsContext<'input>,
    ) {
    }

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericliteral(&mut self, _ctx: &GenericLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericliteral(&mut self, _ctx: &GenericLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_genericmapliteral(&mut self, _ctx: &GenericMapLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_genericmapliteral(&mut self, _ctx: &GenericMapLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_mapkey(&mut self, _ctx: &MapKeyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_mapkey(&mut self, _ctx: &MapKeyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_mapentry(&mut self, _ctx: &MapEntryContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_mapentry(&mut self, _ctx: &MapEntryContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringliteral(&mut self, _ctx: &StringLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringliteral(&mut self, _ctx: &StringLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_stringnullableliteral(&mut self, _ctx: &StringNullableLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_stringnullableliteral(&mut self, _ctx: &StringNullableLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_integerliteral(&mut self, _ctx: &IntegerLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_integerliteral(&mut self, _ctx: &IntegerLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_floatliteral(&mut self, _ctx: &FloatLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_floatliteral(&mut self, _ctx: &FloatLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_numericliteral(&mut self, _ctx: &NumericLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_numericliteral(&mut self, _ctx: &NumericLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_booleanliteral(&mut self, _ctx: &BooleanLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_booleanliteral(&mut self, _ctx: &BooleanLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_dateliteral(&mut self, _ctx: &DateLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_dateliteral(&mut self, _ctx: &DateLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nullliteral(&mut self, _ctx: &NullLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nullliteral(&mut self, _ctx: &NullLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nanliteral(&mut self, _ctx: &NanLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nanliteral(&mut self, _ctx: &NanLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_infliteral(&mut self, _ctx: &InfLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_infliteral(&mut self, _ctx: &InfLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_uuidliteral(&mut self, _ctx: &UuidLiteralContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_uuidliteral(&mut self, _ctx: &UuidLiteralContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_nakedkey(&mut self, _ctx: &NakedKeyContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_nakedkey(&mut self, _ctx: &NakedKeyContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_classtype(&mut self, _ctx: &ClassTypeContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_classtype(&mut self, _ctx: &ClassTypeContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_variable(&mut self, _ctx: &VariableContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_variable(&mut self, _ctx: &VariableContext<'input>) {}

    /**
     * Enter a parse tree produced by \{@link GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn enter_keyword(&mut self, _ctx: &KeywordContext<'input>) {}
    /**
     * Exit a parse tree produced by \{@link  GremlinBaseParser#s}.
     * @param ctx the parse tree
     */
    fn exit_keyword(&mut self, _ctx: &KeywordContext<'input>) {}
}
