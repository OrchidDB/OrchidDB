use arrow::{
    array::{Int64Array, RecordBatch, StringArray},
    datatypes::{DataType, Field, Schema},
};
use datafusion::{datasource::MemTable, prelude::SessionContext};
use orchiddb::ir::rel::{
    constraints::{Constraint, ConstraintCatalog, Evidence, Fact, analyze, optimize},
    mapping::GraphMapping,
};
use std::sync::Arc;
fn names(cols: &[&str]) -> Vec<String> {
    cols.iter().map(|s| s.to_string()).collect()
}
fn fixture(evidence: Evidence) -> GraphMapping {
    let mut mapping = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("name", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])),
            Arc::new(StringArray::from(vec!["Ada", "Bob", "Bob"])),
        ],
    )
    .unwrap();
    mapping.register_table(
        "people",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("person", DataType::Int64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![10, 11, 12])),
            Arc::new(Int64Array::from(vec![1, 1, 2])),
        ],
    )
    .unwrap();
    mapping.register_table(
        "orders",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    let mut catalog = ConstraintCatalog::default();
    for (table, cols) in [("people", vec!["id"]), ("orders", vec!["id"])] {
        catalog.insert(
            table,
            Constraint {
                name: "pk".into(),
                fact: Fact::Unique {
                    columns: names(&cols),
                    nulls_equal: false,
                },
                evidence: evidence.clone(),
            },
        );
        catalog.insert(
            table,
            Constraint {
                name: "nn".into(),
                fact: Fact::NonNull {
                    columns: names(if table == "orders" {
                        &["id", "person"]
                    } else {
                        &["id"]
                    }),
                },
                evidence: evidence.clone(),
            },
        );
    }
    catalog.insert(
        "orders",
        Constraint {
            name: "person_fk".into(),
            fact: Fact::ForeignKey {
                columns: names(&["person"]),
                target: "people".into(),
                references: names(&["id"]),
            },
            evidence,
        },
    );
    mapping.set_constraints(catalog);
    mapping
}
async fn rows(plan: datafusion::logical_expr::LogicalPlan) -> Vec<Vec<String>> {
    let batches = SessionContext::new()
        .execute_logical_plan(plan)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    let mut rows = Vec::new();
    for b in batches {
        for r in 0..b.num_rows() {
            rows.push(
                b.columns()
                    .iter()
                    .map(|c| arrow::util::display::array_value_to_string(c.as_ref(), r).unwrap())
                    .collect(),
            );
        }
    }
    rows.sort();
    rows
}
#[tokio::test]
async fn benefiting_queries_preserve_results_and_have_proofs() {
    let m = fixture(Evidence::Enforced);
    for (sql, rule) in [
        ("SELECT DISTINCT id, name FROM people", "remove_distinct"),
        (
            "SELECT id,name,count(id) FROM people GROUP BY id,name",
            "reduce_grouping",
        ),
        (
            "SELECT id FROM people WHERE id > 1 UNION SELECT id FROM people",
            "contained_union",
        ),
        (
            "SELECT o.id FROM orders o JOIN people p ON o.person=p.id",
            "eliminate_join",
        ),
        (
            "SELECT o.id FROM orders o LEFT JOIN people p ON o.person=p.id",
            "eliminate_join",
        ),
        (
            "SELECT a.id,b.name FROM people a JOIN people b ON a.id=b.id",
            "eliminate_self_join",
        ),
        (
            "SELECT o.id FROM orders o LEFT SEMI JOIN people p ON o.person=p.id",
            "contained_membership",
        ),
    ] {
        let original = m.relational_plan(sql).unwrap();
        let (optimized, proofs) = optimize(original.clone()).unwrap();
        assert!(
            proofs.iter().any(|p| p.rule == rule),
            "{sql}\n{original:?}\n{proofs:?}"
        );
        assert_eq!(rows(original).await, rows(optimized).await, "{sql}");
    }
}
#[tokio::test]
async fn declarations_estimates_and_expired_scopes_do_not_authorize_rewrites() {
    for evidence in [
        Evidence::Declared,
        Evidence::Estimate,
        Evidence::Validated {
            scope: "old".into(),
        },
    ] {
        let m = fixture(evidence);
        let (_, proofs) =
            optimize(m.relational_plan("SELECT DISTINCT id FROM people").unwrap()).unwrap();
        assert!(proofs.is_empty(), "{proofs:?}");
    }
    let mut m = fixture(Evidence::Validated {
        scope: "snapshot-1".into(),
    });
    m.set_constraint_scope(Some("snapshot-1".into()));
    assert!(
        !optimize(m.relational_plan("SELECT DISTINCT id FROM people").unwrap())
            .unwrap()
            .1
            .is_empty()
    );
    m.set_constraint_scope(None);
    assert!(
        optimize(m.relational_plan("SELECT DISTINCT id FROM people").unwrap())
            .unwrap()
            .1
            .is_empty()
    );
}
#[tokio::test]
async fn fanout_filtered_targets_and_payload_distinct_are_retained() {
    let m = fixture(Evidence::Enforced);
    for sql in [
        "SELECT DISTINCT name FROM people",
        "SELECT DISTINCT p.id FROM people p JOIN orders o ON o.person=p.id",
        "SELECT o.id FROM orders o JOIN (SELECT * FROM people WHERE name='Ada') p ON o.person=p.id",
        "SELECT p.id FROM people p LEFT JOIN orders o ON o.person=p.id",
        "SELECT o.id,p.name FROM orders o JOIN people p ON o.person=p.id",
    ] {
        let original = m.relational_plan(sql).unwrap();
        let (optimized, proofs) = optimize(original.clone()).unwrap();
        assert!(proofs.is_empty(), "{sql}: {proofs:?}");
        assert_eq!(rows(original).await, rows(optimized).await);
    }
}
#[test]
fn propagation_through_lenses_and_proof_barriers() {
    let mut m = fixture(Evidence::Enforced);
    m.register_view("renamed", "SELECT id AS key, name FROM people WHERE id > 0")
        .unwrap();
    let p = m.relational_plan("SELECT key FROM renamed").unwrap();
    assert!(analyze(&p).unique_on(&[0], true));
    let p = m
        .relational_plan("SELECT CAST(id AS VARCHAR) AS key FROM people")
        .unwrap();
    assert!(!analyze(&p).unique_on(&[0], true));
    let p = m
        .relational_plan("SELECT id FROM people UNION ALL SELECT id FROM people")
        .unwrap();
    assert!(!analyze(&p).unique_on(&[0], true));
    let p = m
        .relational_plan("SELECT name,count(*) FROM people GROUP BY name")
        .unwrap();
    assert!(analyze(&p).unique_on(&[0], true));
}
#[test]
fn invalid_metadata_fails_and_serialization_preserves_evidence() {
    let mut m = fixture(Evidence::Enforced);
    let json = serde_json::to_string(m.constraints()).unwrap();
    let c: ConstraintCatalog = serde_json::from_str(&json).unwrap();
    assert_eq!(&c, m.constraints());
    let mut c = c;
    c.insert(
        "people",
        Constraint::enforced(
            "bad",
            Fact::NonNull {
                columns: names(&["missing"]),
            },
        ),
    );
    m.set_constraints(c);
    assert!(
        m.relational_plan("SELECT id FROM people")
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
}

#[cfg(feature = "duckdb")]
#[test]
fn duckdb_extraction_and_snapshot_validation() {
    use orchiddb::ir::rel::constraints::duckdb::{TableBinding, extract, validate};
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT PRIMARY KEY,name VARCHAR); CREATE TABLE orders(id BIGINT PRIMARY KEY, person BIGINT NOT NULL REFERENCES people(id)); INSERT INTO people VALUES(1,'Ada'); INSERT INTO orders VALUES(10,1),(11,1)").unwrap();
    let database: String = db
        .query_row("SELECT current_database()", [], |r| r.get(0))
        .unwrap();
    let bindings = vec![
        TableBinding::new("people", &database, "main", "people"),
        TableBinding::new("orders", &database, "main", "orders"),
    ];
    let catalog = extract(&db, &bindings).unwrap();
    assert!(
        catalog.tables["orders"]
            .iter()
            .any(|c| matches!(c.fact, Fact::ForeignKey { .. }))
    );
    db.execute_batch("BEGIN TRANSACTION").unwrap();
    let v = validate(&db, &bindings, &catalog, "test-snapshot").unwrap();
    assert!(
        v.tables
            .values()
            .flatten()
            .all(|c| matches!(c.evidence, Evidence::Validated { .. }))
    );
    db.execute_batch("ROLLBACK").unwrap();
    let mut invalid = catalog;
    invalid.insert(
        "orders",
        Constraint::enforced(
            "false_unique",
            Fact::Unique {
                columns: names(&["person"]),
                nulls_equal: false,
            },
        ),
    );
    assert!(
        validate(&db, &bindings, &invalid, "s")
            .unwrap_err()
            .contains("false_unique")
    );
}

