use super::*;
use datafusion::sql::sqlparser::{ast,parser::Parser,dialect::PostgreSqlDialect};
fn expression(text:&str)->ast::Expr {Parser::new(&PostgreSqlDialect{}).try_with_sql(text).unwrap().parse_expr().unwrap()}

#[test]
fn list_decoders_reject_nonliteral_and_nonarray_type_metadata() {
    for (name,prefix) in [("__orchiddb_pg_list_element",vec![expression("ARRAY['[1,2]'::jsonb]"),expression("1")]),
        ("__orchiddb_pg_list_decode",vec![expression("'[1,2]'::jsonb")])] {
        for metadata in ["type_column","42","NULL","'BIGINT'"] {
            let mut arguments=prefix.clone();arguments.push(expression(metadata));
            let mut target=expression("original");
            assert!(adapt(&mut target,name,&arguments).unwrap_err().to_string().contains("list element type"),"{name}: {metadata}");
            assert_eq!(target,expression("original"));
        }
    }
}

#[test]
fn postgres_list_placeholders_cannot_be_executed_as_native_functions() {
    use datafusion::logical_expr::ScalarFunctionArgs;
    let value=placeholder("__orchiddb_pg_list_decode",vec![],DataType::Int64);
    let Expr::ScalarFunction(call)=value else {panic!()};
    let error=call.func.invoke_with_args(ScalarFunctionArgs{args:vec![],arg_fields:vec![],number_rows:1,
        return_field:Arc::new(Field::new("result",DataType::Int64,true)),config_options:Default::default()}).unwrap_err();
    assert!(error.to_string().contains("PostgreSQL list SQL placeholder"));
}

#[test]
#[ignore="requires PostgreSQL"]
fn postgres_child_list_extraction_preserves_ragged_null_and_negative_index_values() {
    let pg=std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    for (source,index,ty,expected) in [
        ("ARRAY['[1,2]'::jsonb,'[]'::jsonb,NULL,'[3,null]'::jsonb]",1,"BIGINT[]","[1, 2]"),
        ("ARRAY['[1,2]'::jsonb,'[]'::jsonb,NULL,'[3,null]'::jsonb]",2,"BIGINT[]","[]"),
        ("ARRAY['[1,2]'::jsonb,'[]'::jsonb,NULL,'[3,null]'::jsonb]",3,"BIGINT[]","null"),
        ("ARRAY['[1,2]'::jsonb,'[]'::jsonb,NULL,'[3,null]'::jsonb]",-1,"BIGINT[]","[3, null]"),
        ("ARRAY['[[1],null,[]]'::jsonb]",1,"JSONB[]","[[1], null, []]"),
        ("ARRAY['[1]'::jsonb]",9,"BIGINT[]","null"),
    ] {
        let arguments=vec![expression(source),expression(&index.to_string()),expression(&format!("'{ty}'"))];
        let mut value=expression("original");
        assert!(adapt(&mut value,"__orchiddb_pg_list_element",&arguments).unwrap());
        let sql=format!("SELECT coalesce(to_jsonb({value})::text,'null')");
        let result=std::process::Command::new("psql").args([&pg,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",&sql]).output().unwrap();
        assert!(result.status.success(),"{sql}: {}",String::from_utf8_lossy(&result.stderr));
        assert_eq!(String::from_utf8(result.stdout).unwrap().trim(),expected,"{sql}");
    }
}
