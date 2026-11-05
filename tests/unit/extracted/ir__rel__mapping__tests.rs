
    use super::*;

    #[test]
    fn query_function_resolution_preserves_case_aliases_and_namespaces() {
        let mapping = GraphMapping::new();
        let provider = MappingContextProvider::new(&mapping);
        assert_eq!(provider.get_function_meta("ChAr_LeNgTh").unwrap().name(),"character_length");
        assert_eq!(provider.get_aggregate_meta("MeAn").unwrap().name(), "avg");
        assert_eq!(
            provider.get_window_meta("RoW_NuMbEr").unwrap().name(),
            "row_number"
        );
        assert!(provider.get_function_meta("row_number").is_none());
        assert!(provider.get_aggregate_meta("character_length").is_none());
        assert!(provider.get_window_meta("__missing").is_none());
        // Table dependency tracking remains scoped to each mapping query.
        let other = MappingContextProvider::new(&mapping);
        provider.requested.borrow_mut().insert("customers".into());
        assert!(other.requested.borrow().is_empty());
        mapping
            .plan_sql("SELECT ChAr_LeNgTh('hello'), MeAn(3) OVER (), RoW_NuMbEr() OVER ()")
            .unwrap();
        mapping.plan_sql("SELECT sqrt(9.0), char_length('hello')").unwrap();
        mapping.plan_sql("SELECT fn.sqrt(9.0), fn.char_length('hello')").unwrap();
        assert!(provider.get_function_meta("fn.sqrt").is_some());
        assert!(provider.udf_names().contains(&"fn.sqrt".into()));
    }

    #[test]
    fn toml_round_trip() {
        let toml = r#"
# BYOS mapping
[node.Person]
table = "customers"
id = "cust_id"

[node.Person.properties]
name = "full_name"
age = "age"

[node.Vip]
query = "SELECT cust_id, full_name FROM customers WHERE age >= 30"
id = "cust_id"

[node.Vip.properties]
name = "full_name"

[edge.ORDERED]
table = "orders"
src = "cust_id"
dst = "order_id"
src_label = "Person"
dst_label = "Order"
edge_id = "order_id"

[edge.ORDERED.properties]
total = "total"
"#;
        let mapping = GraphMapping::from_toml(toml).expect("parse");
        let person = mapping.node("Person").expect("person");
        assert_eq!(person.source, MappedSource::Table("customers".into()));
        assert_eq!(person.id_column.columns(), &["cust_id"]);
        assert_eq!(person.properties.get("name").unwrap(), "full_name");
        let vip = mapping.node("Vip").expect("vip");
        assert!(matches!(vip.source, MappedSource::Query(_)));
        let ordered = mapping.edge("ORDERED").expect("ordered");
        assert_eq!(ordered.src_label, "Person");
        assert_eq!(
            ordered.id_column.as_ref().map(KeyColumns::columns),
            Some(["order_id".to_string()].as_slice())
        );
        assert_eq!(ordered.properties.get("total").unwrap(), "total");

        let reparsed = GraphMapping::from_toml(&mapping.to_toml()).expect("reparse");
        assert_eq!(reparsed.nodes, mapping.nodes);
        assert_eq!(reparsed.edges, mapping.edges);
    }

    #[test]
    fn toml_rejects_bad_sections_and_values() {
        assert!(GraphMapping::from_toml("[wat.Person]\ntable = \"t\"").is_err());
        assert!(GraphMapping::from_toml("[node.Person]\nid = \"x\"").is_err());
        assert!(
            GraphMapping::from_toml("[node.Person]\ntable = \"t\"\nquery = \"q\"\nid = \"x\"")
                .is_err()
        );
        assert!(GraphMapping::from_toml("[node.Person]\ntable = unquoted\nid = \"x\"").is_err());
    }

    #[test]
    fn query_planning_resolves_registered_tables() {
        use arrow::datatypes::{DataType, Field, Schema};
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "customers",
            Arc::new(Schema::new(vec![
                Field::new("cust_id", DataType::Int64, false),
                Field::new("age", DataType::Int64, true),
            ])),
        );
        let plan = mapping
            .plan_sql("SELECT cust_id FROM customers WHERE age > 30")
            .expect("plan view sql");
        assert!(format!("{}", plan.display_indent()).contains("TableScan: customers"));
        assert!(mapping.plan_sql("SELECT * FROM missing").is_err());
    }