#[tokio::test]
async fn nullable_unique_and_functional_dependency_do_not_prove_row_uniqueness() {
    let mut m = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("value", DataType::Int64, true),
    ]));
    let b = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![None, None, Some(1)])),
            Arc::new(Int64Array::from(vec![7, 7, 8])),
        ],
    )
    .unwrap();
    m.register_table(
        "t",
        Arc::new(MemTable::try_new(schema, vec![vec![b]]).unwrap()),
    );
    let mut c = ConstraintCatalog::default();
    c.insert(
        "t",
        Constraint::enforced(
            "unique_nullable",
            Fact::Unique {
                columns: names(&["id"]),
                nulls_equal: false,
            },
        ),
    );
    c.insert(
        "t",
        Constraint::enforced(
            "dependency",
            Fact::FunctionalDependency {
                determinant: names(&["id"]),
                dependent: names(&["value"]),
            },
        ),
    );
    m.set_constraints(c);
    let p = m.relational_plan("SELECT DISTINCT id FROM t").unwrap();
    assert!(optimize(p.clone()).unwrap().1.is_empty());
    assert_eq!(rows(p).await.len(), 2);
    let p = m
        .relational_plan("SELECT DISTINCT id FROM t WHERE id IS NOT NULL")
        .unwrap();
    assert!(!optimize(p).unwrap().1.is_empty());
    let p = m
        .relational_plan("SELECT id,value,count(value) FROM t GROUP BY id,value")
        .unwrap();
    let (q, proofs) = optimize(p.clone()).unwrap();
    assert!(proofs.iter().any(|p| p.rule == "reduce_grouping"));
    assert_eq!(rows(p).await, rows(q).await);
}
#[test]
fn replacing_catalog_invalidates_view_proofs() {
    let mut m = fixture(Evidence::Enforced);
    m.register_view("v", "SELECT id FROM people").unwrap();
    assert!(analyze(&m.relational_plan("SELECT * FROM v").unwrap()).unique_on(&[0], true));
    m.set_constraints(ConstraintCatalog::default());
    assert!(!analyze(&m.relational_plan("SELECT * FROM v").unwrap()).unique_on(&[0], true));
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn mapped_cypher_executes_optimized_lens_with_supplied_constraints() {
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::{
            constraints::duckdb::{TableBinding, extract},
            mapping::NodeMapping,
        },
    };
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Bob')").unwrap();
    let database: String = db
        .query_row("SELECT current_database()", [], |r| r.get(0))
        .unwrap();
    let catalog = extract(
        &db,
        &[TableBinding::new("people", database, "main", "people")],
    )
    .unwrap();
    let mut m = GraphMapping::new();
    m.register_table_schema(
        "people",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("name", DataType::Utf8, true),
        ])),
    );
    m.set_constraints(catalog);
    m.map_node(
        NodeMapping::query(
            "Person",
            "SELECT a.id,b.name FROM people a JOIN people b ON a.id=b.id",
            "id",
        )
        .property("name", "name"),
    );
    let mut e = GraphEngine::mapped(db, Arc::new(m)).unwrap();
    let r = e
        .cypher("MATCH (p:Person) RETURN p.name AS name ORDER BY name")
        .await
        .unwrap();
    assert_eq!(r.returned.batch.num_rows(), 2);
    assert_eq!(
        arrow::util::display::array_value_to_string(r.returned.batch.column(0), 0).unwrap(),
        "Ada"
    );
    assert!(
        r.stats
            .constraint_proofs
            .iter()
            .any(|p| p.rule == "eliminate_self_join"),
        "{:?}",
        r.stats
    );
    assert!(
        r.stats.sql_queries.iter().all(|s| !s.contains(" JOIN ")),
        "{:?}",
        r.stats.sql_queries
    );
}

