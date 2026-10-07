//! A third SQL engine can join the public compiler without editing dispatch.
use arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use datafusion::sql::{
    sqlparser::{
        ast,
        dialect::{Dialect, GenericDialect},
    },
    unparser::dialect::{DefaultDialect, Dialect as UnparserDialect},
};
use orchiddb::{
    compiler::compile_json,
    ir::rel::sql::{DialectAdapter, SqlDialect, SqlError, SqlResult},
};
use serde_json::{Value, json};

#[derive(Debug)]
struct Warehouse;
static WAREHOUSE: Warehouse = Warehouse;
impl DialectAdapter for Warehouse {
    fn name(&self) -> &'static str {
        "adapter_test_warehouse"
    }
    fn parser_dialect(&self) -> Box<dyn Dialect> {
        Box::new(GenericDialect {})
    }
    fn unparser_dialect(&self) -> Box<dyn UnparserDialect> {
        Box::new(DefaultDialect {})
    }
    fn sql_type(&self, ty: &DataType) -> SqlResult<String> {
        match ty {
            DataType::Int64 => Ok("BIGINT".into()),
            DataType::Utf8 => Ok("VARCHAR".into()),
            DataType::Float64 => Ok("DOUBLE PRECISION".into()),
            DataType::List(field) => Ok(format!("{}[]", self.sql_type(field.data_type())?)),
            _ => Err(SqlError::Unsupported(format!("warehouse type {ty}"))),
        }
    }
    fn exchange_literal(&self, value: &ScalarValue, ty: &DataType) -> SqlResult<String> {
        let literal = match value {
            ScalarValue::Int64(Some(value)) => value.to_string(),
            ScalarValue::Float64(Some(value)) if value.is_finite() => value.to_string(),
            ScalarValue::List(array) if !value.is_null() => {
                let values = array.value(0);
                let elements = (0..values.len())
                    .map(|i| {
                        let value = ScalarValue::try_from_array(values.as_ref(), i)?;
                        self.exchange_literal(&value, values.data_type())
                    })
                    .collect::<SqlResult<Vec<_>>>()?;
                format!("ARRAY[{}]", elements.join(", "))
            }
            ScalarValue::Utf8(Some(value)) if !value.contains('\0') => {
                format!("'{}'", value.replace('\'', "''"))
            }
            value if value.is_null() => "NULL".into(),
            _ => return Err(SqlError::Unsupported("warehouse literal".into())),
        };
        Ok(format!("CAST({literal} AS {})", self.sql_type(ty)?))
    }
    fn supports_scalar_function(&self, name: &str) -> bool {
        name == "__orchiddb_json_valid"
    }
    fn lower_scalar_function(
        &self,
        name: &str,
        args: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        if name != "__orchiddb_json_valid" {
            return Ok(None);
        }
        let mut parser = datafusion::sql::sqlparser::parser::Parser::new(&GenericDialect {})
            .try_with_sql("warehouse_json_valid(value)")
            .unwrap();
        let ast::Expr::Function(mut function) = parser.parse_expr().unwrap() else {
            unreachable!()
        };
        let ast::FunctionArguments::List(arguments) = &mut function.args else {
            unreachable!()
        };
        arguments.args = vec![ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(
            args[0].clone(),
        ))];
        Ok(Some(ast::Expr::Function(function)))
    }
    fn function_mapping(
        &self,
        name: &str,
    ) -> Option<orchiddb::ir::functions::logical::SqlFunctionMapping> {
        let value = match name {
            "portable.abs" => "ABS(__arg0)",
            _ => return None,
        };
        Some(orchiddb::ir::functions::logical::SqlFunctionMapping {
            value: value.into(),
            ordering: None,
        })
    }

    fn relation_placement(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
    ) -> Option<orchiddb::ir::rel::sql::lowering::RelationPlacement> {
        let datafusion::logical_expr::LogicalPlan::Extension(extension) = plan else {
            return None;
        };
        let remote = extension.node.as_any().downcast_ref::<RemoteRows>()?;
        Some(orchiddb::ir::rel::sql::lowering::RelationPlacement {
            source: None,
            target: Some(remote.target.clone()),
        })
    }
    fn lower_relation(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        context: &orchiddb::ir::rel::sql::lowering::LoweringContext,
    ) -> SqlResult<Option<orchiddb::ir::rel::sql::lowering::RelationLowering>> {
        use orchiddb::ir::rel::{dependent::TableFunction, sql::lowering::builtin_lower_relation};
        if let datafusion::logical_expr::LogicalPlan::Filter(filter) = plan {
            if let datafusion::logical_expr::Expr::BinaryExpr(binary) = &filter.predicate {
                if binary.op == datafusion::logical_expr::Operator::Gt {
                    let predicate = binary
                        .right
                        .as_ref()
                        .clone()
                        .lt(binary.left.as_ref().clone());
                    let filter =
                        datafusion::logical_expr::Filter::try_new(predicate, filter.input.clone())?;
                    return Ok(Some(
                        orchiddb::ir::rel::sql::lowering::RelationLowering::Rewrite(
                            datafusion::logical_expr::LogicalPlan::Filter(filter),
                        ),
                    ));
                }
            }
        }
        if let datafusion::logical_expr::LogicalPlan::Extension(extension) = plan {
            if let Some(search) = extension
                .node
                .as_any()
                .downcast_ref::<orchiddb::ir::rel::search::RankedJoin>()
            {
                use datafusion::{common::DFSchema, logical_expr::lit};
                use orchiddb::ir::rel::{dependent::ArgumentBinding, search::SearchMetric};
                use std::sync::Arc;
                // This adapter's indexed function takes an excluded identity so
                // eligibility is enforced before selecting the nearest k rows.
                let excluded = search.predicate.as_ref().and_then(|predicate| {
                    let datafusion::logical_expr::Expr::BinaryExpr(binary) = predicate else {
                        return None;
                    };
                    if binary.op != datafusion::logical_expr::Operator::NotEq {
                        return None;
                    }
                    for (source, target) in
                        [(&binary.left, &binary.right), (&binary.right, &binary.left)]
                    {
                        if physical_field(source, &search.source).as_deref() == Some("id")
                            && physical_field(target, &search.target).as_deref() == Some("id")
                        {
                            return Some(source.as_ref().clone());
                        }
                    }
                    None
                });
                if search.metric() != Some(SearchMetric::Cosine)
                    || search.exact
                    || search.ascending
                    || excluded.is_none()
                {
                    return Err(SqlError::Unsupported(
                        "warehouse indexed search contract".into(),
                    ));
                }
                let output = Arc::new(DFSchema::new_with_metadata(
                    search
                        .schema
                        .iter()
                        .skip(search.source.schema().fields().len())
                        .map(|(q, f)| (q.cloned(), f.clone()))
                        .collect(),
                    Default::default(),
                )?);
                let mut physical = TableFunction::new(
                    vec!["warehouse_neighbors".into()],
                    vec![
                        search.query().clone(),
                        excluded.unwrap(),
                        lit(search.limit as i64),
                    ],
                    Some(search.source.clone()),
                    ArgumentBinding::Correlated,
                    output,
                )?;
                assert_eq!(physical.schema.fields(), search.schema.fields());
                // SELECT exports fields by name; source CTE qualifiers are input
                // scope only. Preserve the ranked node's output qualification.
                physical.schema = search.schema.clone();
                return builtin_lower_relation(&physical.into_plan(), context);
            }
            if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>() {
                let mut physical = function.clone();
                if physical.name != ["expand_numbers"] {
                    return Err(SqlError::Unsupported("warehouse table function".into()));
                }
                physical.name = vec!["warehouse_range".into()];
                return builtin_lower_relation(&physical.into_plan(), context);
            }
        }
        Ok(None)
    }
    fn rewrite_expression(&self, expression: &mut ast::Expr) -> SqlResult<()> {
        if let ast::Expr::Function(function) = expression {
            match function.name.to_string().as_str() {
                "row_number" | "warehouse_json_valid" => {}
                "lower" => {
                    function.name = ast::ObjectName::from(vec![ast::Ident::new("warehouse_lower")])
                }
                other => return Err(SqlError::Unsupported(format!("warehouse function {other}"))),
            }
        }
        Ok(())
    }
}
fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"adapter_test_warehouse","language":"cypher","query":query,
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name","id":"id"}}]})
}
#[tokio::test]
async fn registered_engine_compiles_graph_query_and_runs_its_ast_rewrite() {
    SqlDialect::register(&WAREHOUSE).unwrap();
    let sql: Value = serde_json::from_str(&compile_json(&request("MATCH (p:Person) WHERE p.id > 3 RETURN toLower(p.name) AS name ORDER BY name LIMIT 5").to_string()).await.unwrap()).unwrap();
    assert_eq!(sql["dialect"], "adapter_test_warehouse");
    let sql = sql["sql"].as_str().unwrap();
    assert!(sql.contains("warehouse_lower"), "{sql}");
    assert!(sql.contains("people") && sql.contains("LIMIT 5"), "{sql}");
}
#[tokio::test]
async fn third_engine_lowers_portable_json_through_general_function_hook() {
    SqlDialect::register(&WAREHOUSE).unwrap();
    let output: Value = serde_json::from_str(
        &compile_json(&request("MATCH (p:Person) RETURN json.valid(p.name) AS valid").to_string())
            .await
            .unwrap(),
    )
    .unwrap();
    let sql = output["sql"].as_str().unwrap();
    assert!(sql.contains("warehouse_json_valid"), "{sql}");
    assert!(!sql.contains("__orchiddb_json_"), "{sql}");
}
#[tokio::test]
async fn unknown_functions_do_not_inherit_duckdb_implementations() {
    SqlDialect::register(&WAREHOUSE).unwrap();
    let error = compile_json(&request("MATCH (p:Person) RETURN reverse(p.name)").to_string())
        .await
        .unwrap_err();
    assert!(error.contains("warehouse function reverse"), "{error}");
}
#[test]
fn registration_and_exchange_codecs_are_engine_owned() {
    let dialect = SqlDialect::register(&WAREHOUSE).unwrap();
    assert_eq!(
        SqlDialect::resolve("adapter_test_warehouse").unwrap(),
        dialect
    );
    assert_eq!(
        dialect
            .exchange_literal(ScalarValue::Utf8(Some("it's".into())), DataType::Utf8)
            .unwrap(),
        "CAST('it''s' AS VARCHAR)"
    );
    assert!(
        dialect
            .exchange_literal(ScalarValue::Boolean(Some(true)), DataType::Boolean)
            .is_err()
    );
    assert!(SqlDialect::resolve("not_registered").is_err());
}

