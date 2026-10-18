#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    functions::scalar_union,
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{GraphMapping, NodeMapping},
        sql::{self, DuckDbExecutor, SqlDialect},
    },
};
use std::{collections::BTreeMap, sync::Arc};

#[tokio::test]
async fn whole_node_display_renders_logical_property_not_tagged_storage() {
    without_constant_folding(async {
        let connection=duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE items(id BIGINT, v STRUCT(__cypher_scalar_tag TINYINT, integer BIGINT, string VARCHAR, boolean BOOLEAN)); INSERT INTO items VALUES (1,{__cypher_scalar_tag:1,integer:9223372036854775807,string:NULL,boolean:NULL}),(2,{__cypher_scalar_tag:2,integer:NULL,string:'abc',boolean:NULL}),(3,{__cypher_scalar_tag:3,integer:NULL,string:NULL,boolean:true}),(4,NULL)").unwrap();
        let mut mapping=GraphMapping::new();
        mapping.register_table_schema("items",Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,false),Field::new("v",scalar_union::data_type(),true)])));
        mapping.map_node(NodeMapping::table("Item","items","id").property("v","v"));
        let plan=orchiddb::language::cypher::preparation::prepare("MATCH(n) RETURN n ORDER BY id(n)",&BTreeMap::new(),None).unwrap();
        let lowered=RelBackend::with_options(RelBackendOptions{mapping:Some(Arc::new(mapping.clone())),language_functions:true,..Default::default()}).lower(&plan,&PropertyGraph::new()).unwrap();
        let prepared=sql::prepare_with_external(&lowered,SqlDialect::DuckDb,&mapping.physical_table_names()).await.unwrap();
        let result=sql::execute_prepared(&mut DuckDbExecutor::from_connection(connection),&prepared).unwrap();
        let actual=(0..result.batch.num_rows()).map(|row|arrow::util::display::array_value_to_string(result.batch.column(0),row).unwrap()).collect::<Vec<_>>();
        assert_eq!(actual,vec!["{_ID: 0:1, _LABEL: Item, v: 9223372036854775807}","{_ID: 0:2, _LABEL: Item, v: abc}","{_ID: 0:3, _LABEL: Item, v: True}","{_ID: 0:4, _LABEL: Item}"]);
    }).await;
}
