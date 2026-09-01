//! "Bring your own schema" (BYOS) smoke tests.
//!
//! A user-owned relational schema — `customers`, `orders`, `follows`, plus a
//! `high_value_customers` VIEW — is mapped onto graph labels and edge types
//! with a `GraphMapping`. Real Cypher and Gremlin queries then run against
//! that schema through both engines:
//!
//! * in-process DataFusion, with the user's tables registered as `MemTable`s
//!   (the view as a DataFusion `ViewTable`), and
//! * DuckDB, where the same tables/views exist in the database and the
//!   generated SQL references them directly (query-backed labels are inlined
//!   as derived tables; nothing user-owned is re-materialized).

use std::collections::BTreeSet;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;
use datafusion::prelude::SessionContext;

use new_graph::ir::bridge::gremlin as gb;
use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::expr::Lit;
use new_graph::ir::plan::GraphPlan;
use new_graph::ir::rel::mapping::{EdgeMapping, GraphMapping, MappedSource, NodeMapping};
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};
use new_graph::ir::rel::{LoweredPlan, RelBackend, RelBackendOptions, execute_lowered};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner as AstCypherPlanner;
use new_graph::planner::GremlinPlanner;

const HIGH_VALUE_VIEW_SQL: &str = "SELECT c.cust_id, c.full_name \
     FROM customers c JOIN orders o ON o.cust_id = c.cust_id \
     WHERE o.total > 400.0";

const VIP_QUERY_SQL: &str = "SELECT cust_id, full_name, age FROM customers WHERE age >= 30";

/// The user's own schema as in-memory Arrow tables (the in-process side).
fn user_tables() -> Vec<(&'static str, RecordBatch)> {
    let customers = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("cust_id", DataType::Int64, false),
            Field::new("full_name", DataType::Utf8, true),
            Field::new("age", DataType::Int64, true),
            Field::new("city", DataType::Utf8, true),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
            Arc::new(StringArray::from(vec!["alice", "bob", "carol"])),
            Arc::new(Int64Array::from(vec![30, 28, 41])),
            Arc::new(StringArray::from(vec!["berlin", "lisbon", "berlin"])),
        ],
    )
    .unwrap();
    let orders = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("order_id", DataType::Int64, false),
            Field::new("cust_id", DataType::Int64, false),
            Field::new("total", DataType::Float64, true),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![100, 101, 102, 103])) as ArrayRef,
            Arc::new(Int64Array::from(vec![1, 1, 2, 3])),
            Arc::new(Float64Array::from(vec![50.0, 120.0, 80.0, 500.0])),
        ],
    )
    .unwrap();
    let follows = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("src_id", DataType::Int64, false),
            Field::new("dst_id", DataType::Int64, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 1, 2])) as ArrayRef,
            Arc::new(Int64Array::from(vec![2, 3, 3])),
        ],
    )
    .unwrap();
    vec![
        ("customers", customers),
        ("orders", orders),
        ("follows", follows),
    ]
}

/// The same schema as DuckDB DDL (the external side). Includes a real VIEW.
fn user_schema_ddl() -> Vec<String> {
    vec![
        "CREATE TABLE customers (cust_id BIGINT, full_name VARCHAR, age BIGINT, city VARCHAR)"
            .to_string(),
        "INSERT INTO customers VALUES \
         (1, 'alice', 30, 'berlin'), (2, 'bob', 28, 'lisbon'), (3, 'carol', 41, 'berlin')"
            .to_string(),
        "CREATE TABLE orders (order_id BIGINT, cust_id BIGINT, total DOUBLE)".to_string(),
        "INSERT INTO orders VALUES \
         (100, 1, 50.0), (101, 1, 120.0), (102, 2, 80.0), (103, 3, 500.0)"
            .to_string(),
        "CREATE TABLE follows (src_id BIGINT, dst_id BIGINT)".to_string(),
        "INSERT INTO follows VALUES (1, 2), (1, 3), (2, 3)".to_string(),
        format!("CREATE VIEW high_value_customers AS {HIGH_VALUE_VIEW_SQL}"),
    ]
}