fn render(plan: datafusion::logical_expr::LogicalPlan) -> String {
    use orchiddb::ir::{
        policy::ResultForm,
        rel::{LoweredPlan, sql},
    };
    let fields = plan
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    sql::unparse(
        &LoweredPlan {
            plan,
            fields,
            result_form: ResultForm::RowSet,
            islands: Default::default(),
        },
        SqlDialect::register(&WAREHOUSE).unwrap(),
    )
    .unwrap()
}
#[test]
fn adapter_supplies_portable_function_mapping_without_editing_the_function() {
    use datafusion::logical_expr::{LogicalPlanBuilder, lit};
    use orchiddb::ir::functions::logical::LogicalFunction;
    let logical = LogicalFunction::new(
        "portable.abs",
        datafusion::functions::math::abs(),
        Default::default(),
    )
    .into_udf();
    let plan = LogicalPlanBuilder::empty(true)
        .project(vec![logical.call(vec![lit(-7_i64)]).alias("magnitude")])
        .unwrap()
        .build()
        .unwrap();
    let sql = render(plan);
    assert!(sql.contains("ABS(") && sql.contains("-7"), "{sql}");
}
#[test]
fn adapter_rewrites_non_search_table_function_in_code() {
    use arrow::datatypes::{Field, Schema};
    use datafusion::{common::DFSchema, logical_expr::lit};
    use orchiddb::ir::rel::dependent::{ArgumentBinding, TableFunction};
    use std::sync::Arc;
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let function = TableFunction::new(
        vec!["expand_numbers".into()],
        vec![lit(5_i64)],
        None,
        ArgumentBinding::Correlated,
        schema,
    )
    .unwrap();
    let sql = render(function.into_plan());
    assert!(
        sql.contains("warehouse_range") && sql.contains("5"),
        "{sql}"
    );
    assert!(!sql.contains("expand_numbers"), "{sql}");
}
#[test]
fn prepare_time_table_function_uses_the_same_custom_codec() {
    use arrow::datatypes::{Field, Schema};
    use datafusion::{
        common::DFSchema,
        logical_expr::{LogicalPlanBuilder, col, lit},
    };
    use orchiddb::ir::rel::{
        dependent::{ArgumentBinding, TableFunction},
        sql::lowering::{LoweringContext, LoweringMode, RelationLowering},
    };
    use std::sync::Arc;
    let dialect = SqlDialect::register(&WAREHOUSE).unwrap();
    let source = LogicalPlanBuilder::empty(true)
        .project(vec![lit(5_i64).alias("count")])
        .unwrap()
        .build()
        .unwrap();
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let function = TableFunction::new(
        vec!["expand_numbers".into()],
        vec![col("count")],
        Some(Arc::new(source)),
        ArgumentBinding::PrepareTime,
        schema,
    )
    .unwrap();
    let lowered = dialect
        .lower_relation(
            &function.into_plan(),
            &LoweringContext {
                dialect,
                mode: LoweringMode::InIsland,
            },
        )
        .unwrap()
        .unwrap();
    let RelationLowering::Dependent { template, .. } = lowered else {
        panic!("prepare-time function must have a dependent boundary")
    };
    let bound = template.bind(&[ScalarValue::Int64(Some(9))]).unwrap();
    assert!(
        bound.contains("warehouse_range") && bound.contains("CAST(9 AS BIGINT)"),
        "{bound}"
    );
    assert!(!bound.contains("$1"), "{bound}");
}

