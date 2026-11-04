use super::*;
use crate::ir::plan::ProjectErrorPolicy;

#[test]
fn path_absence_proof_tracks_projection_scope_and_preserving_operators() {
    let bound=Node::GraphValues {bindings:vec!["p".into()],rows:vec![],bulk:None};
    assert!(!absent_path(&bound,"p"));
    assert!(absent_path(&Node::GraphOneRow,"p"));
    assert!(!absent_path(&Node::GraphEmpty,"p")); // Unknown nodes cannot prove scope.
    for present in [false,true] {
        let input=if present {bound.clone()}else{Node::GraphOneRow};
        let filtered=Node::GraphFilter {condition:IrExpr::Lit(Lit::Bool(true)),input:input.boxed()};
        let sorted=Node::GraphSort {keys:vec![],input:filtered.boxed()};
        let sliced=Node::GraphSlice {slice:Slice {offset:0,fetch:Some(1),tail:None},input:sorted.boxed()};
        assert_eq!(absent_path(&sliced,"p"),!present);
        for mode in [ProjectMode::PreserveVisible,ProjectMode::ReplaceScope] {
            for alias in ["p","other"] {
                let projected=Node::GraphProject {mode,items:vec![ProjectionItem {alias:alias.into(),expr:IrExpr::Lit(Lit::Null)}],
                    error_policy:ProjectErrorPolicy::PropagateError,input:sliced.clone().boxed()};
                assert_eq!(absent_path(&projected,"p"),alias!="p" && (mode==ProjectMode::ReplaceScope || !present));
            }
        }
    }
}

#[test]
fn path_absence_proof_respects_every_expansion_output_and_previous_scope() {
    use crate::language::cypher::{parser::parse_query,planner::CypherPlanner};
    fn expansion(node: &Node) -> Option<Node> {
        if matches!(node,Node::GraphExpand{..}) {return Some(node.clone());}
        crate::ir::analysis::children(node).into_iter().find_map(expansion)
    }
    let graph=CypherPlanner::new().plan(&parse_query("MATCH p=(a)-[r:R]->(b) RETURN p").unwrap()).unwrap();
    let template=expansion(&graph.root).unwrap();
    for producer in ["path","target","relationship","input","none"] {
        let mut node=template.clone();
        let Node::GraphExpand{path,target,rel_binding,input,..}=&mut node else {unreachable!()};
        *path=Some(if producer=="path" {"watched"}else{"p"}.into());
        *target=if producer=="target" {"watched"}else{"b"}.into();
        *rel_binding=Some(if producer=="relationship" {"watched"}else{"r"}.into());
        *input=if producer=="input" {Node::GraphCorrelate{bindings:vec!["watched".into()]}}else{Node::GraphOneRow}.boxed();
        assert_eq!(absent_path(&node,"watched"),producer=="none","{producer}");
    }
}
