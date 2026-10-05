use arrow::{
    array::{Array, ArrayRef, Float64Array, RecordBatch, StringArray},
    util::display::array_value_to_string,
};
use datafusion::{
    logical_expr::{Expr, col, lit},
    prelude::SessionContext,
};
use orchiddb::ir::{
    functions::{logical::definition, portable},
    policy::ResultForm,
    rel::{
        LoweredPlan,
        sql::{self, SqlDialect},
    },
};
use std::{process::Command, sync::Arc};

fn call(name: &str, args: Vec<Expr>) -> Expr {
    portable::function(&format!("fn.{name}"))
        .unwrap()
        .call(args)
}

#[test]
fn catalog_covers_every_native_scalar_and_alias_without_engine_dependencies() {
    let natives = datafusion::functions::all_default_functions()
        .into_iter()
        .chain(datafusion::functions_nested::all_default_nested_functions())
        .collect::<Vec<_>>();
    assert_eq!(portable::capabilities().len(), natives.len());
    for native in &natives {
        for name in
            std::iter::once(native.name()).chain(native.aliases().iter().map(String::as_str))
        {
            let expected = natives
                .iter()
                .find(|f| f.name() == name || f.aliases().iter().any(|a| a == name))
                .unwrap();
            let found = portable::function(&format!("FN.{}", name.to_uppercase())).unwrap();
            assert_eq!(
                definition(&found).unwrap().logical_name(),
                format!("fn.{}", expected.name())
            );
            assert_eq!(found.signature(), expected.signature());
            assert_eq!(
                found.inner().short_circuits(),
                expected.inner().short_circuits()
            );
        }
    }
    assert!(portable::function("sqrt").is_none());
    assert!(portable::function("fn.does_not_exist").is_none());
    for capability in portable::capabilities() {
        assert!(
            !capability.duckdb_note.starts_with("NOT AUDITED"),
            "{}",
            capability.name
        );
        assert!(
            !capability.postgres_note.starts_with("NOT AUDITED"),
            "{}",
            capability.name
        );
    }
    let mapped = portable::capabilities()
        .iter()
        .filter(|f| f.duckdb && f.postgres)
        .count();
    println!(
        "{} native portable functions, {mapped} mapped to both SQL engines",
        natives.len()
    );
    assert_eq!(
        mapped,
        natives.len(),
        "every catalog function must have SQL support or preparation lowering on both engines"
    );
}

async fn fixture(name: &str, strings: bool, extra: Vec<Expr>) -> (SessionContext, LoweredPlan) {
    let ctx = SessionContext::new();
    let array: ArrayRef = if strings {
        Arc::new(StringArray::from(vec![
            Some(""),
            Some("abc"),
            Some("  é👩‍💻a\u{301}  "),
            Some("a'b\\c"),
            None,
        ]))
    } else {
        Arc::new(Float64Array::from(vec![
            Some(-2.),
            Some(-1.),
            Some(-0.),
            Some(0.),
            Some(0.5),
            Some(1.),
            Some(2.),
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(f64::NEG_INFINITY),
            None,
        ]))
    };
    ctx.register_batch(
        "input",
        RecordBatch::try_from_iter(vec![("x", array)]).unwrap(),
    )
    .unwrap();
    let args = std::iter::once(col("x")).chain(extra).collect();
    let frame = ctx
        .table("input")
        .await
        .unwrap()
        .select(vec![call(name, args).alias("value")])
        .unwrap();
    let state = ctx.state();
    let plan = state
        .analyzer()
        .execute_and_check(
            frame.into_unoptimized_plan(),
            state.config_options(),
            |_, _| {},
        )
        .unwrap();
    (
        ctx,
        LoweredPlan {
            plan,
            fields: vec!["value".into()],
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        },
    )
}

#[tokio::test]
async fn arity_mappings_coexist_and_variadic_arguments_remain_expressions() {
    let ctx = SessionContext::new();
    ctx.register_batch(
        "input",
        RecordBatch::try_from_iter(vec![(
            "x",
            Arc::new(StringArray::from(vec![" a "])) as ArrayRef,
        )])
        .unwrap(),
    )
    .unwrap();
    let plan = ctx
        .table("input")
        .await
        .unwrap()
        .select(vec![
            call("btrim", vec![col("x")]).alias("one"),
            call("btrim", vec![col("x"), lit("a")]).alias("two"),
            call(
                "concat",
                vec![col("x"), lit("__args"), lit("'); DROP TABLE input; --")],
            )
            .alias("many"),
        ])
        .unwrap()
        .into_unoptimized_plan();
    let lowered = LoweredPlan {
        plan,
        fields: vec!["one".into(), "two".into(), "many".into()],
        result_form: ResultForm::RowSet,
        islands: Default::default(),
    };
    for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
        let query = sql::unparse(&lowered, dialect).unwrap();
        assert!(query.contains("'__args'"), "{query}");
        assert!(query.contains("DROP TABLE input; --'"), "{query}");
        assert!(!query.contains("__orchiddb_logical_"), "{query}");
    }
}

#[tokio::test]
async fn portable_functions_are_available_in_cypher_without_an_engine_catalog() {
    use orchiddb::{
        ir::{
            catalog::PropertyGraph,
            rel::{RelBackend, execute_lowered},
        },
        language::cypher::{parse_query, planner::CypherPlanner},
    };
    let graph = PropertyGraph::new();
    let ir = CypherPlanner::new()
        .plan(&parse_query("RETURN fn.sqrt(9.0), fn.upper('hello')").unwrap())
        .unwrap();
    let plan = RelBackend::default().lower(&ir, &graph).unwrap();
    let result = execute_lowered(plan).await.unwrap();
    assert_eq!(
        array_value_to_string(result.batch.column(0), 0).unwrap(),
        "3.0"
    );
    assert_eq!(
        array_value_to_string(result.batch.column(1), 0).unwrap(),
        "HELLO"
    );
}