#[test]
fn mapped_views_track_transitive_dependencies_and_reject_cycles_atomically() {
    use arrow::datatypes::{Field,Schema};
    let mut mapping=GraphMapping::new();
    mapping.register_table_schema("base",Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,false)])));
    mapping.register_view("first","SELECT id FROM base").unwrap();
    mapping.register_view("second","SELECT id FROM first").unwrap();
    mapping.register_view("diamond","SELECT a.id FROM first a JOIN second b ON a.id=b.id").unwrap();
    assert_eq!(mapping.view_dependencies["first"],BTreeSet::from(["base".into()]));
    assert_eq!(mapping.view_dependencies["second"],BTreeSet::from(["first".into()]));
    assert_eq!(mapping.table_schema("diamond").unwrap().field(0).data_type(),&DataType::Int64);
    for sql in ["SELECT id FROM first","SELECT id FROM second","SELECT id FROM diamond"] {
        let before=mapping.view_dependencies.clone();
        let provider=mapping.tables["first"].clone();
        assert!(mapping.register_view("first",sql).unwrap_err().to_string().contains("cyclic mapped view"));
        assert_eq!(mapping.view_dependencies,before);
        assert!(Arc::ptr_eq(&mapping.tables["first"],&provider));
    }
    let before=mapping.view_dependencies.clone();
    assert!(mapping.register_view("bad","SELECT id FROM nonexistent").is_err());
    assert!(!mapping.tables.contains_key("bad"));assert_eq!(mapping.view_dependencies,before);
    mapping.register_view("first","SELECT id FROM base WHERE id > 10").unwrap();
    assert_eq!(mapping.view_dependencies["first"],BTreeSet::from(["base".into()]));
    assert!(format!("{mapping:?}").contains("diamond"));
}

#[test]
fn mapped_source_metadata_validation_does_not_mutate_existing_registration() {
    use crate::ir::rel::{source_metadata::{SourceMetadata,IndexMetadata},search::SearchMetric};
    let index=IndexMetadata {column:"vector".into(),metric:SearchMetric::Cosine,options:Default::default()};
    let valid=SourceMetadata {table:"vectors".into(),format:Some("lance".into()),options:Default::default(),indexes:vec![index.clone()]};
    let mut mapping=GraphMapping::new();
    for invalid in [SourceMetadata {table:String::new(),..valid.clone()},
        SourceMetadata {format:Some(String::new()),..valid.clone()},
        SourceMetadata {indexes:vec![index.clone(),index.clone()],..valid.clone()},
        SourceMetadata {indexes:vec![IndexMetadata {column:String::new(),..index.clone()}],..valid.clone()}] {
        assert!(mapping.register_source_metadata(invalid).is_err());
        assert!(mapping.source_metadata.is_empty());
    }
    mapping.register_source_metadata(valid.clone()).unwrap();
    assert!(mapping.register_source_metadata(valid.clone()).unwrap_err().to_string().contains("duplicate source metadata"));
    assert_eq!(mapping.source_metadata["vectors"],valid);
}

#[test]
fn mapped_search_index_merging_preserves_source_options_and_rejects_duplicates() {
    use crate::ir::rel::search::{SearchIndex,SearchMetric,LegacySearchOptions};
    let index=SearchIndex {table:"vectors".into(),column:"embedding".into(),metric:SearchMetric::Cosine,
        backend:LegacySearchOptions {kind:"lance".into(),options:BTreeMap::from([
            ("uri".into(),serde_json::json!("memory://vectors")),("nprobes".into(),serde_json::json!(8)),
        ])}};
    let mut mapping=GraphMapping::new();mapping.register_search_index(index.clone()).unwrap();
    let original=mapping.source_metadata["vectors"].clone();
    for invalid in [index.clone(),SearchIndex {column:String::new(),..index.clone()},
        SearchIndex {backend:LegacySearchOptions {kind:"different".into(),options:index.backend.options.clone()},..index.clone()}] {
        assert!(mapping.register_search_index(invalid).is_err());
        assert_eq!(mapping.source_metadata["vectors"],original);
    }
    mapping.register_search_index(SearchIndex {metric:SearchMetric::Dot,..index}).unwrap();
    let stored=&mapping.source_metadata["vectors"];
    assert_eq!(stored.indexes.len(),2);
    assert_eq!(stored.options,BTreeMap::from([("uri".into(),serde_json::json!("memory://vectors"))]));
    for index in &stored.indexes {assert_eq!(index.options["nprobes"],serde_json::json!(8));}
}