#[tokio::test]
async fn composite_keys_need_all_columns_and_preserve_duplicate_occurrences() {
    let mut m = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("tenant", DataType::Int64, false),
        Field::new("id", DataType::Int64, false),
    ]));
    let b = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 1, 2])),
            Arc::new(Int64Array::from(vec![1, 2, 1])),
        ],
    )
    .unwrap();
    m.register_table(
        "t",
        Arc::new(MemTable::try_new(schema, vec![vec![b]]).unwrap()),
    );
    let mut c = ConstraintCatalog::default();
    c.insert(
        "t",
        Constraint::enforced(
            "composite",
            Fact::Unique {
                columns: names(&["tenant", "id"]),
                nulls_equal: false,
            },
        ),
    );
    m.set_constraints(c);
    for (sql, eliminates, count) in [
        ("SELECT DISTINCT tenant,id FROM t", true, 3),
        ("SELECT DISTINCT id FROM t", false, 2),
        ("SELECT a.id FROM t a JOIN t b ON a.id=b.id", false, 5),
        (
            "SELECT a.id FROM t a JOIN t b ON a.id=b.id AND a.tenant=b.tenant",
            true,
            3,
        ),
        ("SELECT id FROM t UNION ALL SELECT id FROM t", false, 6),
    ] {
        let p = m.relational_plan(sql).unwrap();
        let (q, proofs) = optimize(p.clone()).unwrap();
        assert_eq!(!proofs.is_empty(), eliminates, "{sql}: {proofs:?}");
        let before = rows(p).await;
        assert_eq!(before.len(), count, "{sql}");
        assert_eq!(before, rows(q).await);
    }
    let reloaded = GraphMapping::from_toml(&m.to_toml()).unwrap();
    assert_eq!(m.constraints(), reloaded.constraints());
}
#[test]
fn relationship_multiplicity_separates_degree_from_endpoint_integrity() {
    use orchiddb::ir::rel::mapping::{EdgeMapping, NodeMapping};
    let mut m = fixture(Evidence::Enforced);
    m.map_node(NodeMapping::table("Person", "people", "id"));
    m.map_node(NodeMapping::table("Order", "orders", "id"));
    m.map_edge(
        EdgeMapping::table("ORDERED", "orders", "person", "id", "Person", "Order").with_id("id"),
    );
    let bounds = m.relationship_multiplicity("ORDERED").unwrap();
    assert!(!bounds.at_most_one_outgoing);
    assert!(bounds.at_most_one_incoming);
    assert!(bounds.source_endpoint_exists);
    assert!(bounds.target_endpoint_exists);
}
#[tokio::test]
async fn throwing_projection_and_filtered_self_join_cannot_be_discarded() {
    let m = fixture(Evidence::Enforced);
    for sql in [
        "SELECT a.id FROM people a JOIN (SELECT id,CAST(name AS BIGINT) AS bad FROM people) b ON a.id=b.id",
        "SELECT a.id FROM people a JOIN (SELECT id FROM people WHERE name='Ada') b ON a.id=b.id",
    ] {
        let (_, proofs) = optimize(m.relational_plan(sql).unwrap()).unwrap();
        assert!(proofs.is_empty(), "{sql}: {proofs:?}");
    }
}