// These execute emitted SQL against actual engines, on column inputs so
// constant folding cannot turn the backend checks into tests of literals.
#[tokio::test]
#[ignore = "requires duckdb and psql CLI; PostgreSQL database defaults to postgres"]
async fn native_duckdb_and_postgres_agree_on_scalar_edge_cases() {
    let mut cases = vec![];
    for name in [
        "abs", "ceil", "floor", "atan", "cbrt", "asinh", "tanh", "isnan", "iszero", "signum",
        "sqrt", "ln", "log2", "log10", "acos", "asin", "acosh", "atanh", "sin", "cos", "tan",
        "log", "degrees", "radians", "exp", "sinh", "cosh", "cot", "round", "trunc",
    ] {
        cases.push((name, false, vec![]));
    }
    cases.push(("atan2", false, vec![lit(2.0)]));
    cases.push(("log", false, vec![lit(2.0)]));
    for exponent in [0.0, 0.5, 1.0, 2.0] {
        cases.push(("power", false, vec![lit(exponent)]));
    }
    for name in ["round", "trunc"] {
        cases.push((name, false, vec![lit(2_i64)]));
    }
    for name in ["greatest", "least"] {
        cases.push((name, false, vec![lit(2.0)]));
    }
    for name in ["left", "right", "repeat", "lpad", "rpad", "substr"] {
        cases.push((name, true, vec![lit(2_i64)]));
    }
    cases.push(("substr", true, vec![lit(-2_i64), lit(5_i64)]));
    cases.push(("lpad", true, vec![lit(7_i64), lit("é_")]));
    cases.push(("rpad", true, vec![lit(7_i64), lit("é_")]));
    cases.push(("split_part", true, vec![lit("a"), lit(1_i64)]));
    cases.push(("find_in_set", true, vec![lit(",abc,z")]));
    cases.push(("find_in_set", true, vec![lit("")]));
    cases.push(("nvl2", true, vec![lit("yes"), lit("no")]));
    cases.push(("nanvl", false, vec![lit(7.0)]));
    for name in [
        "character_length",
        "octet_length",
        "bit_length",
        "reverse",
        "ascii",
        "md5",
        "btrim",
        "ltrim",
        "rtrim",
    ] {
        cases.push((name, true, vec![]));
    }
    for name in [
        "contains",
        "starts_with",
        "ends_with",
        "strpos",
        "btrim",
        "ltrim",
        "rtrim",
        "nullif",
        "nvl",
        "coalesce",
    ] {
        cases.push((name, true, vec![lit("a")]));
    }
    cases.push(("replace", true, vec![lit("a"), lit("z")]));
    cases.push(("concat", true, vec![lit("|"), lit("__args")]));
    cases.push(("concat_ws", true, vec![lit("a"), lit("b")]));
    let mut failures = Vec::new();
    for (name, strings, extra) in cases {
        let arity = extra.len() + 1;
        let (ctx, plan) = fixture(name, strings, extra).await;
        let batches = ctx
            .execute_logical_plan(plan.plan.clone())
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();
        let expected = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows()).map(|r| {
                    if batch.column(0).is_null(r) {
                        None
                    } else {
                        Some(array_value_to_string(batch.column(0), r).unwrap())
                    }
                })
            })
            .collect::<Vec<_>>();
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            if definition(&portable::function(&format!("fn.{name}")).unwrap())
                .unwrap()
                .sql_mapping(dialect.name(), arity)
                .is_none()
            {
                continue;
            }
            let prepared = sql::prepare(&plan, dialect).await.unwrap();
            let mut statements = vec![];
            for table in &prepared.tables {
                statements.extend(sql::table_setup_sql(dialect, table).unwrap());
            }
            let type_function = if dialect == SqlDialect::Postgres {
                "pg_typeof"
            } else {
                "typeof"
            };
            let wrapped = format!(
                "SELECT CAST(value AS VARCHAR) AS value, CAST({type_function}(value) AS VARCHAR) AS sql_type FROM ({}) AS result",
                prepared.query
            );
            let sql = if dialect == SqlDialect::Postgres {
                statements.push(format!("SELECT row_to_json(r) FROM ({wrapped}) AS r"));
                statements.join(";\n")
            } else {
                statements.push(wrapped);
                statements.join(";\n")
            };
            let output = if dialect == SqlDialect::DuckDb {
                Command::new("duckdb")
                    .args(["-json", "-c", &sql])
                    .output()
                    .unwrap()
            } else {
                Command::new("psql")
                    .args([
                        "-X",
                        "-qAt",
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-d",
                        &std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| "postgres".into()),
                        "-c",
                        &sql,
                    ])
                    .output()
                    .unwrap()
            };
            if !output.status.success() {
                failures.push(format!(
                    "{name} {dialect:?}: {}\n{sql}",
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
            let stdout = String::from_utf8(output.stdout).unwrap();
            let rows: Vec<serde_json::Value> = if dialect == SqlDialect::DuckDb {
                serde_json::from_str(stdout.trim()).unwrap()
            } else {
                stdout
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect()
            };
            assert_eq!(rows.len(), expected.len(), "{name} {dialect:?}");
            for (row, expected) in rows.iter().zip(&expected) {
                use arrow::datatypes::DataType;
                let native_type = batches[0].column(0).data_type();
                let expected_type = match native_type {
                    DataType::Int32 => Some("integer"),
                    DataType::Int64 => Some("bigint"),
                    DataType::Float64 => Some(if dialect == SqlDialect::Postgres {
                        "double precision"
                    } else {
                        "double"
                    }),
                    _ => None,
                };
                if let Some(expected_type) = expected_type {
                    assert_eq!(
                        row["sql_type"].as_str().unwrap().to_lowercase(),
                        expected_type,
                        "{name}: {dialect:?}"
                    );
                }
                let actual = row["value"].as_str();
                match (expected.as_deref(), actual) {
                    (Some(a), Some(b))
                        if !strings && a.parse::<f64>().is_ok() && b.parse::<f64>().is_ok() =>
                    {
                        let (a, b) = (a.parse::<f64>().unwrap(), b.parse::<f64>().unwrap());
                        if !(a == b || a.is_nan() && b.is_nan() || (a - b).abs() < 1e-12) {
                            failures.push(format!("{name} {dialect:?}: {a} != {b}"));
                        }
                    }
                    (a, b) => {
                        if a != b {
                            failures.push(format!("{name} {dialect:?}: {a:?} != {b:?}"));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn portable_calls_place_sql_and_native_residuals_by_capability() {
    for (name, strings, extra, residual) in [
        ("sqrt", false, vec![], false),
        ("upper", true, vec![], false),
        ("log", false, vec![lit(2.0)], false),
    ] {
        let (_, plan) = fixture(name, strings, extra).await;
        let (_, stats) = orchiddb::ir::rel::dag::execute(plan).await.unwrap();
        assert_eq!(stats.duckdb_regions, 1, "{name}: {stats:?}");
        assert_eq!(
            stats.datafusion_operators > 0,
            residual,
            "{name}: {stats:?}"
        );
        assert!(
            stats
                .sql_queries
                .iter()
                .all(|q| !q.contains("__orchiddb_logical_"))
        );
    }
    let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true)
        .project(vec![call("upper", vec![lit("hello")]).alias("value")])
        .unwrap()
        .build()
        .unwrap();
    let (_, stats) = orchiddb::ir::rel::dag::execute(LoweredPlan {
        plan,
        fields: vec!["value".into()],
        result_form: ResultForm::RowSet,
        islands: Default::default(),
    })
    .await
    .unwrap();
    assert_eq!(stats.duckdb_regions, 1);
    assert_eq!(stats.datafusion_operators, 0);
    assert!(stats.sql_queries[0].contains("HELLO"));
}

#[tokio::test]
#[ignore = "requires DuckDB 1.5.2+ and psql CLI"]
async fn sql_templates_evaluate_volatile_arguments_once() {
    use arrow::datatypes::DataType;
    use datafusion::logical_expr::{LogicalPlanBuilder, Volatility, create_udf};
    // Engine-owned expression: binding/unparsing must never invoke it locally.
    let next = create_udf(
        "__engine_function_nextval",
        vec![DataType::Utf8],
        DataType::Float64,
        Volatility::Volatile,
        Arc::new(|_| panic!("engine call evaluated locally")),
    );
    for name in ["sqrt", "signum", "ln", "isnan"] {
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![
                call(name, vec![next.call(vec![lit("portable_function_calls")])]).alias("value"),
            ])
            .unwrap()
            .build()
            .unwrap();
        let lowered = LoweredPlan {
            plan,
            fields: vec!["value".into()],
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        };
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let query = sql::unparse(&lowered, dialect).unwrap();
            let script = format!(
                "CREATE TEMPORARY SEQUENCE portable_function_calls; {query}; SELECT currval('portable_function_calls');"
            );
            let output = if dialect == SqlDialect::DuckDb {
                Command::new("duckdb")
                    .args(["-csv", "-noheader", "-c", &script])
                    .output()
                    .unwrap()
            } else {
                Command::new("psql")
                    .args([
                        "-X",
                        "-qAt",
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-d",
                        &std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| "postgres".into()),
                        "-c",
                        &script,
                    ])
                    .output()
                    .unwrap()
            };
            assert!(
                output.status.success(),
                "{name} {dialect:?}: {}\n{script}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            assert_eq!(
                stdout.lines().last(),
                Some("1"),
                "{name} {dialect:?}: {stdout}"
            );
        }
    }
}

#[tokio::test]
async fn portable_coalesce_keeps_lazy_argument_evaluation() {
    use arrow::datatypes::DataType;
    use datafusion::logical_expr::{Volatility, create_udf};
    let fail = create_udf(
        "must_not_execute",
        vec![],
        DataType::Utf8,
        Volatility::Volatile,
        Arc::new(|_| panic!("unused coalesce branch ran")),
    );
    let ctx = SessionContext::new();
    ctx.register_batch(
        "input",
        RecordBatch::try_from_iter(vec![(
            "x",
            Arc::new(StringArray::from(vec!["safe", "also safe"])) as ArrayRef,
        )])
        .unwrap(),
    )
    .unwrap();
    let result = ctx
        .table("input")
        .await
        .unwrap()
        .select(vec![call("coalesce", vec![col("x"), fail.call(vec![])])])
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(
        array_value_to_string(result[0].column(0), 0).unwrap(),
        "safe"
    );
}

#[tokio::test]
#[ignore = "requires DuckDB 1.5.2+ and psql CLI"]
async fn expanded_catalog_matches_native_columns() {
    use arrow::{
        array::{Int64Array, ListArray},
        datatypes::Int64Type,
    };
    let ints: ArrayRef = Arc::new(Int64Array::from(vec![
        Some(-2),
        Some(0),
        Some(3),
        Some(5),
        None,
    ]));
    let lists: ArrayRef = Arc::new(ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
        Some(vec![Some(3), None, Some(1), Some(3), Some(2)]),
        Some(vec![]),
        Some(vec![None, None]),
        Some(vec![Some(1), Some(2)]),
        None,
    ]));
    let strings: ArrayRef = Arc::new(StringArray::from(vec![
        Some(""),
        Some("abc"),
        Some("é👩‍💻a\u{301}"),
        Some("\\x'"),
        None,
    ]));
    let ctx = SessionContext::new();
    let timestamps: ArrayRef = Arc::new(arrow::array::TimestampMicrosecondArray::from(vec![
        Some(-1_234_567),
        Some(0),
        Some(1_709_210_096_123_456),
        Some(1_735_689_599_999_999),
        None,
    ]));
    ctx.register_batch(
        "input",
        RecordBatch::try_from_iter(vec![
            ("i", ints),
            ("a", lists),
            ("s", strings),
            ("t", timestamps),
        ])
        .unwrap(),
    )
    .unwrap();
    let mut cases = Vec::new();
    for name in [
        "array_length",
        "array_ndims",
        "array_dims",
        "cardinality",
        "empty",
        "flatten",
        "array_min",
        "array_max",
        "array_any_value",
        "array_reverse",
        "array_pop_front",
        "array_pop_back",
        "array_distinct",
        "array_sort",
    ] {
        cases.push((name, vec![col("a")]));
    }
    cases.push(("make_array", vec![col("i"), lit(3_i64)]));
    cases.push(("array_distance", vec![col("a"), col("a")]));
    cases.push(("array_append", vec![col("a"), col("i")]));
    cases.push(("array_prepend", vec![col("i"), col("a")]));
    for name in [
        "array_element",
        "array_has",
        "array_position",
        "array_positions",
        "array_remove",
        "array_remove_all",
    ] {
        cases.push((name, vec![col("a"), col("i")]));
        cases.push((name, vec![col("a"), lit(3_i64)]));
    }
    for name in [
        "array_has_all",
        "array_has_any",
        "array_union",
        "array_intersect",
        "array_except",
        "array_concat",
    ] {
        cases.push((
            name,
            vec![
                col("a"),
                call("make_array", vec![lit(3_i64), lit(3_i64), lit(1_i64)]),
            ],
        ));
        cases.push((name, vec![col("a"), call("make_array", vec![col("i")])]));
    }
    for name in ["array_replace", "array_replace_all"] {
        cases.push((name, vec![col("a"), col("i"), lit(9_i64)]));
        cases.push((name, vec![col("a"), lit(3_i64), lit(9_i64)]));
    }
    for n in [-1, 0, 1, 2] {
        cases.push(("array_remove_n", vec![col("a"), lit(3_i64), lit(n as i64)]));
        cases.push((
            "array_replace_n",
            vec![col("a"), lit(3_i64), lit(9_i64), lit(n as i64)],
        ));
    }
    cases.push(("array_repeat", vec![col("i"), lit(3_i64)]));
    for name in ["factorial", "to_hex"] {
        cases.push((name, vec![col("i")]));
    }
    for name in ["gcd", "lcm"] {
        cases.push((name, vec![col("i"), lit(6_i64)]));
    }
    for name in ["sha224", "sha256", "sha384", "sha512", "levenshtein"] {
        cases.push((
            name,
            if name == "levenshtein" {
                vec![col("s"), lit("é")]
            } else {
                vec![col("s")]
            },
        ));
    }
    for name in [
        "to_timestamp",
        "to_timestamp_seconds",
        "to_timestamp_millis",
        "to_timestamp_micros",
        "from_unixtime",
    ] {
        cases.push((name, vec![col("i")]));
    }
    for name in ["to_date", "to_local_time", "to_unixtime"] {
        cases.push((name, vec![col("t")]));
    }
    cases.push((
        "make_date",
        vec![col("i") + lit(2020_i64), lit(2_i64), lit(28_i64)],
    ));
    cases.push((
        "make_time",
        vec![col("i") + lit(4_i64), lit(2_i64), lit(3_i64)],
    ));
    for unit in [
        "year", "quarter", "month", "week", "day", "hour", "minute", "second",
    ] {
        for name in ["date_part", "date_trunc"] {
            cases.push((name, vec![lit(unit), col("t")]));
        }
    }
    for format in ["%Y-%m-%d", "%H:%M:%S", "%Y-%m-%d %H:%M:%S"] {
        cases.push(("to_char", vec![col("t"), lit(format)]));
    }
    for unit in ["dow", "doy", "isoyear", "isodow"] {
        cases.push(("date_part", vec![lit(unit), col("t")]));
    }
    for count in [-3_i64, -1, 0, 1, 3] {
        cases.push((
            "substr_index",
            vec![
                call("concat", vec![col("s"), lit("aaa")]),
                lit("aa"),
                lit(count),
            ],
        ));
    }
    cases.push((
        "date_bin",
        vec![
            lit(datafusion::common::ScalarValue::new_interval_mdn(
                0,
                0,
                60_000_000_000,
            )),
            col("t"),
            lit(datafusion::common::ScalarValue::TimestampMicrosecond(
                Some(0),
                None,
            )),
        ],
    ));
    for format in ["hex", "base64", "base64pad"] {
        cases.push(("encode", vec![col("s"), lit(format)]));
    }
    for format in ["hex", "base64", "base64pad"] {
        cases.push((
            "decode",
            vec![call("encode", vec![col("s"), lit(format)]), lit(format)],
        ));
        cases.push(("decode", vec![col("s"), lit(format)]));
    }
    for algorithm in ["md5", "sha256"] {
        cases.push(("digest", vec![col("s"), lit(algorithm)]));
    }
    cases.push((
        "make_time",
        vec![col("i") + lit(24_i64), lit(0_i64), lit(0_i64)],
    ));
    for direction in ["ASC", "DESC"] {
        cases.push(("array_sort", vec![col("a"), lit(direction)]));
        for nulls in ["NULLS FIRST", "NULLS LAST"] {
            cases.push(("array_sort", vec![col("a"), lit(direction), lit(nulls)]));
        }
    }
    for n in [0_i64, 2, 7] {
        cases.push(("array_resize", vec![col("a"), lit(n)]));
        cases.push(("array_resize", vec![col("a"), lit(n), lit(9_i64)]));
    }
    cases.push(("array_position", vec![col("a"), lit(3_i64), lit(2_i64)]));
    cases.push(("array_position", vec![col("a"), lit(3_i64), lit(1_i64)]));
    cases.push(("factorial", vec![col("i") + lit(25_i64)]));
    for (start, end) in [(0_i64, 3_i64), (-2, -1), (-9, 99), (2, 4), (2, 0)] {
        cases.push(("array_slice", vec![col("a"), lit(start), lit(end)]));
        cases.push((
            "array_slice",
            vec![col("a"), lit(start), lit(end), lit(2_i64)],
        ));
    }
    for name in ["range", "generate_series"] {
        cases.push((name, vec![col("i")]));
        cases.push((name, vec![col("i"), lit(6_i64)]));
        cases.push((name, vec![col("i"), lit(-3_i64), lit(-2_i64)]));
    }
    for delimiter in [
        lit(""),
        lit("a"),
        lit(datafusion::common::ScalarValue::Utf8(None)),
    ] {
        cases.push(("string_to_array", vec![col("s"), delimiter.clone()]));
        cases.push(("string_to_array", vec![col("s"), delimiter, lit("")]));
    }
    let text_list = call(
        "make_array",
        vec![
            col("s"),
            lit(datafusion::common::ScalarValue::Utf8(None)),
            lit("tail"),
        ],
    );
    cases.push(("array_to_string", vec![text_list.clone(), lit("|")]));
    cases.push(("array_to_string", vec![text_list, lit("|"), lit("missing")]));
    for name in [
        "regexp_like",
        "regexp_count",
        "regexp_instr",
        "regexp_match",
    ] {
        cases.push((name, vec![col("s"), lit("[a-z]+")]));
        cases.push((name, vec![col("s"), lit("z+")]));
    }
    cases.push(("regexp_replace", vec![col("s"), lit("[a-z]+"), lit("é")]));
    let mut failures = vec![];
    for (name, args) in cases {
        let label = format!("{name} {args:?}");
        let arity = args.len();
        let frame = ctx
            .table("input")
            .await
            .unwrap()
            .select(vec![call(name, args).alias("value")])
            .unwrap();
        let state = ctx.state();
        let plan = state
            .analyzer()
            .execute_and_check(
                frame.into_unoptimized_plan(),
                state.config_options(),
                |_, _| {},
            )
            .unwrap();
        let native = ctx
            .execute_logical_plan(plan.clone())
            .await
            .unwrap()
            .collect()
            .await;
        let expect_error = native.is_err();
        let native_type = plan.schema().field(0).data_type().clone();
        let batches = native.unwrap_or_default();
        let binary = matches!(
            native_type,
            arrow::datatypes::DataType::Binary | arrow::datatypes::DataType::LargeBinary
        );
        let expected: Vec<serde_json::Value> = if expect_error {
            vec![]
        } else if binary {
            batches
                .iter()
                .flat_map(|b| {
                    (0..b.num_rows()).map(|i| {
                        if b.column(0).is_null(i) {
                            serde_json::Value::Null
                        } else {
                            serde_json::json!(
                                array_value_to_string(b.column(0), i)
                                    .unwrap()
                                    .to_lowercase()
                            )
                        }
                    })
                })
                .collect()
        } else {
            let mut writer = arrow_json::ArrayWriter::new(Vec::new());
            writer
                .write_batches(&batches.iter().collect::<Vec<_>>())
                .unwrap();
            writer.finish().unwrap();
            let rows: Vec<serde_json::Value> =
                serde_json::from_slice(&writer.into_inner()).unwrap();
            rows.into_iter().map(|r| r["value"].clone()).collect()
        };
        let plan = LoweredPlan {
            plan,
            fields: vec!["value".into()],
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        };
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let udf = portable::function(&format!("fn.{name}")).unwrap();
            if definition(&udf)
                .unwrap()
                .sql_mapping(dialect.name(), arity)
                .is_none()
            {
                continue;
            }
            let prepared = match sql::prepare(&plan, dialect).await {
                Ok(p) => p,
                Err(e) => {
                    failures.push(format!("{label} {dialect:?}: prepare {e}"));
                    continue;
                }
            };
            let mut statements = vec![];
            for table in &prepared.tables {
                statements.extend(sql::table_setup_sql(dialect, table).unwrap());
            }
            let value = if binary {
                if dialect == SqlDialect::Postgres {
                    "encode(value, 'hex')"
                } else {
                    "lower(hex(value))"
                }
            } else {
                "value"
            };
            let value = if dialect == SqlDialect::DuckDb {
                format!("to_json({value})")
            } else {
                value.into()
            };
            let wrapped = format!(
                "SELECT {value} AS value FROM ({}) AS result",
                prepared.query
            );
            statements.push(if dialect == SqlDialect::Postgres {
                format!("SELECT row_to_json(r) FROM ({wrapped}) AS r")
            } else {
                wrapped
            });
            let script = statements.join(";\n");
            let output = if dialect == SqlDialect::DuckDb {
                Command::new("duckdb")
                    .args(["-json", "-c", &script])
                    .output()
                    .unwrap()
            } else {
                Command::new("psql")
                    .args([
                        "-X",
                        "-qAt",
                        "-v",
                        "ON_ERROR_STOP=1",
                        "-d",
                        &std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| "postgres".into()),
                        "-c",
                        &script,
                    ])
                    .output()
                    .unwrap()
            };
            if !output.status.success() {
                if !expect_error {
                    failures.push(format!(
                        "{label} {dialect:?}: {}\n{script}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                continue;
            }
            if expect_error {
                failures.push(format!(
                    "{label} {dialect:?}: SQL succeeded but native execution failed"
                ));
                continue;
            }
            let stdout = String::from_utf8(output.stdout).unwrap();
            let rows: Vec<serde_json::Value> = if dialect == SqlDialect::DuckDb {
                serde_json::from_str(stdout.trim()).unwrap()
            } else {
                stdout
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect()
            };
            let mut actual = rows
                .into_iter()
                .map(|r| r["value"].clone())
                .collect::<Vec<_>>();
            let temporal = matches!(native_type, arrow::datatypes::DataType::Timestamp(_, _));
            if temporal {
                for value in &mut actual {
                    if let Some(s) = value.as_str() {
                        *value = serde_json::json!(s.replace(' ', "T"));
                    }
                }
            }
            if expected.len() != actual.len()
                || expected
                    .iter()
                    .zip(&actual)
                    .any(|(a, b)| !json_equivalent(a, b))
            {
                failures.push(format!("{label} {dialect:?}: {expected:?} != {actual:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[tokio::test]
async fn unsupported_shapes_report_the_reviewed_reason() {
    for (name, strings, extra, reason) in [
        ("regexp_like", true, vec![lit(r"\w+")], "Unicode classes"),
        ("regexp_match", true, vec![lit("(a|b)")], "captures"),
        ("round", false, vec![lit(100_i64)], "precision"),
        ("to_timestamp_micros", true, vec![], "Int64"),
    ] {
        let (_, plan) = fixture(name, strings, extra).await;
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let error = sql::prepare(&plan, dialect).await.unwrap_err().to_string();
            assert!(
                error.contains(&format!("fn.{name}")) && error.contains(reason),
                "{error}"
            );
        }
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn a_mapped_function_with_an_unsupported_pattern_stays_native() {
    let (_, plan) = fixture("regexp_like", true, vec![lit(r"\w+")]).await;
    let (_, stats) = orchiddb::ir::rel::dag::execute(plan).await.unwrap();
    assert!(stats.datafusion_operators > 0, "{stats:?}");
    assert_eq!(stats.duckdb_regions, 1);
}

#[cfg(all(feature = "duckdb", feature = "postgres"))]
#[test]
#[ignore = "requires a local PostgreSQL server (GRAPH_PG_URL overrides connection)"]
fn portable_results_round_trip_through_both_executors() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    use arrow::{
        array::{BinaryArray, Int64Array, ListArray},
        datatypes::Int64Type,
    };
    let ctx = SessionContext::new();
    use arrow::buffer::{NullBuffer, OffsetBuffer, ScalarBuffer};
    use arrow::datatypes::{DataType, Field, UnionFields};
    let union: ArrayRef = Arc::new(
        arrow::array::UnionArray::try_new(
            UnionFields::try_new(
                [3, 9],
                [
                    Field::new("integer", DataType::Int64, true),
                    Field::new("text", DataType::Utf8, true),
                ],
            )
            .unwrap(),
            ScalarBuffer::from(vec![3_i8, 9, 9]),
            None,
            vec![
                Arc::new(Int64Array::from(vec![Some(42), None, None])) as ArrayRef,
                Arc::new(StringArray::from(vec![None, Some("é"), None])) as ArrayRef,
            ],
        )
        .unwrap(),
    );
    let entry_fields = arrow::datatypes::Fields::from(vec![
        Field::new("key", DataType::Utf8, false),
        Field::new("value", DataType::Int64, true),
    ]);
    let entries = arrow::array::StructArray::try_new(
        entry_fields.clone(),
        vec![
            Arc::new(StringArray::from(vec!["a", "b", "a"])) as ArrayRef,
            Arc::new(Int64Array::from(vec![Some(7), None, Some(9)])) as ArrayRef,
        ],
        None,
    )
    .unwrap();
    let maps: ArrayRef = Arc::new(
        arrow::array::MapArray::try_new(
            Arc::new(Field::new("entries", DataType::Struct(entry_fields), false)),
            OffsetBuffer::new(vec![0_i32, 2, 2, 3].into()),
            entries,
            Some(NullBuffer::from(vec![true, false, true])),
            false,
        )
        .unwrap(),
    );
    let records: ArrayRef = Arc::new(
        arrow::array::StructArray::try_new(
            vec![
                Field::new("with'quote", DataType::Int64, true),
                Field::new("inner", maps.data_type().clone(), true),
            ]
            .into(),
            vec![
                Arc::new(Int64Array::from(vec![
                    Some(9007199254740993),
                    None,
                    Some(-1),
                ])) as ArrayRef,
                maps.clone(),
            ],
            Some(NullBuffer::from(vec![true, false, true])),
        )
        .unwrap(),
    );
    let batch = RecordBatch::try_from_iter(vec![
        (
            "f",
            Arc::new(Float64Array::from(vec![f64::MAX, f64::from_bits(1), -0.0])) as ArrayRef,
        ),
        ("u", union.clone()),
        ("m", maps.clone()),
        ("r", records.clone()),
        (
            "i",
            Arc::new(Int64Array::from(vec![Some(-1), Some(0), None])) as ArrayRef,
        ),
        (
            "a",
            Arc::new(ListArray::from_iter_primitive::<Int64Type, _, _>(vec![
                Some(vec![Some(3), None, Some(3), Some(1)]),
                Some(vec![]),
                None,
            ])) as ArrayRef,
        ),
        (
            "s",
            Arc::new(StringArray::from(vec![
                Some("é\\x ß İ ﬃ ΟΣ ΟΣΑ ΟΣ́ ΟΣ́Α AΣʰA a\u{301} 👩‍💻 🇺🇳🇫🇷 क्‍ष 각각\r\n"),
                Some(""),
                None,
            ])) as ArrayRef,
        ),
        (
            "b",
            Arc::new(BinaryArray::from(vec![
                Some(&b"\x00\xff\\x".repeat(70)[..]),
                Some(&b""[..]),
                None,
            ])) as ArrayRef,
        ),
    ])
    .unwrap();
    ctx.register_batch("input", batch).unwrap();
    let record = call(
        "named_struct",
        vec![
            lit("number"),
            col("i"),
            lit("text"),
            col("s"),
            lit("bytes"),
            col("b"),
            lit("list"),
            col("a"),
            lit("float"),
            col("f"),
            lit("json"),
            lit(orchiddb::ir::functions::domain::json_scalar("null").unwrap()),
        ],
    );
    let map = call(
        "map",
        vec![
            call("make_array", vec![lit("a"), lit("b")]),
            call("make_array", vec![col("i"), col("i") + lit(1_i64)]),
        ],
    );
    let cases = vec![
        lit(datafusion::common::ScalarValue::try_from_array(&union, 2).unwrap()),
        lit(datafusion::common::ScalarValue::try_from_array(&maps, 0).unwrap()),
        lit(datafusion::common::ScalarValue::try_from_array(&records, 0).unwrap()),
        col("u"),
        col("m"),
        col("r"),
        call("arrow_typeof", vec![col("r")]),
        call("arrow_metadata", vec![col("r")]),
        call("arrow_metadata", vec![col("r"), lit("missing")]),
        call("arrow_cast", vec![col("i"), lit("Float64")]),
        call("version", vec![]),
        call("power", vec![col("f"), lit(2.0)]),
        call("power", vec![col("f"), lit(0.5)]),
        call("get_field", vec![record.clone(), lit("json")]),
        call("union_tag", vec![col("u")]),
        call("union_extract", vec![col("u"), lit("integer")]),
        call("union_extract", vec![col("u"), lit("text")]),
        call("map_keys", vec![col("m")]),
        call("map_values", vec![col("m")]),
        call("map_entries", vec![col("m")]),
        call("map_extract", vec![col("m"), lit("a")]),
        call("get_field", vec![col("r"), lit("with'quote")]),
        call("get_field", vec![col("r"), lit("inner"), lit("a")]),
        call("upper", vec![col("s")]),
        call("lower", vec![col("s")]),
        call("initcap", vec![col("s")]),
        call("levenshtein", vec![col("s"), lit("é")]),
        call("levenshtein", vec![col("s"), lit("kittenÉ👩‍💻")]),
        call("translate", vec![col("s"), lit("éa\u{301}"), lit("X👩‍💻")]),
        call("lpad", vec![col("s"), lit(5_i64), lit("é")]),
        call("rpad", vec![col("s"), lit(1_i64)]),
        call("overlay", vec![col("s"), lit("👩‍💻"), lit(2_i64)]),
        call(
            "overlay",
            vec![col("s"), lit("X"), lit(100_i64), lit(0_i64)],
        ),
        call(
            "struct",
            vec![
                call("get_field", vec![record.clone(), lit("text")]),
                call("get_field", vec![record.clone(), lit("list")]),
            ],
        ),
        record.clone(),
        call("get_field", vec![record.clone(), lit("bytes")]),
        call("get_field", vec![record.clone(), lit("list")]),
        call("struct", vec![col("i"), record]),
        map.clone(),
        call("map_keys", vec![map.clone()]),
        call("map_values", vec![map.clone()]),
        call("map_entries", vec![map.clone()]),
        call("map_extract", vec![map.clone(), lit("a")]),
        call("map_extract", vec![map.clone(), lit("missing")]),
        call("get_field", vec![map, lit("a")]),
        call(
            "arrays_zip",
            vec![col("a"), call("make_array", vec![col("s")])],
        ),
        call("array_distinct", vec![col("a")]),
        call("array_dims", vec![col("a")]),
        call("array_replace_all", vec![col("a"), lit(3_i64), lit(9_i64)]),
        call("sha256", vec![col("s")]),
        call("sha256", vec![col("b")]),
        call("sha224", vec![col("b")]),
        call("sha384", vec![col("b")]),
        call("sha512", vec![col("b")]),
        call("digest", vec![col("s"), lit("sha512")]),
        call(
            "decode",
            vec![call("encode", vec![col("b"), lit("hex")]), lit("hex")],
        ),
        call("to_timestamp_micros", vec![col("i")]),
        call(
            "make_date",
            vec![col("i") + lit(2020_i64), lit(2_i64), lit(28_i64)],
        ),
        call(
            "make_time",
            vec![col("i") + lit(4_i64), lit(2_i64), lit(3_i64)],
        ),
    ];
    let mut executors: Vec<Box<dyn sql::SqlExecutor>> = vec![
        Box::new(sql::DuckDbExecutor::new()),
        Box::new(
            sql::PostgresExecutor::connect(&std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| {
                format!(
                    "host=/tmp dbname=postgres user={}",
                    std::env::var("USER").unwrap_or_else(|_| "postgres".into())
                )
            }))
            .unwrap(),
        ),
    ];
    for expr in cases {
        let label = format!("{expr}");
        let (expected, plan) = runtime.block_on(async {
            let frame = ctx
                .table("input")
                .await
                .unwrap()
                .select(vec![expr.alias("value")])
                .unwrap();
            let state = ctx.state();
            let plan = state
                .analyzer()
                .execute_and_check(
                    frame.into_unoptimized_plan(),
                    state.config_options(),
                    |_, _| {},
                )
                .unwrap();
            let expected = ctx
                .execute_logical_plan(plan.clone())
                .await
                .unwrap()
                .collect()
                .await
                .unwrap();
            let plan = LoweredPlan {
                plan,
                fields: vec!["value".into()],
                result_form: ResultForm::RowSet,
                islands: Default::default(),
            };
            (expected, plan)
        });
        for executor in &mut executors {
            let prepared = runtime
                .block_on(sql::prepare(&plan, executor.dialect()))
                .unwrap();
            let result = sql::execute_prepared(executor.as_mut(), &prepared)
                .unwrap_or_else(|e| panic!("{label} {:?}: {e}", executor.dialect()));
            assert_eq!(result.batch.num_rows(), expected[0].num_rows());
            for row in 0..result.batch.num_rows() {
                assert_eq!(
                    result.batch.column(0).is_null(row),
                    expected[0].column(0).is_null(row)
                );
                assert_eq!(
                    array_value_to_string(result.batch.column(0), row).unwrap(),
                    array_value_to_string(expected[0].column(0), row).unwrap(),
                    "{label} {:?}",
                    executor.dialect()
                );
            }
        }
    }
}

fn json_equivalent(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                return a == b;
            }
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            a == b || (a - b).abs() <= 1e-12 * a.abs().max(1.0)
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| json_equivalent(a, b))
        }
        _ => a == b,
    }
}

#[test]
fn prepared_temporal_literals_cannot_bypass_postgres_precision_checks() {
    use datafusion::{common::ScalarValue, logical_expr::LogicalPlanBuilder};
    for ticks in [1, -1, 1000, -1000] {
        let plan = LogicalPlanBuilder::empty(true)
            .project(vec![
                lit(ScalarValue::TimestampNanosecond(Some(ticks), None)).alias("value"),
            ])
            .unwrap()
            .build()
            .unwrap();
        let plan = LoweredPlan {
            plan,
            fields: vec!["value".into()],
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        };
        let sql = sql::unparse(&plan, SqlDialect::Postgres);
        if ticks % 1000 == 0 {
            assert!(sql.is_ok(), "{sql:?}");
        } else {
            assert!(sql.unwrap_err().to_string().contains("sub-microsecond"));
        }
        assert!(sql::unparse(&plan, SqlDialect::DuckDb).is_ok());
    }
}

#[tokio::test]
async fn integer_option_guards_do_not_accept_fractional_casts() {
    // Arrow truncates fractional counts; PostgreSQL numeric-to-integer casts
    // round. An unresolved coercion must not be emitted as a database cast.
    let (ctx, plan) = fixture(
        "repeat",
        true,
        vec![Expr::Cast(datafusion::logical_expr::Cast::new(
            Box::new(lit(2.5_f64)),
            arrow::datatypes::DataType::Int64,
        ))],
    )
    .await;
    let native = ctx
        .execute_logical_plan(plan.plan.clone())
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(
        array_value_to_string(native[0].column(0), 1).unwrap(),
        "abcabc"
    );
    for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
        assert!(
            sql::unparse(&plan, dialect)
                .unwrap_err()
                .to_string()
                .contains("Int32-range literal")
        );
        assert!(
            sql::prepare(&plan, dialect)
                .await
                .unwrap_err()
                .to_string()
                .contains("Int32-range literal")
        );
    }
}

#[tokio::test]
async fn exact_postgres_power_scope_rejects_unsafe_exponents() {
    let (_, plan) = fixture("power", false, vec![lit(1024.0)]).await;
    let error = sql::prepare(&plan, SqlDialect::Postgres)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("literal exponents"), "{error}");
    assert!(sql::prepare(&plan, SqlDialect::DuckDb).await.is_ok());
}

#[tokio::test]
async fn postgres_nul_and_nanosecond_constraints_are_explicit() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for (expr, supported) in [
        (call("chr", vec![lit(65_i64)]), true),
        (call("chr", vec![lit(0_i64)]), false),
        (call("to_timestamp_nanos", vec![lit(1000_i64)]), true),
        (call("to_timestamp_nanos", vec![lit(1001_i64)]), false),
    ] {
        let plan = LoweredPlan {
            plan: LogicalPlanBuilder::empty(true)
                .project(vec![expr.alias("value")])
                .unwrap()
                .build()
                .unwrap(),
            fields: vec!["value".into()],
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        };
        assert_eq!(
            sql::prepare(&plan, SqlDialect::Postgres).await.is_ok(),
            supported
        );
    }
}