#[test]
fn mapped_logical_labels_cannot_alias_keys_or_properties() {
    for column in ["","id","name"] {
        let mut mapping=GraphMapping::new();
        mapping.map_node(NodeMapping::table("Person","people","id").property("display","name").labels_column(column));
        assert!(mapping.validate_foreign_keys().unwrap_err().to_string().contains("logical label column must be separate"));
    }
    let mut mapping=GraphMapping::new();
    mapping.map_node(NodeMapping::table("Person","people","id").property("display","name").labels_column("labels"));
    mapping.validate_foreign_keys().unwrap();
}

#[test]
fn mapped_key_columns_reject_nested_components_and_keep_declared_order() {
    use arrow::datatypes::{Field,Schema};
    let columns=KeyColumns::from(["tenant","id"]);
    let schema=Schema::new(vec![Field::new("id",DataType::UInt64,false),Field::new("tenant",DataType::Utf8,false)]);
    let DataType::Struct(fields)=columns.data_type(&schema).unwrap() else {panic!("composite key")};
    assert_eq!(fields[0].name(),"k0");assert_eq!(fields[0].data_type(),&DataType::Utf8);
    assert_eq!(fields[1].name(),"k1");assert_eq!(fields[1].data_type(),&DataType::UInt64);
    assert!(columns.values(&crate::ir::ElementId::from(7_i64)).unwrap_err().contains("component count"));
    for kind in [DataType::Null,DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
        DataType::Struct(vec![Field::new("part",DataType::Int64,false)].into())] {
        let schema=Schema::new(vec![Field::new("id",kind,false)]);
        assert!(KeyColumns::from("id").data_type(&schema).unwrap_err().contains("scalar identity type"));
    }
    assert!(KeyColumns::from("missing").data_type(&schema).is_err());
    assert!(is_identity_type(&DataType::Struct(fields)));
    assert!(!is_identity_type(&DataType::Struct(vec![Field::new("only",DataType::Int64,false)].into())));
    assert!(!is_identity_type(&DataType::Struct(vec![Field::new("one",DataType::Int64,false),Field::new("bad",DataType::Null,true)].into())));
}

#[test]
fn mapped_label_algebra_preserves_empty_and_negated_sets() {
    for (expr,label,expected) in [
        (LabelExpr::Any,"",true),
        (LabelExpr::AnyOf(vec![]),"Person",false),
        (LabelExpr::AllOf(vec![]),"Person",true),
        (LabelExpr::AllOf(vec!["Person".into(),"Person".into()]),"Person",true),
        (LabelExpr::AllOf(vec!["Person".into(),"Other".into()]),"Person",false),
        (LabelExpr::AllOf(vec![String::new()]),"",false),
        (LabelExpr::Not(Box::new(LabelExpr::AnyOf(vec!["Person".into()]))),"Other",true),
        (LabelExpr::Not(Box::new(LabelExpr::Any)),"Person",false),
    ] {assert_eq!(static_labels_match(&expr,label),expected,"{expr:?}/{label}");}
    assert_eq!(mapped_label_predicate(&LabelExpr::AnyOf(vec![]),&|_|panic!("empty set")),lit(false));
    assert_eq!(mapped_label_predicate(&LabelExpr::AllOf(vec![]),&|_|panic!("empty set")),lit(true));
    let pred=mapped_label_predicate(&LabelExpr::Not(Box::new(LabelExpr::AnyOf(vec!["A".into(),"B".into()]))),&|name|col_exact(name));
    assert_eq!(pred,Expr::Not(Box::new(col_exact("A").or(col_exact("B")))));
}