#[tokio::test]
async fn containment_keeps_union_output_names_and_never_removes_bag_branches() {
    let m = fixture(Evidence::Enforced);
    let original=m.relational_plan("SELECT id AS first_name FROM people WHERE id>1 UNION SELECT id AS second_name FROM people").unwrap();
    let (optimized, proofs) = optimize(original.clone()).unwrap();
    assert!(proofs.iter().any(|p| p.rule == "contained_union"));
    assert_eq!(
        original.schema().field(0).name(),
        optimized.schema().field(0).name()
    );
    assert_eq!(rows(original).await, rows(optimized).await);
}
#[test]
fn cyclic_view_replacement_is_rejected() {
    let mut m = fixture(Evidence::Enforced);
    m.register_view("a", "SELECT id FROM people").unwrap();
    m.register_view("b", "SELECT id FROM a").unwrap();
    assert!(m.register_view("a", "SELECT id FROM b").is_err());
}
#[tokio::test]
async fn compiler_accepts_supplied_constraints_without_database_discovery() {
    let catalog = fixture(Evidence::Enforced).constraints().clone();
    let request = serde_json::json!({"version":1,"dialect":"postgres","language":"cypher","query":"MATCH (p:Person) RETURN DISTINCT p.id AS id", "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},{"name":"orders","columns":[{"name":"id","data_type":"int64"},{"name":"person","data_type":"int64"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id"}}],"constraints":catalog});
    let response = orchiddb::compiler::compile_json(&request.to_string())
        .await
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(value["fields"], serde_json::json!(["id"]));
    assert!(value.get("constraint_proofs").is_some());
    assert!(!value["sql"].as_str().unwrap().contains("ROW_NUMBER"));
}