#[test]
fn adapter_rewrites_ordinary_relational_nodes_before_sql_generation() {
    use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
    let plan = LogicalPlanBuilder::empty(true)
        .project(vec![lit(7_i64).alias("number")])
        .unwrap()
        .filter(col("number").gt(lit(3_i64)))
        .unwrap()
        .build()
        .unwrap();
    let sql = render(plan);
    assert!(sql.contains("3 <"), "{sql}");
    assert!(!sql.contains("> 3"), "{sql}");
}

fn search_request(mixed: bool) -> Value {
    json!({"version":1,"dialect":"adapter_test_warehouse","language":"cypher",
        "query":"MATCH (s:Question {id: 1})-[e:SIMILAR_TO]->(t:Document) RETURN t.id AS id, e.score AS score",
        "execution_engine":"warehouse",
        "engines":{"warehouse":{"dialect":"adapter_test_warehouse"},"seed":{"dialect":"duckdb"}},
        "tables":[
            {"name":"questions","engine":if mixed {"seed"} else {"warehouse"},"columns":[{"name":"id","data_type":"int64"},{"name":"embedding","data_type":"list:float64"}]},
            {"name":"documents","engine":"warehouse","columns":[{"name":"id","data_type":"int64"},{"name":"embedding","data_type":"list:float64"}]}],
        "source_metadata":[{"table":"documents","format":"warehouse_vectors","indexes":[{"column":"embedding","metric":"cosine"}]}],
        "nodes":[{"label":"Question","table":"questions","id":"id","properties":{"id":"id","embedding":"embedding"}},
                  {"label":"Document","table":"documents","id":"id","properties":{"id":"id","embedding":"embedding"}}],
        "computed_relationships":[{"name":"SIMILAR_TO","source":"Question","target":"Document","predicate":"source.id <> target.id",
            "properties":{"score":"vector.cosine_similarity(source.embedding,target.embedding)"},"order_by":[{"expression":"score","direction":"desc"}],"limit_per_source":3}]
    })
}
#[tokio::test]
async fn third_engine_indexed_relationship_compiles_in_its_own_sql_island() {
    SqlDialect::register(&WAREHOUSE).unwrap();
    let plan = orchiddb::compiler::compile(serde_json::from_value(search_request(false)).unwrap())
        .await
        .unwrap();
    assert!(plan.transfers.is_empty(), "{plan:?}");
    assert!(plan.sql.contains("warehouse_neighbors"), "{}", plan.sql);
    assert!(plan.sql.contains("LATERAL"), "{}", plan.sql);
    assert!(
        !plan.sql.contains("lance_") && !plan.sql.contains("<=>"),
        "{}",
        plan.sql
    );
}
fn fixture_batch(
    columns: &[orchiddb::federation::TransferColumn],
) -> arrow::record_batch::RecordBatch {
    use arrow::datatypes::{Field, Schema};
    use std::sync::Arc;
    let values = columns
        .iter()
        .map(|column| match column.data_type.as_str() {
            "int64" => ScalarValue::Int64(Some(1)),
            "string" => ScalarValue::Utf8(Some("1".into())),
            "float64" => ScalarValue::Float64(Some(0.9)),
            "list:float64" => ScalarValue::List(ScalarValue::new_list(
                &[
                    ScalarValue::Float64(Some(1.0)),
                    ScalarValue::Float64(Some(0.0)),
                ],
                &DataType::Float64,
                true,
            )),
            other => panic!("unexpected fixture type {other}"),
        })
        .collect::<Vec<_>>();
    let schema = Arc::new(Schema::new(
        columns
            .iter()
            .zip(&values)
            .map(|(column, value)| Field::new(&column.name, value.data_type(), column.nullable))
            .collect::<Vec<_>>(),
    ));
    arrow::record_batch::RecordBatch::try_new(
        schema,
        values
            .iter()
            .map(|value| value.to_array_of_size(1).unwrap())
            .collect(),
    )
    .unwrap()
}
#[tokio::test]
async fn third_engine_bound_search_executes_on_target_session_and_preserves_schema() {
    use arrow::record_batch::RecordBatch;
    use orchiddb::federation::{self, Session};
    use std::{
        cell::RefCell,
        collections::{BTreeMap, VecDeque},
        rc::Rc,
    };
    SqlDialect::register(&WAREHOUSE).unwrap();
    let plan = orchiddb::compiler::compile(serde_json::from_value(search_request(true)).unwrap())
        .await
        .unwrap();
    let dependent = plan
        .transfers
        .iter()
        .find(|t| t.operation.is_some())
        .expect("mixed ownership must bind search inputs");
    let operation = dependent.operation.as_ref().unwrap();
    assert_eq!(operation.engine, "warehouse");
    assert_eq!(operation.template.dialect, "adapter_test_warehouse");
    assert!(operation.template.sql.contains("warehouse_neighbors"));
    let source_batch = fixture_batch(&operation.input_columns);
    let hits = fixture_batch(&dependent.columns);
    let log = Rc::new(RefCell::new(vec![]));
    struct Mock {
        owner: &'static str,
        dialect: &'static str,
        responses: VecDeque<Vec<RecordBatch>>,
        log: Rc<RefCell<Vec<(String, String)>>>,
    }
    #[async_trait::async_trait(?Send)]
    impl Session for Mock {
        fn dialect(&self) -> &str {
            self.dialect
        }
        async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String> {
            self.log.borrow_mut().push((self.owner.into(), sql.into()));
            self.responses
                .pop_front()
                .ok_or_else(|| format!("unexpected {} query: {sql}", self.owner))
        }
    }
    // Each transfer's source query executes before its bound operation. Earlier
    // exchanges, if present, are given batches with precisely their own schema.
    let mut seed = VecDeque::new();
    let mut warehouse = VecDeque::new();
    for transfer in &plan.transfers {
        let batch = if transfer.operation.is_some() {
            source_batch.clone()
        } else {
            fixture_batch(&transfer.columns)
        };
        if transfer.source_engine == "seed" {
            seed.push_back(vec![batch]);
        } else {
            warehouse.push_back(vec![batch]);
        }
        if transfer.operation.is_some() {
            warehouse.push_back(vec![hits.clone()]);
        }
    }
    let final_rows = fixture_batch(&[
        orchiddb::federation::TransferColumn {
            name: "id".into(),
            data_type: "int64".into(),
            nullable: false,
        },
        orchiddb::federation::TransferColumn {
            name: "score".into(),
            data_type: "float64".into(),
            nullable: true,
        },
    ]);
    warehouse.push_back(vec![final_rows.clone()]);
    let mut sessions: BTreeMap<String, Box<dyn Session>> = BTreeMap::new();
    sessions.insert(
        "seed".into(),
        Box::new(Mock {
            owner: "seed",
            dialect: "duckdb",
            responses: seed,
            log: log.clone(),
        }),
    );
    sessions.insert(
        "warehouse".into(),
        Box::new(Mock {
            owner: "warehouse",
            dialect: "adapter_test_warehouse",
            responses: warehouse,
            log: log.clone(),
        }),
    );
    let result = federation::execute(&plan, &mut sessions).await.unwrap();
    assert_eq!(result[0].schema(), final_rows.schema());
    assert_eq!(plan.fields, vec!["id", "score"]);
    let log = log.borrow();
    let searches = log
        .iter()
        .filter(|(_, sql)| sql.contains("warehouse_neighbors"))
        .collect::<Vec<_>>();
    assert_eq!(searches.len(), 1, "{log:?}");
    assert_eq!(searches[0].0, "warehouse");
    assert!(
        searches[0].1.contains("ARRAY[") && !searches[0].1.contains("$1"),
        "{log:?}"
    );
}