#[test]
fn mapped_source_column_resolution_prefers_exact_names_and_diagnoses_missing_fields() {
    use arrow::datatypes::{Field,Schema};
    let plan=LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {produce_one_row:false,
        schema:Arc::new(DFSchema::try_from(Schema::new(vec![Field::new("ID",DataType::Int64,false),Field::new("Name",DataType::Utf8,true),Field::new("name",DataType::Boolean,true)])).unwrap())});
    assert_eq!(resolve_column(&plan,"id").unwrap(),"ID");
    assert_eq!(resolve_column(&plan,"Name").unwrap(),"Name");
    assert_eq!(resolve_column(&plan,"name").unwrap(),"name");
    assert_eq!(resolve_column(&plan,"NAME").unwrap(),"NAME","ambiguous folded names must remain unresolved");
    assert_eq!(resolve_column(&plan,"missing").unwrap(),"missing");
    let error=source_column_type(&plan,"missing","node key").unwrap_err().to_string();
    for fragment in ["node key","missing","ID","Name","name"] {assert!(error.contains(fragment),"{error}");}
    assert_eq!(schema_field_type(plan.schema(),"Name"),Some(DataType::Utf8));
    assert_eq!(schema_field_type(plan.schema(),"missing"),None);
}

#[test]
fn mapped_query_boundaries_reject_multiple_statements_and_keep_dependencies_scoped() {
    let mapping=GraphMapping::new();
    for (sql,count) in [("",0),(";",0),("SELECT 1; SELECT 2",2)] {
        let error=mapping.plan_sql(sql).unwrap_err().to_string();
        assert!(error.contains(&format!("single statement, got {count}")),"{error}");
    }
    assert!(mapping.plan_sql("SELECT (").unwrap_err().to_string().contains("mapping query parse"));
    assert!(mapping.plan_sql("SELECT missing FROM absent").unwrap_err().to_string().contains("mapping query plan"));
    let (_,dependencies)=mapping.plan_sql_with_dependencies("SELECT 1 AS id").unwrap();assert!(dependencies.is_empty());
    let provider=MappingContextProvider::new(&mapping);
    assert!(provider.get_variable_type(&["parameter".into()]).is_none());
    assert!(mapping.collection_sql("unregistered").is_none());
    assert!(mapping.table_schema("unregistered").is_none());
}

#[test]
fn mapped_scan_name_resolution_deduplicates_and_reports_unsupported_label_forms() {
    assert_eq!(resolve_names(&LabelExpr::Any,||vec!["Z".into(),"A".into(),"A".into()],"node").unwrap(),vec!["A","Z"]);
    assert_eq!(resolve_names(&LabelExpr::AnyOf(vec!["B".into(),"A".into(),"B".into()]),||panic!(),"node").unwrap(),vec!["A","B"]);
    assert_eq!(resolve_names(&LabelExpr::AllOf(vec!["A".into()]),||panic!(),"node").unwrap(),vec!["A"]);
    for labels in [vec![],vec!["A".into(),"B".into()]] {
        assert!(resolve_names(&LabelExpr::AllOf(labels),||panic!(),"relationship").unwrap_err().to_string().contains("multi-relationship scan"));
    }
    assert!(resolve_names(&LabelExpr::Not(Box::new(LabelExpr::Any)),||panic!(),"node").unwrap_err().to_string().contains("negated node scan"));
}