#[cfg(feature = "duckdb")]
#[test]
fn extraction_rejects_mismatched_collation_domains() {
    use orchiddb::ir::rel::constraints::duckdb::{TableBinding, extract};
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE names(id VARCHAR COLLATE NOCASE PRIMARY KEY)")
        .unwrap();
    let database: String = db
        .query_row("SELECT current_database()", [], |r| r.get(0))
        .unwrap();
    assert!(
        extract(
            &db,
            &[TableBinding::new("names", database, "main", "names")]
        )
        .unwrap_err()
        .contains("collated")
    );
}
#[cfg(feature = "duckdb")]
#[test]
fn rewritten_sql_matches_duckdb_for_benefiting_queries() {
    use orchiddb::ir::{
        policy::ResultForm,
        rel::{
            LoweredPlan,
            sql::{SqlDialect, unparse},
        },
    };
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT PRIMARY KEY,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Bob'),(3,'Bob'); CREATE TABLE orders(id BIGINT PRIMARY KEY,person BIGINT NOT NULL REFERENCES people(id)); INSERT INTO orders VALUES (10,1),(11,1),(12,2)").unwrap();
    let m = fixture(Evidence::Enforced);
    let execute = |sql: &str| {
        let mut stmt = db.prepare(sql).unwrap();
        let batches = stmt.query_arrow([]).unwrap();
        let mut result = Vec::new();
        for b in batches {
            for r in 0..b.num_rows() {
                result.push(
                    b.columns()
                        .iter()
                        .map(|c| {
                            arrow::util::display::array_value_to_string(c.as_ref(), r).unwrap()
                        })
                        .collect::<Vec<_>>(),
                );
            }
        }
        result.sort();
        result
    };
    for sql in [
        "SELECT DISTINCT id,name FROM people",
        "SELECT o.id FROM orders o JOIN people p ON o.person=p.id",
        "SELECT a.id,b.name FROM people a JOIN people b ON a.id=b.id",
        "SELECT id,name,count(id) FROM people GROUP BY id,name",
        "SELECT id AS a FROM people WHERE id>1 UNION SELECT id AS b FROM people",
        "SELECT o.id FROM orders o LEFT SEMI JOIN people p ON o.person=p.id",
    ] {
        let (plan, proofs) = optimize(m.relational_plan(sql).unwrap()).unwrap();
        assert!(!proofs.is_empty(), "{sql}");
        let fields = plan
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        let rewritten = unparse(
            &LoweredPlan {
                plan,
                fields,
                result_form: ResultForm::RowSet,
                islands: Default::default(),
            },
            SqlDialect::DuckDb,
        )
        .unwrap();
        // DuckDB spells this operator SEMI JOIN rather than LEFT SEMI JOIN.
        assert_eq!(
            execute(&sql.replace("LEFT SEMI JOIN", "SEMI JOIN")),
            execute(&rewritten),
            "{sql}\n{rewritten}"
        );
    }
}

#[tokio::test]
async fn dependency_into_nullable_unique_does_not_prove_join_cardinality() {
    let mut m = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("a", DataType::Int64, false),
        Field::new("b", DataType::Int64, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 1])),
            Arc::new(Int64Array::from(vec![None::<i64>, None])),
        ],
    )
    .unwrap();
    m.register_table(
        "t",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    let mut c = ConstraintCatalog::default();
    c.insert(
        "t",
        Constraint::enforced(
            "nullable_unique",
            Fact::Unique {
                columns: names(&["b"]),
                nulls_equal: false,
            },
        ),
    );
    c.insert(
        "t",
        Constraint::enforced(
            "fd",
            Fact::FunctionalDependency {
                determinant: names(&["a"]),
                dependent: names(&["b"]),
            },
        ),
    );
    m.set_constraints(c);
    let p = m
        .relational_plan("SELECT l.a FROM t l LEFT JOIN t r ON l.a=r.a")
        .unwrap();
    let (q, proofs) = optimize(p.clone()).unwrap();
    assert!(proofs.is_empty());
    assert_eq!(rows(p).await.len(), 4);
    assert_eq!(rows(q).await.len(), 4);
}

#[test]
fn changed_view_schema_rejects_stale_constraint_columns() {
    let mut m = fixture(Evidence::Enforced);
    m.register_view("v", "SELECT * FROM people").unwrap();
    let mut catalog = m.constraints().clone();
    catalog.insert(
        "v",
        Constraint::enforced(
            "name_required",
            Fact::NonNull {
                columns: names(&["name"]),
            },
        ),
    );
    m.set_constraints(catalog);
    m.register_table_schema(
        "people",
        Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
    );
    let error = m
        .relational_plan("SELECT * FROM v")
        .unwrap_err()
        .to_string();
    assert!(error.contains("missing column v.name"), "{error}");
}

#[test]
fn implicit_throwing_comparisons_are_not_removed_with_unused_join_sides() {
    let m = fixture(Evidence::Enforced);
    let p=m.relational_plan("SELECT o.id FROM orders o LEFT JOIN (SELECT id FROM people WHERE id='not_an_integer') p ON o.person=p.id").unwrap();
    assert!(optimize(p).unwrap().1.is_empty());
}