fn byos_mapping() -> Arc<GraphMapping> {
    let mut mapping = GraphMapping::new();
    for (name, batch) in user_tables() {
        let provider = MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap();
        mapping.register_table(name, Arc::new(provider));
    }
    // A SQL-defined view over the registered tables; in DuckDB the same name
    // exists as a real VIEW.
    mapping
        .register_view("high_value_customers", HIGH_VALUE_VIEW_SQL)
        .expect("register view");

    mapping.map_node(
        NodeMapping::table("Person", "customers", "cust_id")
            .property("name", "full_name")
            .property("age", "age")
            .property("city", "city"),
    );
    mapping.map_node(NodeMapping::table("Order", "orders", "order_id").property("total", "total"));
    // Query-backed label: parsed by datafusion-sql, inlined into generated SQL.
    mapping.map_node(
        NodeMapping::query("Vip", VIP_QUERY_SQL, "cust_id")
            .property("name", "full_name")
            .property("age", "age"),
    );
    // View-backed label: table mapping that points at the VIEW name.
    mapping.map_node(
        NodeMapping::table("HighValue", "high_value_customers", "cust_id")
            .property("name", "full_name"),
    );
    mapping.map_edge(
        EdgeMapping::table(
            "ORDERED", "orders", "cust_id", "order_id", "Person", "Order",
        )
        .with_id("order_id")
        .property("total", "total"),
    );
    mapping.map_edge(EdgeMapping::table(
        "FOLLOWS", "follows", "src_id", "dst_id", "Person", "Person",
    ));
    Arc::new(mapping)
}

fn byos_backend(mapping: Arc<GraphMapping>) -> RelBackend {
    RelBackend::with_options(RelBackendOptions {
        mapping: Some(mapping),
        ..RelBackendOptions::default()
    })
}

fn cypher_plan(query: &str) -> GraphPlan {
    let parsed = parse_query(query).expect("parse");
    AstCypherPlanner::new().plan(&parsed).expect("plan")
}

fn lower(plan: &GraphPlan) -> LoweredPlan {
    // The property-graph catalog is empty: every scan resolves via the mapping.
    byos_backend(byos_mapping())
        .lower(plan, &PropertyGraph::new())
        .expect("lower through mapping")
}

fn batch_lines(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| cell_to_string(batch.column(col), row))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn cell_to_string(array: &ArrayRef, row: usize) -> String {
    if array.is_null(row) {
        return "null".into();
    }
    match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Float64 => {
            let value = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row);
            format!("{value}")
        }
        DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap()
            .value(row)
            .to_string(),
        other => panic!("unhandled result type in byos smoke test: {other}"),
    }
}

/// Execute the plan through DuckDB (against the user's real tables/views) and
/// through in-process DataFusion (against the registered MemTables/ViewTable),
/// assert both engines agree, and return the rows.
async fn both_engines(plan: &GraphPlan, ordered: bool) -> Vec<String> {
    let lowered = lower(plan);

    // External path: only the user's DDL sets up the database. Everything the
    // mapping owns is external, so `prepared.setup` must stay empty.
    let external: BTreeSet<String> = byos_mapping().physical_table_names();
    let mut prepared = sql::prepare_with_external(&lowered, SqlDialect::DuckDb, &external)
        .await
        .expect("prepare sql");
    assert!(
        prepared.setup.is_empty(),
        "user-owned tables must not be re-materialized, got: {:?}",
        prepared.setup
    );
    prepared.setup = user_schema_ddl();
    let mut executor = DuckDbExecutor::new();
    let from_duckdb = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|err| panic!("duckdb execute: {err}\nquery: {}", prepared.query));

    // In-process path.
    let from_datafusion = execute_lowered(lower(plan))
        .await
        .expect("datafusion execute");

    let mut duckdb_lines = batch_lines(&from_duckdb.batch);
    let mut datafusion_lines = batch_lines(&from_datafusion.batch);
    if !ordered {
        duckdb_lines.sort();
        datafusion_lines.sort();
    }
    assert_eq!(
        duckdb_lines, datafusion_lines,
        "duckdb and datafusion disagree\nquery: {}",
        prepared.query
    );
    duckdb_lines
}

#[tokio::test]
async fn cypher_match_expand_filter_over_user_tables() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:ORDERED]->(o) WHERE o.total > 100.0 \
         RETURN p.name, o.total ORDER BY o.total",
    );
    let rows = both_engines(&plan, true).await;
    assert_eq!(rows, vec!["alice|120", "carol|500"]);
}

#[tokio::test]
async fn cypher_edge_properties_and_aggregates_over_user_tables() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[r:ORDERED]->(:Order) \
         RETURN p.name AS name, sum(r.total) AS spent ORDER BY name",
    );
    let rows = both_engines(&plan, true).await;
    assert_eq!(rows, vec!["alice|170", "bob|80", "carol|500"]);
}

#[tokio::test]
async fn cypher_follow_join_table_between_persons() {
    let plan = cypher_plan(
        "MATCH (a:Person)-[:FOLLOWS]->(b:Person) WHERE a.name = 'alice' \
         RETURN b.name ORDER BY b.name",
    );
    let rows = both_engines(&plan, true).await;
    assert_eq!(rows, vec!["bob", "carol"]);
}