#[test]
fn correlated_table_function_preserves_qualified_source_columns_in_parent() {
    use arrow::datatypes::{Field, Schema};
    use datafusion::{
        common::DFSchema,
        logical_expr::{LogicalPlanBuilder, col, lit},
    };
    use orchiddb::ir::rel::dependent::{ArgumentBinding, TableFunction};
    use std::sync::Arc;
    let source = LogicalPlanBuilder::empty(true)
        .project(vec![lit(5_i64).alias("n")])
        .unwrap()
        .alias("numbers")
        .unwrap()
        .build()
        .unwrap();
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let function = TableFunction::new(
        vec!["expand_numbers".into()],
        vec![col("numbers.n")],
        Some(Arc::new(source)),
        ArgumentBinding::Correlated,
        schema,
    )
    .unwrap();
    let plan = LogicalPlanBuilder::from(function.into_plan())
        .project(vec![col("numbers.n"), col("value")])
        .unwrap()
        .build()
        .unwrap();
    let sql = render(plan);
    // The source alias lives inside the lowered table function; the parent's
    // output columns must refer to the derived relation that actually exposes them.
    assert!(!sql.starts_with("SELECT numbers.n"), "{sql}");
}

// An extension defined entirely by the third-engine package, unknown to core.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RemoteRows {
    target: std::sync::Arc<datafusion::logical_expr::LogicalPlan>,
}
impl PartialOrd for RemoteRows {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl datafusion::logical_expr::UserDefinedLogicalNodeCore for RemoteRows {
    fn name(&self) -> &str {
        "WarehouseRemoteRows"
    }
    fn inputs(&self) -> Vec<&datafusion::logical_expr::LogicalPlan> {
        vec![self.target.as_ref()]
    }
    fn schema(&self) -> &datafusion::common::DFSchemaRef {
        self.target.schema()
    }
    fn expressions(&self) -> Vec<datafusion::logical_expr::Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "WarehouseRemoteRows")
    }
    fn with_exprs_and_inputs(
        &self,
        exprs: Vec<datafusion::logical_expr::Expr>,
        inputs: Vec<datafusion::logical_expr::LogicalPlan>,
    ) -> datafusion::common::Result<Self> {
        if !exprs.is_empty() || inputs.len() != 1 {
            return Err(datafusion::common::DataFusionError::Plan(
                "remote rows inputs".into(),
            ));
        }
        Ok(Self {
            target: std::sync::Arc::new(inputs.into_iter().next().unwrap()),
        })
    }
}
#[test]
fn placement_discovers_target_adapter_when_execution_engine_differs() {
    use datafusion::logical_expr::{Extension, LogicalPlan, LogicalPlanBuilder, lit};
    use orchiddb::ir::rel::sql::lowering::placement_for;
    use std::sync::Arc;
    SqlDialect::register(&WAREHOUSE).unwrap();
    let target = Arc::new(
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("id")])
            .unwrap()
            .build()
            .unwrap(),
    );
    let plan = LogicalPlan::Extension(Extension {
        node: Arc::new(RemoteRows {
            target: target.clone(),
        }),
    });
    let placement = placement_for(&plan, SqlDialect::DuckDb)
        .unwrap()
        .expect("third adapter must describe its own node despite different coordinator");
    assert_eq!(placement.target, Some(target));
    assert!(placement.source.is_none());
}

