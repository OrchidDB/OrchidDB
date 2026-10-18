#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::common::ScalarValue;
use orchiddb::ir::{
    Value,
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    expr::BinaryOp,
    functions::domain,
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{GraphMapping, NodeMapping},
        sql::{self, DuckDbExecutor, SqlDialect},
    },
    temporal::{self},
};
use std::{collections::BTreeMap, sync::Arc};

fn fixture(kind: &str) -> (GraphMapping, DuckDbExecutor) {
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("SET memory_limit='256MB'; CREATE TABLE temporal_items(id BIGINT PRIMARY KEY, v VARCHAR, d VARCHAR)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "temporal_items",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "v",
                domain::data_type(&format!("cypher_temporal_{kind}"), DataType::Utf8),
                true,
            ),
            Field::new(
                "d",
                domain::data_type("cypher_temporal_duration", DataType::Utf8),
                true,
            ),
        ])),
    );
    mapping.map_node(
        NodeMapping::table("T", "temporal_items", "id")
            .property("id", "id")
            .property("v", "v")
            .property("d", "d"),
    );
    (mapping, DuckDbExecutor::from_connection(conn))
}
async fn compile(mapping: &GraphMapping, query: &str) -> sql::PreparedSql {
    let plan =
        orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping.clone())),
        language_functions: true,
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap_or_else(|e| panic!("{query}: {e}"));
    sql::prepare_with_external(
        &lowered,
        SqlDialect::DuckDb,
        &mapping.physical_table_names(),
    )
    .await
    .unwrap_or_else(|e| panic!("{query}: {e}"))
}
fn text(value: ScalarValue) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let value = domain::storage(&value).unwrap_or(value);
    Some(match value {
        ScalarValue::Utf8(Some(s)) => s,
        ScalarValue::Decimal128(Some(n), _, 0) => n.to_string(),
        _ => value.to_string(),
    })
}
#[tokio::test]
async fn mapped_temporal_arithmetic_runs_on_rows_inserted_after_compilation_without_folding() {
    without_constant_folding(async{
        for(kind,values)in [
            ("date",vec!["2000-01-31","0001-01-01","-1000000-02-29"]),
            ("localtime",vec!["12:31:14.000000001","23:59:59.999999999","00:00"]),
            ("time",vec!["12:31:14.000000001+01:00","23:59:59.999999999-01:00","00:00Z"]),
            ("localdatetime",vec!["2000-01-31T23:59:59.999999999","-1000000-02-29T12:31:14.000000001","0001-01-01T00:00"]),
            ("datetime_fixed",vec!["1984-10-11T12:31:14.000000001+01:00","2000-01-31T23:59:59.999999999-01:00","0001-01-01T00:00Z"]),
            ("duration",vec!["P1M","P-1DT-0.000000001S","P12Y5M14DT16H13M10.000000002S"]),
        ]{
            let(mapping,mut executor)=fixture(kind);
            let query="MATCH (n:T) RETURN n.v + n.d AS sum, n.v - n.d AS diff ORDER BY n.id";
            let prepared=compile(&mapping,query).await;
            assert!(!prepared.query.contains("ROW_NUMBER"));
            let delta=temporal::parse("duration","P1M2DT6M0.000000002S").unwrap();
            let values=values.iter().map(|v|temporal::parse(if kind=="datetime_fixed"{"datetime"}else{kind},v).unwrap()).collect::<Vec<_>>();
            let conn=executor.connection().unwrap();
            for(i,v)in values.iter().enumerate(){conn.execute("INSERT INTO temporal_items VALUES (?, ?, ?)",duckdb::params![i as i64,v.encode(),delta.encode()]).unwrap();}
            conn.execute("INSERT INTO temporal_items VALUES (100,NULL,NULL)",[]).unwrap();
            let result=sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{kind}: {e}\n{}",prepared.query));
            for(i,v)in values.iter().enumerate(){for(column,op)in [BinaryOp::Add,BinaryOp::Sub].into_iter().enumerate(){
                let Value::Temporal(expected)=temporal::arithmetic(op,&Value::Temporal(v.clone()),&Value::Temporal(delta.clone())).unwrap()else{panic!()};
                assert_eq!(text(ScalarValue::try_from_array(result.batch.column(column),i).unwrap()),Some(expected.encode()),"{kind} {op:?}");
            }}
            assert!(ScalarValue::try_from_array(result.batch.column(0),values.len()).unwrap().is_null());
            if kind=="duration"{
                for factor in ["1","2","0.5","-0.5"]{
                    let q=format!("MATCH (n:T) RETURN n.v * {factor} AS mul, n.v / {factor} AS div ORDER BY n.id");
                    let prepared=compile(&mapping,&q).await;
                    let result=sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{e}\n{}",prepared.query));
                    for(i,v)in values.iter().enumerate(){for(column,op)in [BinaryOp::Mul,BinaryOp::Div].into_iter().enumerate(){
                        let Value::Temporal(expected)=temporal::arithmetic(op,&Value::Temporal(v.clone()),&Value::Float(factor.parse().unwrap())).unwrap()else{panic!()};
                        assert_eq!(text(ScalarValue::try_from_array(result.batch.column(column),i).unwrap()),Some(expected.encode()),"{op:?} {factor}");
                    }}
                }
            }
        }
    }).await;
}
#[tokio::test]
async fn mapped_temporal_order_uses_nanoseconds_offsets_and_named_zone_ties() {
    without_constant_folding(async {
        for (kind, values) in [
            (
                "date",
                vec!["2000-01-01", "-1000000-12-31", "+1000000-01-01"],
            ),
            (
                "localtime",
                vec!["12:00:00.000000002", "12:00:00.000000001", "00:00"],
            ),
            (
                "time",
                vec![
                    "00:30-01:00",
                    "00:30+01:00",
                    "00:30+01:00",
                    "23:59:59.999999999+00:01:02",
                ],
            ),
            (
                "localdatetime",
                vec![
                    "2000-01-01T00:00:00.000000002",
                    "2000-01-01T00:00:00.000000001",
                    "-1000000-01-01T00:00",
                ],
            ),
            (
                "datetime",
                vec![
                    "2000-01-01T00:00+01:00",
                    "1999-12-31T23:00Z",
                    "2000-01-01T00:00+01:00[Europe/Stockholm]",
                    "2000-01-01T00:00:00.000000001+01:00",
                ],
            ),
        ] {
            let (mapping, mut executor) = fixture(kind);
            let prepared = compile(&mapping, "MATCH (n:T) RETURN n.v AS v ORDER BY v").await;
            let mut values = values
                .iter()
                .map(|v| temporal::parse(kind, v).unwrap())
                .collect::<Vec<_>>();
            let conn = executor.connection().unwrap();
            for (i, v) in values.iter().enumerate() {
                conn.execute(
                    "INSERT INTO temporal_items VALUES (?, ?, NULL)",
                    duckdb::params![i as i64, v.encode()],
                )
                .unwrap();
            }
            conn.execute("INSERT INTO temporal_items VALUES (100,NULL,NULL)", [])
                .unwrap();
            values.sort_by(|a, b| a.compare(b).unwrap());
            let result = sql::execute_prepared(&mut executor, &prepared)
                .unwrap_or_else(|e| panic!("{kind}: {e}\n{}", prepared.query));
            let mut expected = values
                .into_iter()
                .map(|v| Some(v.encode()))
                .collect::<Vec<_>>();
            expected.push(None);
            let actual = (0..result.batch.num_rows())
                .map(|i| text(ScalarValue::try_from_array(result.batch.column(0), i).unwrap()))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{kind}\n{}", prepared.query);
            if matches!(kind, "time" | "localtime" | "localdatetime" | "date") {
                let query =
                    "MATCH (n:T) RETURN n.v AS v ORDER BY n.v + duration({minutes:6}) LIMIT 3";
                let computed = compile(&mapping, query).await;
                assert!(
                    computed.query.len() < 20000,
                    "temporal operand repeated: {} bytes",
                    computed.query.len()
                );
                let result = sql::execute_prepared(&mut executor, &computed)
                    .unwrap_or_else(|e| panic!("{kind}: {e}\n{}", computed.query));
                assert_eq!(result.batch.num_rows(), 3);
            }
        }
    })
    .await;
}
#[tokio::test]
async fn literal_component_constructors_execute_sql_and_wide_integer_components_round_trip() {
    without_constant_folding(async{
        let(mapping,mut executor)=fixture("duration");
        executor.connection().unwrap().execute("INSERT INTO temporal_items VALUES (1,'duration|1|2|9223372036854775807|999999999','duration|1|2|3|4')",[]).unwrap();
        for constructor in ["date({year:1984,month:10,day:11})","localtime({hour:12,minute:31,second:14,nanosecond:1})","time({hour:12,minute:31,second:14,nanosecond:1,timezone:'+01:00'})","localdatetime({year:1984,month:10,day:11,hour:12,minute:31,second:14,nanosecond:1})","datetime({year:1984,month:10,day:11,hour:12,minute:31,second:14,nanosecond:1,timezone:'+01:00'})"]{
            let query=format!("WITH {constructor} AS x MATCH (n:T) RETURN x + n.d AS sum, x - n.d AS diff");
            let prepared=compile(&mapping,&query).await;
            assert!(prepared.query.contains("printf")||prepared.query.contains("make_date"),"{}",prepared.query);
            let result=sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{e}\n{}",prepared.query));
            if constructor.starts_with("date("){assert_eq!(domain::descriptor(result.batch.column(0).data_type()),Some(("cypher_temporal",&DataType::Date32)));}
        }
        let prepared=compile(&mapping,"MATCH (n:T) WITH n.v AS d RETURN d.nanoseconds AS ns").await;
        let result=sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{e}\n{}",prepared.query));
        assert_eq!(text(ScalarValue::try_from_array(result.batch.column(0),0).unwrap()),Some("9223372036854775807999999999".into()));
        let prepared=compile(&mapping,"RETURN duration({years:1000000000000000000,months:-9000000000000000000,weeks:2000000000000000000,days:-9000000000000000000}) AS d").await;
        let result=sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{e}\n{}",prepared.query));
        assert_eq!(text(ScalarValue::try_from_array(result.batch.column(0),0).unwrap()),Some("duration|3000000000000000000|5000000000000000000|0|0".into()));
        let prepared=compile(&mapping,"MATCH (n:T) RETURN n.d + duration({months:1,days:2,minutes:6}) AS d").await;
        sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{e}\n{}",prepared.query));
    }).await;
}
