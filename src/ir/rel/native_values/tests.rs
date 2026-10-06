use super::*;
use serde_json::json;

fn element(id: i64, properties: serde_json::Value) -> serde_json::Value {
    json!({elements::FIELD: {"kind":"node", "id":id,"label":"Person","properties":properties}})
}

#[test]
fn current_projection_preserves_productive_map_null_and_drops_absent_element_property() {
    let expression = serde_json::to_string(&IrExpr::property("current", "name", crate::ir::policy::PropertyMissing::DropUnproductive)).unwrap();
    assert_eq!(project(&expression, &json!({"current":{}}), 0, 0).unwrap(), json!([null]));
    assert_eq!(project(&expression, &json!({"current":{"Name":"case-sensitive"}}), 0, 0).unwrap(), json!([null]));
    assert_eq!(project(&expression, &json!({"current":element(1,json!({}))}), 0, 0).unwrap(), json!([]));
    let result = project(&expression, &json!({"current":element(1,json!({"name":"Ada"}))}), 0, 0).unwrap();
    assert_eq!(json(&result[0]).unwrap(), json!({"type":"string","value":"Ada"}));
    let null = serde_json::to_string(&IrExpr::Lit(Lit::Null)).unwrap();
    assert_eq!(project(&null, &json!({}), 0, 0).unwrap(), json!([]));
}

#[test]
fn equality_keys_follow_language_numeric_and_element_identity_rules() {
    let integer = json!({"__orchiddb_number":["INTEGER","1"]});
    let double = json!({"__orchiddb_number":["DOUBLE","1"]});
    assert_eq!(key(&integer,true).unwrap(),key(&double,true).unwrap());
    assert_ne!(key(&integer,false).unwrap(),key(&double,false).unwrap());
    let a = element(1,json!({"name":"Ada"}));
    let b = element(1,json!({"age":30}));
    for cypher in [false,true] { assert_eq!(key(&a,cypher).unwrap(),key(&b,cypher).unwrap()); }
}

#[test]
fn scalar_round_trip_retains_union_of_projected_element_properties() {
    let input = json!([element(1,json!({"name":"Ada"})),element(1,json!({"age":30}))]);
    let mut context = Context::default();
    let Value::List(values) = from_json(&input,&mut context).unwrap() else { panic!("expected values") };
    let output=json(&context.wire(&values[1])).unwrap();
    assert_eq!(output["properties"]["name"],json!({"type":"string","value":"Ada"}));
    assert_eq!(output["properties"]["age"]["value"],json!(30));
}

#[test]
fn sort_reuses_language_comparator_and_returns_original_row_permutation() {
    let spec=SortSpecification {
        keys:vec![SortKey {expr:IrExpr::Call{name:"gremlin_order_key".into(),args:vec![IrExpr::binding("value")]},dir:SortDir::Asc,nulls:NullsOrder::ProviderDefined}],
        scopes:vec!["scope".into()],
    };
    let input=json!([
        {"payload":"ten","scope":{"value":{"__orchiddb_number":["INTEGER","10"]}}},
        {"payload":"two","scope":{"value":{"__orchiddb_number":["DOUBLE","2"]}}},
        {"payload":"negative","scope":{"value":{"__orchiddb_number":["BIGINT","-1"]}}},
    ]);
    assert_eq!(sort(&serde_json::to_string(&spec).unwrap(),&input).unwrap(),vec![2,1,0]);
}

#[test]
fn program_arrow_boundary_preserves_null_bulk_and_property_record_context() {
    use crate::ir::catalog::Cardinality;
    use crate::ir::policy::{GraphPlanPolicy, ResultForm};
    let graph = PropertyGraph::default();
    graph.enable_null_property_values(true);
    let node = graph.insert_node("Person", BTreeMap::new());
    graph.set_element_public_id(&node, Value::String("person-1".into())).unwrap();
    graph.set_node_labels(&node, ["Person".into(), "Employee".into()]).unwrap();
    let property = graph.set_vertex_property(&node,"name".into(),Value::Null,Cardinality::List,
        BTreeMap::from([("since".into(),Value::Int(2000))])).unwrap();
    graph.set_vertex_property_public_id(&property,Value::Long(73)).unwrap();
    let mut row = KernelRow::new().with("current",node.clone());
    row.bulk = 2;
    let rows = vec![row,KernelRow::new().with("current",Value::Null)];
    let fields=vec!["current".into()];
    let batch=result_batch(&fields,ResultForm::TraverserStream,&rows,&graph,&GraphPlanPolicy::gremlin()).unwrap();
    assert_eq!(batch.num_rows(),3);
    assert!(batch.column(0).is_null(2));
    let decoded=decode_rows(&batch,&fields,&graph).unwrap();
    assert_eq!(decoded[0].get("current"),node);
    assert_eq!(decoded[1].get("current"),node);
    assert_eq!(decoded[2].get("current"),Value::Null);
    let structs=batch.column(0).as_any().downcast_ref::<arrow::array::StructArray>().unwrap();
    let strings=structs.column(0).as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
    let mut context=Context::default();
    let node=context.decode(strings.value(0)).unwrap();
    let properties=context.graph.properties(&node,&["name".into()]);
    assert_eq!(properties.len(),1);
    assert_eq!(context.graph.element_public_id(&properties[0]),Value::Long(73));
    assert_eq!(context.graph.properties(&properties[0],&["since".into()]).len(),1);
    let Value::Node{label,id}=node else {panic!("node")};
    assert_eq!(context.graph.node_labels(&label,id),vec!["Employee".to_string(),"Person".to_string()]);
}

#[test]
fn detached_output_keeps_user_id_properties_separate_from_identity() {
    let graph = PropertyGraph::default();
    let node = graph.insert_node("Person", BTreeMap::from([("id".into(),Value::Int(42))]));
    let other = graph.insert_node("Person", BTreeMap::new());
    let edge = graph.insert_edge("KNOWS", &node, &other, BTreeMap::from([("id".into(),Value::Int(73))])).unwrap();
    let mut context = Context::default();
    context.capture(&Value::List(vec![node.clone(),edge.clone()]), &graph).unwrap();
    assert_eq!(json(&context.wire(&node)).unwrap()["properties"]["id"],json!({"type":"int","value":42}));
    assert_eq!(json(&context.wire(&edge)).unwrap()["properties"]["id"],json!({"type":"int","value":73}));
}