#[test]
fn mapped_cypher_relationship_arguments_create_isolated_collision_free_instances() {
    use crate::catalog::{CypherRelationship,RelationshipParameter,RelationshipReturns};
    let rule=CypherRelationship {name:"NEAR".into(),description:String::new(),source:"Person".into(),target:"Person".into(),
        parameters:vec![RelationshipParameter {name:"k".into(),schema:serde_json::json!({"type":"integer","minimum":1}),default:Some(serde_json::json!(1))}],
        cypher:"WITH source MATCH (target:Person) RETURN target LIMIT $k".into(),
        returns:RelationshipReturns {target:"target".into(),properties:Default::default()}};
    let mut mapping=GraphMapping::new();
    mapping.map_edge(EdgeMapping::new("NEAR",MappedSource::Cypher(Box::new(rule.clone())),"src","dst","Person","Person"));
    mapping.map_edge(EdgeMapping::table("__orchid_edge_arguments_0","occupied","src","dst","Person","Person"));
    let supplied=BTreeMap::from([("k".into(),serde_json::json!(3))]);
    let alias=mapping.bind_edge_arguments("NEAR",&supplied).unwrap();
    assert_eq!(alias,"__orchid_edge_arguments_1");
    let MappedSource::Cypher(bound)=&mapping.edge(&alias).unwrap().source else {panic!()};
    assert_eq!(bound.parameters[0].default,Some(serde_json::json!(3)));
    assert_eq!(mapping.edge("NEAR").unwrap().source,MappedSource::Cypher(Box::new(rule)));
    let second=mapping.bind_edge_arguments("NEAR",&BTreeMap::new()).unwrap();
    assert_eq!(second,"__orchid_edge_arguments_2");
    let MappedSource::Cypher(default)=&mapping.edge(&second).unwrap().source else {panic!()};
    assert_eq!(default.parameters[0].default,Some(serde_json::json!(1)));
    let count=mapping.edge_instances.len();
    for bad in [BTreeMap::from([("k".into(),serde_json::json!(0))]),BTreeMap::from([("extra".into(),serde_json::json!(1))])] {
        assert!(mapping.bind_edge_arguments("NEAR",&bad).is_err());
        assert_eq!(mapping.edge_instances.len(),count);
    }
    assert!(mapping.bind_edge_arguments("missing",&BTreeMap::new()).unwrap_err().contains("unknown relationship"));
    assert!(mapping.bind_edge_arguments("__orchid_edge_arguments_0",&BTreeMap::new()).unwrap_err().contains("does not declare parameters"));
}

#[test]
fn mapped_source_argument_failures_do_not_install_partial_tables_or_instances() {
    use crate::catalog::RelationshipParameter;
    use crate::ir::rel::source_metadata::SourceMetadata;
    let mut mapping=GraphMapping::new();
    assert!(mapping.bind_node_arguments("missing",&BTreeMap::new()).unwrap_err().contains("unknown node"));
    let mut node=NodeMapping::query("Person","SELECT 1 AS id","id");
    node.parameters=vec![RelationshipParameter {name:"limit".into(),schema:serde_json::json!({"type":"integer"}),default:Some(serde_json::json!(1))}];
    mapping.map_node(node.clone());assert!(mapping.has_source_parameters());
    assert!(mapping.bind_node_arguments("Person",&BTreeMap::new()).unwrap_err().contains("table-backed mapping"));
    node.source=MappedSource::Table("people".into());mapping.map_node(node);
    assert!(mapping.bind_node_arguments("Person",&BTreeMap::new()).unwrap_err().contains("source metadata is required"));
    mapping.register_source_metadata(SourceMetadata {table:"people".into(),format:None,options:Default::default(),indexes:vec![]}).unwrap();
    assert!(mapping.bind_node_arguments("Person",&BTreeMap::new()).unwrap_err().contains("source format is required"));
    mapping.source_metadata.get_mut("people").unwrap().format=Some("duckdb".into());
    assert!(mapping.bind_node_arguments("Person",&BTreeMap::new()).unwrap_err().contains("does not support source arguments"));
    assert!(mapping.node_instances.is_empty());assert!(mapping.bound_table_sources.is_empty());assert!(mapping.tables.is_empty());
}

#[test]
fn mapped_bound_node_labels_are_canonicalized_under_negation() {
    let mut mapping=GraphMapping::new();
    mapping.map_node(NodeMapping::table("Person","people","id"));
    mapping.node_instances.insert("instance".into(),NodeMapping::table("Person","bound_people","id"));
    let labels=LabelExpr::Not(Box::new(LabelExpr::AnyOf(vec!["instance".into(),"Other".into()])));
    assert_eq!(mapping.canonical_node_labels(&labels),LabelExpr::Not(Box::new(LabelExpr::AnyOf(vec!["Person".into(),"Other".into()]))));
    assert_eq!(mapping.node_scan_candidates(&LabelExpr::Any).unwrap().len(),1);
    assert_eq!(mapping.node_scan_candidates(&labels).unwrap()[0].source,MappedSource::Table("people".into()));
    assert_eq!(mapping.node_scan_candidates(&LabelExpr::AllOf(vec!["instance".into()])).unwrap()[0].source,MappedSource::Table("bound_people".into()));
    assert!(mapping.node_scan_candidates(&LabelExpr::AllOf(vec!["instance".into(),"Person".into()])).unwrap_err().to_string().contains("one explicit node label"));
}