#[tokio::test]
async fn cypher_query_backed_label_is_inlined() {
    let plan = cypher_plan("MATCH (v:Vip) WHERE v.age > 29 RETURN v.name ORDER BY v.name");
    let lowered = lower(&plan);
    let external = byos_mapping().physical_table_names();
    let prepared = sql::prepare_with_external(&lowered, SqlDialect::DuckDb, &external)
        .await
        .expect("prepare sql");
    // The Vip query is inlined into the generated SQL as a derived table over
    // the user's `customers` table — no synthetic tables involved.
    assert!(
        prepared.query.contains("customers"),
        "expected inlined view over customers, got: {}",
        prepared.query
    );
    let rows = both_engines(&plan, true).await;
    assert_eq!(rows, vec!["alice", "carol"]);
}

#[tokio::test]
async fn cypher_view_backed_label_reads_duckdb_view() {
    let plan = cypher_plan("MATCH (h:HighValue) RETURN h.name ORDER BY h.name");
    let lowered = lower(&plan);
    let external = byos_mapping().physical_table_names();
    let prepared = sql::prepare_with_external(&lowered, SqlDialect::DuckDb, &external)
        .await
        .expect("prepare sql");
    // Table-backed mapping onto a VIEW name: the SQL references the view
    // directly and DuckDB expands it.
    assert!(
        prepared.query.contains("high_value_customers"),
        "expected a scan of the view, got: {}",
        prepared.query
    );
    let rows = both_engines(&plan, true).await;
    assert_eq!(rows, vec!["carol"]);
}

#[tokio::test]
async fn gremlin_out_over_user_follow_table() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Has {
                key: "name".into(),
                value: Lit::String("alice".into()),
            },
            gb::Step::Out {
                rel_types: vec!["FOLLOWS".into()],
            },
            gb::Step::Values("name".into()),
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let rows = both_engines(&plan, false).await;
    assert_eq!(rows, vec!["bob", "carol"]);
}

#[tokio::test]
async fn gremlin_out_ordered_totals() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Has {
                key: "name".into(),
                value: Lit::String("alice".into()),
            },
            gb::Step::Out {
                rel_types: vec!["ORDERED".into()],
            },
            gb::Step::Values("total".into()),
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let rows = both_engines(&plan, false).await;
    assert_eq!(rows, vec!["120", "50"]);
}

/// Optimizer pushdown evidence: predicates and projections written against
/// graph properties end up on the *user's* columns, pruned to what the query
/// needs, both for table-backed and query/view-backed labels.
#[tokio::test]
async fn optimizer_pushes_filters_and_projections_through_the_mapping() {
    // Table-backed: `p.age` / `p.name` become `customers.age` / `full_name`;
    // the unused mapped column `city` must be pruned from the scan.
    let plan = cypher_plan("MATCH (p:Person) WHERE p.age > 29 RETURN p.name");
    let lowered = lower(&plan);
    let ctx = SessionContext::new();
    let optimized = ctx
        .execute_logical_plan(lowered.plan)
        .await
        .expect("logical plan")
        .into_optimized_plan()
        .expect("optimize");
    let display = format!("{}", optimized.display_indent());
    assert!(
        display.contains("customers.age > ") || display.contains("age > "),
        "filter not pushed onto user column:\n{display}"
    );
    let scan_line = display
        .lines()
        .find(|line| line.contains("TableScan: customers"))
        .unwrap_or_else(|| panic!("no customers scan in:\n{display}"));
    assert!(
        !scan_line.contains("city") && !scan_line.contains("cust_id"),
        "unused user columns not pruned from scan: {scan_line}"
    );

    // Query-backed: the filter pushes *through* the defining SQL down to the
    // base `customers` scan (merged with the view's own `age >= 30`).
    let plan = cypher_plan("MATCH (v:Vip) WHERE v.age > 29 RETURN v.name");
    let lowered = lower(&plan);
    let optimized = ctx
        .execute_logical_plan(lowered.plan)
        .await
        .expect("logical plan")
        .into_optimized_plan()
        .expect("optimize");
    let display = format!("{}", optimized.display_indent());
    let filter_line = display
        .lines()
        .find(|line| line.contains("customers.age"))
        .unwrap_or_else(|| panic!("no pushed-down filter on customers.age in:\n{display}"));
    assert!(
        filter_line.contains("Filter") || filter_line.contains("TableScan"),
        "expected filter merged toward the base scan: {filter_line}"
    );
}

#[test]
fn mapping_round_trips_through_toml() {
    let mapping = byos_mapping();
    let toml = mapping.to_toml();
    let reparsed = GraphMapping::from_toml(&toml).expect("reparse");
    let person = reparsed.node("Person").expect("person");
    assert_eq!(person.source, MappedSource::Table("customers".into()));
    assert_eq!(person.properties.get("name").unwrap(), "full_name");
    let vip = reparsed.node("Vip").expect("vip");
    assert_eq!(vip.source, MappedSource::Query(VIP_QUERY_SQL.into()));
    let ordered = reparsed.edge("ORDERED").expect("ordered");
    assert_eq!(ordered.id_column.as_deref(), Some("order_id"));
    assert_eq!(ordered.src_label, "Person");
}