// Trace projected aliases to the physical column recognized by this adapter's
// indexed function. Key aliases and property aliases can name the same column.
fn physical_field(
    expr: &datafusion::logical_expr::Expr,
    plan: &datafusion::logical_expr::LogicalPlan,
) -> Option<String> {
    use datafusion::logical_expr::{Expr, LogicalPlan};
    let expr = expr.clone().unalias();
    let Expr::Column(column) = &expr else {
        return None;
    };
    let index = plan.schema().index_of_column(column).ok()?;
    match plan {
        LogicalPlan::Projection(projection) => {
            physical_field(&projection.expr[index], &projection.input)
        }
        LogicalPlan::TableScan(scan) => Some(scan.projected_schema.field(index).name().clone()),
        _ => plan.inputs().iter().find_map(|input| {
            let name = plan.schema().field(index).name();
            let column = input
                .schema()
                .columns()
                .into_iter()
                .find(|c| &c.name == name)?;
            physical_field(&Expr::Column(column), input)
        }),
    }
}

#[tokio::test]
async fn third_engine_can_bind_declared_native_functions() {
    SqlDialect::register(&WAREHOUSE).unwrap();
    let mut r = request("MATCH (p:Person) RETURN warehouseScore(p.id) AS score");
    r["functions"] = json!([{
        "name": "warehouseScore", "target": "warehouse_score",
        "parameters": ["int64"], "returns": "int64"
    }]);
    let result: Value = serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap();
    let sql = result["sql"].as_str().unwrap();
    assert!(sql.contains("warehouse_score"), "{sql}");
    assert!(!sql.contains("__engine_function_"), "{sql}");
}