#[cfg(feature="duckdb")]
#[tokio::test]
async fn mapped_nested_views_execute_filters_without_losing_duplicate_rows() {
    use arrow::{array::Int64Array,datatypes::{Field,Schema},record_batch::RecordBatch};
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,true)]));
    let batch=RecordBatch::try_new(schema.clone(),vec![Arc::new(Int64Array::from(vec![Some(1),Some(7),Some(7),None,Some(9)]))]).unwrap();
    let mut mapping=GraphMapping::new();
    mapping.register_table("base",Arc::new(MemTable::try_new(schema,vec![vec![batch]]).unwrap()));
    mapping.register_view("first","SELECT id FROM base WHERE id >= 7").unwrap();
    mapping.register_view("second","SELECT id AS renamed FROM first WHERE id < 9").unwrap();
    let plan=mapping.plan_sql("SELECT renamed FROM second").unwrap();
    let batches=crate::ir::rel::execute_test_plan(plan).await.unwrap();
    let values=batches.iter().flat_map(|batch|(0..batch.num_rows()).map(move |row|ScalarValue::try_from_array(batch.column(0),row).unwrap())).collect::<Vec<_>>();
    assert_eq!(values,vec![ScalarValue::Int64(Some(7));2]);
}

#[test]
fn mapped_cypher_relationship_registration_is_atomic_and_plans_bound_arguments() {
    use arrow::datatypes::{Field,Schema};
    let mut mapping=GraphMapping::new();
    mapping.register_table_schema("people",Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,false)])));
    mapping.map_node(NodeMapping::table("Person","people","id").property("id","id"));
    let rule:crate::catalog::CypherRelationship=serde_json::from_value(serde_json::json!({
        "name":"NEAR","source":"Person","target":"Person",
        "parameters":[{"name":"k","schema":{"type":"integer","minimum":1},"default":1}],
        "cypher":"WITH source MATCH (target:Person) WHERE target.id <> source.id RETURN target ORDER BY target.id LIMIT $k",
        "returns":{"target":"target","properties":{}}
    })).unwrap();
    mapping.map_cypher_relationship(rule.clone()).unwrap();
    assert!(mapping.edge("NEAR").is_some());
    let alias=mapping.bind_edge_arguments("NEAR",&BTreeMap::from([("k".into(),serde_json::json!(2))])).unwrap();
    let plan=mapping.source_plan(&mapping.edge(&alias).unwrap().source).unwrap();
    assert!(!plan.schema().fields().is_empty());
    let edges=mapping.edges.clone();let tables=mapping.physical_table_names();
    let mut bad=rule;bad.name="BROKEN".into();bad.target="Missing".into();
    assert!(mapping.map_cypher_relationship(bad).is_err());
    assert_eq!(mapping.edges,edges);assert_eq!(mapping.physical_table_names(),tables);
    assert!(mapping.edge("BROKEN").is_none());
}

fn computed_boundary_mapping() -> GraphMapping {
    use arrow::datatypes::{Field, Schema};
    let mut mapping = GraphMapping::new();
    for (label, table) in [("Source", "sources"), ("Target", "targets")] {
        mapping.register_table_schema(table, Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("time", DataType::Date32, true),
            Field::new("group", DataType::Int64, true),
            Field::new("score", DataType::Int64, true),
        ])));
        mapping.map_node(NodeMapping::table(label, table, "id")
            .property("time", "time").property("group", "group").property("score", "score"));
    }
    mapping
}
fn computed_boundary_rule() -> ComputedRelationship {
    serde_json::from_value(serde_json::json!({"name":"LINK","source":"Source","target":"Target"})).unwrap()
}

