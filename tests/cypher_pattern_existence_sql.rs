#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    rel::{
        sql::{execute_lowered_sql, unparse, DuckDbExecutor, SqlDialect},
        RelBackend,
    },
    value::Value,
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

fn fixture() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let nodes = (0..5)
        .map(|num| graph.insert_node("X", BTreeMap::from([("num".into(), Value::Int(num))])))
        .collect::<Vec<_>>();
    let other = graph.insert_node("Y", BTreeMap::new());
    for (from, to) in [
        (&nodes[0], &other),
        (&other, &nodes[2]),
        (&nodes[3], &nodes[3]),
        (&nodes[4], &other),
        (&nodes[4], &other),
    ] {
        graph.insert_edge("T", from, to, BTreeMap::new()).unwrap();
    }
    graph
}
async fn run(graph: &PropertyGraph, query: &str, nofold: bool) -> (String, Vec<Vec<String>>) {
    let work = async {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, graph)
            .unwrap_or_else(|error| panic!("{query}: {error}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|error| panic!("{query}: {error}\n{sql}"));
        let rows = (0..result.batch.num_rows())
            .map(|row| {
                result
                    .batch
                    .columns()
                    .iter()
                    .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                    .collect()
            })
            .collect();
        (sql, rows)
    };
    if nofold {
        without_constant_folding(work).await
    } else {
        work.await
    }
}
fn assert_existence(sql: &str) {
    let lower = sql.to_lowercase();
    assert!(lower.contains("exists"), "{sql}");
    for unwanted in [
        "row_number",
        "array_agg",
        "group by",
        "scalar subquery returned more than one row",
    ] {
        assert!(!lower.contains(unwanted), "{unwanted}: {sql}");
    }
}
#[tokio::test]
async fn pattern_size_existence_preserves_returned_nodes_and_avoids_collection() {
    let graph = fixture();
    for nofold in [true, false] {
        let (_, nodes) = run(&graph, "MATCH (n:X) RETURN n ORDER BY n.num", nofold).await;
        let (sql, rows) = run(
            &graph,
            "MATCH (n:X) RETURN n, size([(n)--() | 1]) > 0 AS b ORDER BY n.num",
            nofold,
        )
        .await;
        assert_existence(&sql);
        assert_eq!(rows.len(), 5);
        for (index, (row, node)) in rows.iter().zip(nodes).enumerate() {
            assert_eq!(&row[..row.len() - 1], node.as_slice());
            assert_eq!(
                row.last().unwrap(),
                if index == 1 { "false" } else { "true" }
            );
        }
    }
}
#[tokio::test]
async fn existence_keeps_direction_label_identity_self_loops_and_duplicate_rows() {
    let graph = fixture();
    for (pattern, expected) in [
        ("(n)--()", vec![true, false, true, true, true]),
        ("(n)-->()", vec![true, false, false, true, true]),
        ("(n)<--()", vec![false, false, true, true, false]),
    ] {
        let query=format!("MATCH (n:X) UNWIND [1,1] AS duplicate RETURN n.num, size([{pattern} | 1]) > 0 AS b ORDER BY n.num");
        let (sql, rows) = run(&graph, &query, true).await;
        assert_existence(&sql);
        let expected = expected
            .into_iter()
            .enumerate()
            .flat_map(|(i, b)| {
                [
                    vec![i.to_string(), b.to_string()],
                    vec![i.to_string(), b.to_string()],
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(rows, expected, "{query}\n{sql}");
    }
}
#[tokio::test]
async fn null_projection_and_missing_anchor_keep_comprehension_cardinality() {
    let graph = fixture();
    for query in [
        "MATCH (n:X) RETURN n.num, size([(n)--() | null]) > 0 AS b ORDER BY n.num",
        "OPTIONAL MATCH (n:Missing) RETURN size([(n)--() | 1]) > 0 AS b",
    ] {
        let reference = query
            .replace("| null]", "| CASE WHEN true THEN null ELSE null END]")
            .replace("| 1]", "| 1+0]");
        let (_, expected) = run(&graph, &reference, true).await;
        let (_, actual) = run(&graph, query, true).await;
        assert_eq!(actual, expected, "{query}");
    }
}
#[test]
fn potentially_throwing_or_effectful_projection_is_not_discarded() {
    for map in ["1/0", "rand()", "toInteger('bad')"] {
        let query = format!("MATCH (n:X) RETURN size([(n)--() | {map}]) > 0 AS b");
        let plan = CypherPlanner::new()
            .plan(&parse_query(&query).unwrap())
            .unwrap();
        assert!(
            format!("{plan:?}").contains("GraphCollect"),
            "{query}: {plan:?}"
        );
    }
}

#[tokio::test]
async fn mapped_existence_uses_live_tables_and_checks_target_presence() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::ir::rel::{
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql, RelBackendOptions,
    };
    use std::sync::Arc;
    without_constant_folding(async {
        let connection=duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE xs(id BIGINT PRIMARY KEY,num BIGINT); CREATE TABLE ys(id BIGINT PRIMARY KEY); CREATE TABLE edges(id BIGINT PRIMARY KEY,src BIGINT,dst BIGINT)").unwrap();
        let mut mapping=GraphMapping::new();
        for (table,columns) in [("xs",vec!["id","num"]),("ys",vec!["id"]),("edges",vec!["id","src","dst"])] {
            mapping.register_table_schema(table,Arc::new(Schema::new(columns.into_iter().map(|name|Field::new(name,DataType::Int64,name!="id")).collect::<Vec<_>>())));
        }
        mapping.map_node(NodeMapping::table("X","xs","id").property("num","num"));
        mapping.map_node(NodeMapping::table("Y","ys","id"));
        mapping.map_edge(EdgeMapping::table("T","edges","src","dst","X","Y").with_id("id"));
        let plan=CypherPlanner::new().plan(&parse_query("MATCH (n:X) RETURN n.num,size([(n)--() | 1]) > 0 AS b ORDER BY n.num").unwrap()).unwrap();
        let lowered=RelBackend::with_options(RelBackendOptions { mapping:Some(Arc::new(mapping.clone())),language_functions:true,..Default::default() }).lower(&plan,&PropertyGraph::new()).unwrap();
        let prepared=sql::prepare_with_external(&lowered,SqlDialect::DuckDb,&mapping.physical_table_names()).await.unwrap();
        assert_existence(&prepared.query);
        connection.execute_batch("INSERT INTO xs VALUES(1,42),(2,43),(3,44); INSERT INTO ys VALUES(1); INSERT INTO edges VALUES(1,1,1),(2,1,1),(3,3,99)").unwrap();
        let result=sql::execute_prepared(&mut DuckDbExecutor::from_connection(connection),&prepared).unwrap();
        let rows=(0..result.batch.num_rows()).map(|row|result.batch.columns().iter().map(|column|arrow::util::display::array_value_to_string(column,row).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
        assert_eq!(rows,vec!["42|true","43|false","44|false"]);
    }).await;
}

#[test]
fn existence_rewrite_does_not_hide_invalid_projection_literals() {
    let query = "MATCH (n:X) RETURN size([(n)--() | 99999999999999999999999999999999]) > 0 AS b";
    let parsed = parse_query(query);
    assert!(parsed.is_err() || CypherPlanner::new().plan(&parsed.unwrap()).is_err());
}

#[tokio::test]
async fn engine_optimization_cannot_capture_the_outer_node_identifier() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:X {num:0}),(b:X {num:1}),(c:X {num:2}),(d:X {num:3}),(e:X {num:4}),(y:Y), (a)-[:T]->(y),(y)-[:T]->(c),(d)-[:T]->(d),(e)-[:T]->(y),(e)-[:T]->(y)").await.unwrap();
    for nofold in [true, false] {
        let work = async {
            let result = engine
                .cypher("MATCH (n:X) RETURN n.num, size([(n)--() | 1]) > 0 AS b, size([(n)--() | 1]) AS c ORDER BY n.num")
                .await
                .unwrap();
            let batch = &result.returned.batch;
            let rows = (0..batch.num_rows())
                .map(|row| {
                    batch
                        .columns()
                        .iter()
                        .map(|column| {
                            arrow::util::display::array_value_to_string(column, row).unwrap()
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect::<Vec<_>>();
            assert_eq!(
                rows,
                vec!["0|true|1", "1|false|0", "2|true|1", "3|true|1", "4|true|2"],
                "{:?}",
                result.stats.sql_queries
            );
        };
        if nofold {
            without_constant_folding(work).await
        } else {
            work.await
        }
    }
}

#[tokio::test]
async fn mapped_physical_columns_cannot_capture_outer_identity() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::ir::rel::{
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql, RelBackendOptions,
    };
    use std::sync::Arc;
    for nofold in [true, false] {
        let work = async {
            let connection = duckdb::Connection::open_in_memory().unwrap();
            connection.execute_batch("CREATE TABLE collision_xs(n__id BIGINT PRIMARY KEY, num BIGINT); CREATE TABLE collision_ys(n__id BIGINT PRIMARY KEY); CREATE TABLE collision_edges(id BIGINT PRIMARY KEY, n__id BIGINT, dst BIGINT)").unwrap();
            let mut mapping = GraphMapping::new();
            for (table, columns) in [
                ("collision_xs", vec!["n__id", "num"]),
                ("collision_ys", vec!["n__id"]),
                ("collision_edges", vec!["id", "n__id", "dst"]),
            ] {
                mapping.register_table_schema(table, Arc::new(Schema::new(columns.into_iter()
                    .map(|name| Field::new(name, DataType::Int64, false)).collect::<Vec<_>>())));
            }
            mapping.map_node(NodeMapping::table("X", "collision_xs", "n__id").property("num", "num"));
            mapping.map_node(NodeMapping::table("Y", "collision_ys", "n__id"));
            mapping.map_edge(EdgeMapping::table("T", "collision_edges", "n__id", "dst", "X", "Y").with_id("id"));
            connection.execute_batch("INSERT INTO collision_xs VALUES(1,42),(2,43),(3,44); INSERT INTO collision_ys VALUES(10); INSERT INTO collision_edges VALUES(1,1,10),(2,1,10),(3,3,99)").unwrap();
            let mut executor = DuckDbExecutor::from_connection(connection);
            for pattern in ["(n)--()", "(n)-->()"] {
                let query = format!("MATCH (n:X) RETURN n.num,size([{pattern} | 1]) > 0 AS b,size([{pattern} | null]) AS c ORDER BY n.num");
                let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
                let lowered = RelBackend::with_options(RelBackendOptions {
                    mapping: Some(Arc::new(mapping.clone())), language_functions: true, ..Default::default()
                }).lower(&plan, &PropertyGraph::new()).unwrap();
                let prepared = sql::prepare_with_external(&lowered, SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap();
                assert_existence(&prepared.query);
                let result = sql::execute_prepared(&mut executor, &prepared).unwrap_or_else(|error| panic!("nofold={nofold}: {error}\n{}", prepared.query));
                let rows = (0..result.batch.num_rows()).map(|row| result.batch.columns().iter()
                    .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                    .collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
                assert_scalar_count(&prepared.query);
                assert_eq!(rows, vec!["42|true|2", "43|false|0", "44|false|0"], "{query}\n{}", prepared.query);
            }
        };
        if nofold { without_constant_folding(work).await } else { work.await }
    }
}

fn assert_scalar_count(sql: &str) {
    let sql=sql.to_lowercase();
    assert!(sql.contains("count("),"{sql}");
    for unwanted in ["row_number", "array_agg", "group by", "scalar subquery returned more than one row"] {
        assert!(!sql.contains(unwanted),"{sql}");
    }
}

#[tokio::test]
async fn pattern_cardinality_counts_matches_without_building_lists() {
    let graph=fixture();
    for nofold in [true,false] {
        for (pattern,expected) in [("(n)--()",vec![1,0,1,1,2]),("(n)-->()",vec![1,0,0,1,2]),("(n)<--()",vec![0,0,1,1,0])] {
            for map in ["1","null"] {
                let query=format!("MATCH (n:X) UNWIND [1,1] AS occurrence RETURN n.num,size([{pattern} | {map}]) AS count ORDER BY n.num");
                let (sql,rows)=run(&graph,&query,nofold).await;
                assert_scalar_count(&sql);
                let expected=expected.iter().enumerate().flat_map(|(n,c)|vec![vec![n.to_string(),c.to_string()];2]).collect::<Vec<_>>();
                assert_eq!(rows,expected,"{query}\n{sql}");
            }
        }
        let (sql,rows)=run(&graph,"OPTIONAL MATCH (n:Missing) RETURN size([(n)--() | null]) AS count",nofold).await;
        assert_eq!(rows,vec![vec!["0"]],"{sql}");
    }
}

#[tokio::test]
async fn pattern_cardinality_preserves_complex_pattern_multiplicity() {
    let graph=fixture();
    for pattern in ["(n)--()--()","(n)-[*0..2]->()","(n)--(n)"] {
        let prefix=format!("MATCH (n:X) RETURN n.num,size([{pattern} | ");
        let (sql,rows)=run(&graph,&format!("{prefix}1]) AS count ORDER BY n.num"),true).await;
        let (_,reference)=run(&graph,&format!("{prefix}1+0]) AS count ORDER BY n.num"),true).await;
        assert_eq!(rows,reference,"{pattern}\n{sql}");
        assert!(!sql.to_lowercase().contains("array_agg"),"{sql}");
    }
}

#[test]
fn pattern_count_keeps_projection_errors_and_effects() {
    for map in ["1/0","rand()","toInteger('bad')"] {
        let query=format!("MATCH (n:X) RETURN size([(n)--() | {map}]) AS count");
        let plan=CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        assert!(format!("{plan:?}").contains("GraphCollect"),"{query}");
    }
    let query="MATCH (n:X) RETURN size([(n)--() | 99999999999999999999999999999999]) AS count";
    let parsed=parse_query(query);
    assert!(parsed.is_err() || CypherPlanner::new().plan(&parsed.unwrap()).is_err());
}
