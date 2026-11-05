use super::*;

#[test]
fn correlation_discovery_reaches_each_merge_arm_without_inventing_bindings() {
    for position in 0..3 {
        let mut children=vec![Node::GraphOneRow,Node::GraphOneRow,Node::GraphOneRow];
        children[position]=Node::GraphCorrelate{bindings:vec!["outer".into(),"source".into()]};
        let create_arm=children.pop().unwrap().boxed();
        let match_arm=children.pop().unwrap().boxed();
        let input=children.pop().unwrap().boxed();
        let merge=Node::GraphMerge{correlation:vec!["outer".into(),"source".into()],outputs:vec![],input,match_arm,create_arm};
        assert_eq!(node_children(&merge).len(),3);
        assert_eq!(first_correlate_bindings(&merge),Some(vec!["outer".into(),"source".into()]));
    }
    for leaf in [Node::GraphOneRow,Node::GraphEmpty,Node::GraphCorrelate{bindings:vec![]}] {
        assert!(node_children(&leaf).is_empty());
        let expected=if matches!(leaf,Node::GraphCorrelate{..}) {Some(vec![])} else {None};
        assert_eq!(first_correlate_bindings(&leaf),expected);
    }
}

#[test]
fn cypher_plan_walk_finds_mutations_procedures_and_collection_inputs() {
    use crate::language::cypher::{parser::parse_query,planner::lower_query};
    let mut failures = Vec::new();
    for (query,required) in [
        ("CREATE (n:Person {name:'Ada'}) RETURN n","GraphCreate"),
        ("MERGE (n:Person {name:'Ada'}) RETURN n","GraphMerge"),
        ("MATCH (n) SET n.name='Ada' RETURN n","GraphSetProperty"),
        ("MATCH (n) DETACH DELETE n","GraphDelete"),
        ("UNWIND [1,2] AS n RETURN n","GraphUnwind"),
        ("CALL db.labels() YIELD label RETURN label","GraphProcedureCall"),
        ("UNWIND [1,2] AS n RETURN n SKIP $offset LIMIT $take", "GraphSliceExpr"),
        ("UNWIND [1,2] AS n WITH rand() AS x WHERE EXISTS { RETURN n + x AS y } RETURN x", "GraphBarrier"),
        ("RETURN all(x IN $items WHERE x > $threshold) AS value", "GraphQuantifier"),
        ("RETURN [x IN [1,2] | EXISTS { RETURN x }] AS values", "GraphApply"),
        ("RETURN [(a)-->(b) | b] AS values", "GraphCollect"),
    ] {
        let parsed = match parse_query(query) {
            Ok(parsed) => parsed,
            Err(error) => { failures.push(format!("{query}: {error}")); continue; }
        };
        let plan = match lower_query(&parsed) {
            Ok(plan) => plan,
            Err(error) => { failures.push(format!("{query}: {error}")); continue; }
        };
        let mut pending=vec![plan.root.as_ref()];let mut found=false;let mut count=0;
        while let Some(node)=pending.pop() {
            count+=1;assert!(count<100,"walk failed to terminate: {query}");
            found|=unsupported_node_name(node)==required;
            pending.extend(node_children(node));
        }
        if !found { failures.push(format!("missing {required} under {query}")); }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