#[test]
fn computed_relationship_registration_rejects_invalid_definitions_without_partial_edges() {
    use serde_json::json;
    for (field, value, expected) in [
        ("name", json!(""), "nonempty and unique"),
        ("source", json!("Missing"), "unknown source label"),
        ("target", json!("Missing"), "unknown target label"),
        ("limit_per_source", json!(1), "requires order_by"),
        ("properties", json!({"":"source.score"}), "property name must be nonempty"),
        ("predicate", json!("TRUE; SELECT 1"), "trailing SQL"),
        ("candidates", json!({"limit_per_source":1}), "requires order_by"),
    ] {
        let mut mapping = computed_boundary_mapping();
        let before = mapping.to_toml();
        let mut definition = serde_json::to_value(computed_boundary_rule()).unwrap();
        definition[field] = value;
        let error = mapping.map_computed_relationship(serde_json::from_value(definition).unwrap()).err().unwrap();
        assert!(error.to_string().contains(expected), "{field}: {error}");
        assert_eq!(mapping.to_toml(), before);
    }
    let mut mapping = computed_boundary_mapping();
    mapping.map_computed_relationship(computed_boundary_rule()).unwrap();
    let before = mapping.to_toml();
    assert!(mapping.map_computed_relationship(computed_boundary_rule()).err().unwrap().to_string().contains("unique"));
    assert_eq!(mapping.to_toml(), before);
    mapping.validate_computed_relationships().unwrap();
    for label in ["Source", "Target"] {
        let mut mapping = computed_boundary_mapping();
        mapping.nodes.get_mut(label).unwrap().source = MappedSource::Computed(Box::new(computed_boundary_rule()));
        let before = mapping.node(label).unwrap().clone();
        let error = mapping.map_computed_relationship(computed_boundary_rule()).err().unwrap();
        assert!(error.to_string().contains("require node sources"), "{error}");
        assert_eq!(mapping.node(label).unwrap(), &before);
        assert!(mapping.edges.is_empty());
    }
}

#[test]
fn computed_relationship_as_of_rejects_competing_selection_and_unmapped_properties() {
    use arrow::datatypes::{Field, Schema};
    use serde_json::json;
    let base = json!({"name":"LINK","source":"Source","target":"Target",
        "as_of":{"source_time":"time","target_time":"time","partition_by":{"group":"group"}}});
    for (field, value, expected) in [
        ("order_by", json!([{"expression":"target.score"}]), "as_of owns ordering"),
        ("limit_per_source", json!(1), "as_of owns ordering"),
        ("candidates", json!({"limit_per_source":1,"order_by":[{"expression":"target.score"}]}), "as_of owns ordering"),
        ("as_of", json!({"source_time":"missing","target_time":"time"}), "mapped endpoint properties"),
        ("as_of", json!({"source_time":"time","target_time":"missing"}), "mapped endpoint properties"),
        ("as_of", json!({"source_time":"time","target_time":"time","partition_by":{"group":"missing"}}), "mapped endpoint properties"),
    ] {
        let mut mapping = computed_boundary_mapping(); let before = mapping.to_toml();
        let mut definition = base.clone(); definition[field] = value;
        let error = mapping.map_computed_relationship(serde_json::from_value(definition).unwrap()).err().unwrap();
        assert!(error.to_string().contains(expected), "{field}: {error}");
        assert_eq!(mapping.to_toml(), before);
    }
    for kind in [DataType::Int64, DataType::Date64] {
        let mut mapping = computed_boundary_mapping();
        let fields = mapping.tables["targets"].schema().fields().iter().map(|field| {
            if field.name() == "time" { Arc::new(Field::new("time", kind.clone(), true)) } else { field.clone() }
        }).collect::<Vec<_>>();
        mapping.register_table_schema("targets", Arc::new(Schema::new(fields)));
        mapping.map_computed_relationship(serde_json::from_value(base.clone()).unwrap()).unwrap();
        assert!(mapping.validate_computed_relationships().unwrap_err().to_string().contains("matching declared types"));
    }
    let mut mapping = computed_boundary_mapping();
    mapping.map_computed_relationship(serde_json::from_value(base).unwrap()).unwrap();
    mapping.validate_computed_relationships().unwrap();
}

#[test]
fn computed_relationship_properties_resolve_dependencies_and_reject_cycles_and_collisions() {
    use serde_json::json;
    let mut mapping = computed_boundary_mapping();
    let mut rule = computed_boundary_rule();
    rule.properties = BTreeMap::from([("a".into(), "z + 1".into()), ("z".into(), "source.score + target.score".into())]);
    mapping.map_computed_relationship(rule).unwrap();
    mapping.validate_computed_relationships().unwrap();
    for properties in [json!({"a":"z + 1","z":"a + 1"}), json!({"a":"target.missing"})] {
        let mut mapping = computed_boundary_mapping();
        let mut rule = computed_boundary_rule(); rule.properties = serde_json::from_value(properties).unwrap();
        mapping.map_computed_relationship(rule).unwrap();
        let error = mapping.validate_computed_relationships().unwrap_err();
        assert!(error.to_string().contains("unresolved or cyclic relationship properties"), "{error}");
    }
    let mut mapping = computed_boundary_mapping(); let before = mapping.to_toml();
    let definition = json!({"name":"LINK","source":"Source","target":"Target","properties":{"score":"source.score"},
        "candidates":{"properties":{"score":"target.score"},"order_by":[{"expression":"target.score"}],"limit_per_source":1}});
    let error = mapping.map_computed_relationship(serde_json::from_value(definition).unwrap()).err().unwrap();
    assert!(error.to_string().contains("candidate and final property names must be distinct"), "{error}");
    assert_eq!(mapping.to_toml(), before);
}

#[test]
fn computed_relationship_functions_require_immutable_unique_definitions() {
    use datafusion::logical_expr::{create_udf, Volatility};
    use crate::ir::functions::logical::LogicalFunction;
    let function = |name: &str, volatility| LogicalFunction::new(name,
        Arc::new(create_udf("native_identity", vec![DataType::Int64], DataType::Int64,
            volatility, Arc::new(|args| Ok(args[0].clone())))), BTreeMap::new());
    let mut mapping = computed_boundary_mapping();
    for volatility in [Volatility::Stable, Volatility::Volatile] {
        let error = mapping.register_logical_function(function("custom", volatility)).err().unwrap();
        assert!(error.to_string().contains("must be immutable"), "{error}");
        assert!(mapping.logical_functions.is_empty());
    }
    mapping.register_logical_function(function("CuStOm", Volatility::Immutable)).unwrap();
    for name in ["custom", "CUSTOM", "text.bm25"] {
        let error = mapping.register_logical_function(function(name, Volatility::Immutable)).err().unwrap();
        assert!(error.to_string().contains("already exists"), "{error}");
        assert_eq!(mapping.logical_functions.len(), 1);
    }
    let mut rule = computed_boundary_rule();
    rule.properties.insert("score".into(), "custom(source.score)".into());
    mapping.map_computed_relationship(rule).unwrap();
    mapping.validate_computed_relationships().unwrap();
}

#[test]
fn computed_relationship_expressions_reject_aggregate_window_subquery_and_invalid_bm25_operands() {
    for (expression, expected) in [
        ("sum(source.score)", "must be scalar"),
        ("row_number() OVER ()", "must be scalar"),
        ("(SELECT 1)", "must be scalar"),
        ("text.bm25('query', 'document')", "BM25 second argument must be target"),
        ("text.bm25('query', source.score)", "BM25 second argument must be target"),
        ("text.bm25('query', target.score + 1)", "BM25 second argument must be target"),
    ] {
        let mut mapping = computed_boundary_mapping();
        let mut rule = computed_boundary_rule(); rule.properties.insert("score".into(), expression.into());
        mapping.map_computed_relationship(rule).unwrap();
        let before = mapping.to_toml();
        let error = mapping.validate_computed_relationships().unwrap_err();
        assert!(error.to_string().contains(expected), "{expression}: {error}");
        assert_eq!(mapping.to_toml(), before);
    }
}

#[test]
fn computed_relationship_node_sources_fail_fallible_serialization_without_panicking() {
    let cypher = serde_json::from_value(serde_json::json!({
        "name":"LINK", "source":"Source", "target":"Target", "parameters":[],
        "cypher":"WITH source MATCH (target:Target) RETURN target", "returns":{"target":"target","properties":{}}
    })).unwrap();
    for source in [MappedSource::Computed(Box::new(computed_boundary_rule())), MappedSource::Cypher(Box::new(cypher))] {
    let mut mapping = computed_boundary_mapping();
    mapping.map_node(NodeMapping::new("Invalid", source, "id"));
    let error = mapping.try_to_toml().unwrap_err();
    assert!(error.to_string().contains("node mapping `Invalid` cannot use a relationship source"), "{error}");
    assert!(mapping.edge("LINK").is_none());
    assert_eq!(mapping.node("Invalid").unwrap().id_column, KeyColumns::from("id"));
    }
}
