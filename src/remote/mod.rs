//! Optional HTTP engines. Logical expressions remain engine independent; this
//! module produces typed prepared requests instead of inventing a SQL dialect.
pub mod transport;
#[cfg(feature = "weaviate")]
mod weaviate;
#[cfg(feature = "elasticsearch")]
pub use transport::ElasticsearchSession;
#[cfg(feature = "quickwit")]
pub use transport::QuickwitSession;
#[cfg(feature = "weaviate")]
pub use transport::WeaviateSession;
pub use transport::{Authentication, HttpOptions, HttpSession};

use crate::ir::rel::{
    dependent::TableFunction,
    search::{RankedJoin, SCORE_COLUMN, SearchMetric},
};
use crate::operations::{PreparedOperation, RequestAdapter, RequestBinding, RequestTemplate};
use datafusion::common::{Column, ScalarValue};
use datafusion::logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder, Operator};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

type Result<T> = std::result::Result<T, String>;

#[cfg(feature = "quickwit")]
#[derive(Debug, Default)]
pub struct QuickwitAdapter;
#[cfg(feature = "elasticsearch")]
#[derive(Debug, Default)]
pub struct ElasticsearchAdapter;

#[cfg(feature = "weaviate")]
#[derive(Debug, Default)]
pub struct WeaviateAdapter;

pub fn adapters() -> Vec<Arc<dyn RequestAdapter>> {
    #[allow(unused_mut)]
    let mut result: Vec<Arc<dyn RequestAdapter>> = Vec::new();
    #[cfg(feature = "quickwit")]
    result.push(Arc::new(QuickwitAdapter));
    #[cfg(feature = "elasticsearch")]
    result.push(Arc::new(ElasticsearchAdapter));
    #[cfg(feature = "weaviate")]
    result.push(Arc::new(WeaviateAdapter));
    result
}
macro_rules! adapter {
    ($ty:ty, $name:literal) => {
        impl RequestAdapter for $ty {
            fn name(&self) -> &str {
                $name
            }
            fn owner(&self, plan: &LogicalPlan) -> Result<Option<String>> {
                if let LogicalPlan::Extension(extension) = plan {
                    if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>()
                    {
                        if function.name.len() == 2 && function.name[0] == $name {
                            return Ok(Some(if function.arguments.len() == 3 {
                                literal(&function.arguments[0])?
                                    .as_str()
                                    .ok_or("remote engine must be a literal string")?
                                    .to_owned()
                            } else {
                                $name.to_owned()
                            }));
                        }
                    }
                }
                let Some(search) = ranked(plan) else {
                    return Ok(None);
                };
                let Some(metadata) = &search.source_metadata else {
                    return Ok(None);
                };
                Ok(search
                    .index
                    .as_ref()
                    .and_then(|i| i.options.get("engine"))
                    .or_else(|| metadata.options.get("engine"))
                    .and_then(Value::as_str)
                    .map(str::to_owned))
            }
            fn lower(&self, plan: &LogicalPlan) -> Result<Option<PreparedOperation>> {
                if let Some(search) = ranked(plan) {
                    return lower_ranked($name, search).map(Some);
                }
                if let LogicalPlan::Extension(extension) = plan {
                    if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>()
                    {
                        return lower_function($name, function);
                    }
                }
                match lower_scan($name, plan) {
                    Err(_) if !matches!(plan, LogicalPlan::TableScan(_)) => Ok(None),
                    result => result,
                }
            }
        }
    };
}
#[cfg(feature = "quickwit")]
adapter!(QuickwitAdapter, "quickwit");
#[cfg(feature = "elasticsearch")]
adapter!(ElasticsearchAdapter, "elasticsearch");

#[cfg(feature = "weaviate")]
impl RequestAdapter for WeaviateAdapter {
    fn name(&self) -> &str {
        "weaviate"
    }
    fn owner(&self, plan: &LogicalPlan) -> Result<Option<String>> {
        Ok(ranked(plan)
            .and_then(|search| {
                search
                    .index
                    .as_ref()
                    .and_then(|index| index.options.get("engine"))
                    .or_else(|| {
                        search
                            .source_metadata
                            .as_ref()
                            .and_then(|metadata| metadata.options.get("engine"))
                    })
            })
            .and_then(Value::as_str)
            .map(str::to_owned))
    }
    fn lower(&self, plan: &LogicalPlan) -> Result<Option<PreparedOperation>> {
        ranked(plan)
            .map(|search| lower_ranked("weaviate", search))
            .transpose()
    }
}

fn ranked(plan: &LogicalPlan) -> Option<&RankedJoin> {
    let LogicalPlan::Extension(e) = plan else {
        return None;
    };
    e.node.as_any().downcast_ref()
}

/// A projection's lineage is resolved to physical fields, never inferred from
/// generated graph column names. Every filter below ranking stays below it.
#[derive(Clone)]
struct Scan {
    table: String,
    columns: BTreeMap<Column, Expr>,
    filters: Vec<Expr>,
    order: Vec<(Expr, bool, bool)>,
    limit: Option<u64>,
    offset: u64,
}
fn resolve(expr: &Expr, scan: &Scan) -> Result<Expr> {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    expr.clone()
        .transform_up(|e| {
            if let Expr::Column(c) = &e {
                let v = scan
                    .columns
                    .get(c)
                    .or_else(|| {
                        let mut matches = scan.columns.iter().filter(|(k, _)| k.name == c.name);
                        let first = matches.next();
                        if matches.next().is_none() {
                            first.map(|(_, v)| v)
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| {
                        datafusion::common::DataFusionError::Plan(format!(
                            "unresolved remote column {c}"
                        ))
                    })?;
                return Ok(Transformed::yes(v.clone()));
            }
            Ok(Transformed::no(e))
        })
        .map(|t| t.data)
        .map_err(|e| e.to_string())
}
fn scan(plan: &LogicalPlan) -> Result<Option<Scan>> {
    let result = match plan {
        LogicalPlan::TableScan(t) => {
            let columns = t
                .projected_schema
                .columns()
                .into_iter()
                .map(|c| {
                    let value = Expr::Column(Column::new_unqualified(c.name.clone()));
                    (c, value)
                })
                .collect();
            Scan {
                table: t.table_name.to_string(),
                columns,
                filters: t.filters.clone(),
                order: vec![],
                limit: t.fetch.map(|v| v as u64),
                offset: 0,
            }
        }
        LogicalPlan::Projection(p) => {
            let Some(mut s) = scan(&p.input)? else {
                return Ok(None);
            };
            s.columns = p
                .schema
                .columns()
                .into_iter()
                .zip(
                    p.expr
                        .iter()
                        .map(|e| resolve(&e.clone().unalias(), &s))
                        .collect::<Result<Vec<_>>>()?,
                )
                .collect();
            s
        }
        LogicalPlan::SubqueryAlias(a) => {
            let Some(mut s) = scan(&a.input)? else {
                return Ok(None);
            };
            s.columns = a
                .schema
                .columns()
                .into_iter()
                .zip(
                    a.input
                        .schema()
                        .columns()
                        .iter()
                        .map(|c| resolve(&Expr::Column(c.clone()), &s))
                        .collect::<Result<Vec<_>>>()?,
                )
                .collect();
            s
        }
        LogicalPlan::Filter(f) => {
            let Some(mut s) = scan(&f.input)? else {
                return Ok(None);
            };
            if s.limit.is_some() || s.offset != 0 {
                return Ok(None);
            }
            s.filters.push(resolve(&f.predicate, &s)?);
            s
        }
        LogicalPlan::Limit(l) => {
            let Some(mut s) = scan(&l.input)? else {
                return Ok(None);
            };
            if s.limit.is_some() || s.offset != 0 {
                return Ok(None);
            }
            s.offset = l
                .skip
                .as_deref()
                .map(unsigned_literal)
                .transpose()?
                .unwrap_or(0);
            s.limit = l.fetch.as_deref().map(unsigned_literal).transpose()?;
            s
        }
        LogicalPlan::Sort(sorting) => {
            let Some(mut s) = scan(&sorting.input)? else {
                return Ok(None);
            };
            if s.limit.is_some() || s.offset != 0 {
                return Ok(None);
            }
            s.order = sorting
                .expr
                .iter()
                .map(|o| Ok((resolve(&o.expr, &s)?, o.asc, o.nulls_first)))
                .collect::<Result<_>>()?;
            s.limit = sorting.fetch.map(|n| n as u64);
            s
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
fn unsigned_literal(expr: &Expr) -> Result<u64> {
    literal(expr)?
        .as_u64()
        .ok_or_else(|| "remote limit/offset must be a nonnegative integer literal".into())
}
fn scalar(value: &ScalarValue) -> Result<Value> {
    crate::operations::parameter_json(value)
}
fn literal(expr: &Expr) -> Result<Value> {
    match expr {
        Expr::Literal(v, _) => scalar(v),
        Expr::Alias(a) => literal(&a.expr),
        _ => Err(format!("expected a literal, got {expr}")),
    }
}
fn field(expr: &Expr) -> Result<String> {
    match expr {
        Expr::Column(c) => Ok(c.name.clone()),
        Expr::Alias(a) => field(&a.expr),
        _ => Err(format!(
            "remote index requires a physical field, got {expr}"
        )),
    }
}

struct Build {
    bindings: Vec<RequestBinding>,
    guards: Vec<Value>,
    boolean_guards: Vec<Value>,
    requirements: Vec<Value>,
    source: Option<Arc<LogicalPlan>>,
    arguments: Vec<Expr>,
}
impl Build {
    fn new(source: Option<Arc<LogicalPlan>>) -> Self {
        Self {
            bindings: vec![],
            guards: vec![],
            boolean_guards: vec![],
            requirements: vec![],
            source,
            arguments: vec![],
        }
    }
    fn argument(&mut self, expr: &Expr) -> Result<usize> {
        if self.source.is_none() {
            self.source = Some(Arc::new(
                LogicalPlanBuilder::empty(true)
                    .build()
                    .map_err(|e| e.to_string())?,
            ));
        }
        let source = self.source.as_ref().unwrap();
        if let Expr::Column(c) = expr {
            if let Ok(i) = source.schema().index_of_column(c) {
                return Ok(i);
            }
        }
        if let Some(i) = self.arguments.iter().position(|e| e == expr) {
            return Ok(source.schema().fields().len() + i);
        }
        if expr
            .column_refs()
            .iter()
            .any(|c| !source.schema().has_column(c))
        {
            return Err(format!("remote parameter {expr} is not source-only"));
        }
        let i = source.schema().fields().len() + self.arguments.len();
        self.arguments.push(expr.clone());
        Ok(i)
    }
    fn value(&mut self, expr: &Expr, pointer: String) -> Result<Value> {
        if let Ok(value) = literal(expr) {
            return Ok(value);
        }
        let parameter = self.argument(expr)?;
        self.bindings.push(RequestBinding {
            pointer,
            parameter,
            encoding: Default::default(),
        });
        Ok(Value::Null)
    }
    fn requirement(&mut self, field: &str, usage: &str) {
        let requirement = json!({"field":field,"usage":usage});
        if !self.requirements.contains(&requirement) {
            self.requirements.push(requirement)
        }
    }
    fn finish(
        mut self,
        engine: &str,
        mut request: Value,
        schema: datafusion::common::DFSchemaRef,
    ) -> Result<PreparedOperation> {
        let source = if let Some(source) = self.source {
            if self.arguments.is_empty() {
                Some(source)
            } else {
                let mut projection = source
                    .schema()
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>();
                projection.extend(
                    self.arguments
                        .into_iter()
                        .enumerate()
                        .map(|(i, e)| e.alias(format!("__remote_argument_{i}"))),
                );
                Some(Arc::new(
                    LogicalPlanBuilder::from(source.as_ref().clone())
                        .project(projection)
                        .map_err(|e| e.to_string())?
                        .build()
                        .map_err(|e| e.to_string())?,
                ))
            }
        } else {
            None
        };
        let parameters = source.as_ref().map_or(0, |s| s.schema().fields().len());
        request["parameters"] = json!(vec![Value::Null; parameters]);
        request["parameter_nulls"] = json!(vec![Value::Null; parameters]);
        for parameter in 0..parameters {
            self.bindings.push(RequestBinding {
                pointer: format!("/parameters/{parameter}"),
                parameter,
                encoding: Default::default(),
            });
            self.bindings.push(RequestBinding {
                pointer: format!("/parameter_nulls/{parameter}"),
                parameter,
                encoding: crate::operations::RequestEncoding::SqlNull,
            });
        }
        request["null_guards"] = json!(self.guards);
        request["boolean_guards"] = json!(self.boolean_guards);
        request["requirements"] = json!(self.requirements);
        Ok(PreparedOperation {
            source,
            template: RequestTemplate {
                adapter: engine.into(),
                parameters,
                request,
                bindings: self.bindings,
            },
            schema,
            replacement: None,
        })
    }
}
fn escape_pointer(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
fn and(parts: Vec<Value>) -> Value {
    if parts.is_empty() {
        json!({"match_all":{}})
    } else {
        json!({"bool":{"filter":parts}})
    }
}

/// Compile the rows where the predicate is true, or false. SQL UNKNOWN belongs
/// to neither set. This distinction is essential for NOT, !=, and NULL fields.
fn predicate(
    expr: &Expr,
    truth: bool,
    pointer: &str,
    build: &mut Build,
    source_schema: Option<&datafusion::common::DFSchema>,
) -> Result<Value> {
    if source_schema.is_some_and(|schema| {
        !expr.column_refs().is_empty() && expr.column_refs().iter().all(|c| schema.has_column(c))
    }) {
        let parameter = build.argument(expr)?;
        build
            .boolean_guards
            .push(json!({"parameter":parameter,"clause":pointer,"truth":truth}));
        return Ok(json!({"match_all":{}}));
    }
    match expr {
        Expr::Between(between) => {
            let conjunction = between
                .expr
                .as_ref()
                .clone()
                .gt_eq(between.low.as_ref().clone())
                .and(
                    between
                        .expr
                        .as_ref()
                        .clone()
                        .lt_eq(between.high.as_ref().clone()),
                );
            predicate(
                &conjunction,
                truth != between.negated,
                pointer,
                build,
                source_schema,
            )
        }
        Expr::InList(list) => {
            let positive = truth != list.negated;
            let key = if positive { "should" } else { "filter" };
            let mut parts = Vec::new();
            for (i, value) in list.list.iter().enumerate() {
                parts.push(predicate(
                    &list.expr.as_ref().clone().eq(value.clone()),
                    positive,
                    &format!("{pointer}/bool/{key}/{i}"),
                    build,
                    source_schema,
                )?);
            }
            if parts.is_empty() {
                return Ok(if positive {
                    json!({"match_none":{}})
                } else {
                    json!({"match_all":{}})
                });
            }
            Ok(if positive {
                json!({"bool":{"should":parts,"minimum_should_match":1}})
            } else {
                json!({"bool":{"filter":parts}})
            })
        }
        Expr::Column(_) => predicate(
            &expr
                .clone()
                .eq(Expr::Literal(ScalarValue::Boolean(Some(true)), None)),
            truth,
            pointer,
            build,
            source_schema,
        ),
        Expr::Alias(a) => predicate(&a.expr, truth, pointer, build, source_schema),
        Expr::Not(inner) => predicate(inner, !truth, pointer, build, source_schema),
        Expr::Literal(ScalarValue::Boolean(value), _) => Ok(if *value == Some(truth) {
            json!({"match_all":{}})
        } else {
            json!({"match_none":{}})
        }),
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => {
            let name = field(inner)?;
            build.requirement(&name, "exists");
            let exists = json!({"exists":{"field":name}});
            let positive = matches!(expr, Expr::IsNotNull(_)) == truth;
            Ok(if positive {
                exists
            } else {
                json!({"bool":{"must_not":[exists]}})
            })
        }
        Expr::BinaryExpr(binary) if matches!(binary.op, Operator::And | Operator::Or) => {
            let conjunction = (binary.op == Operator::And) == truth;
            let key = if conjunction { "filter" } else { "should" };
            let left = predicate(
                &binary.left,
                truth,
                &format!("{pointer}/bool/{key}/0"),
                build,
                source_schema,
            )?;
            let right = predicate(
                &binary.right,
                truth,
                &format!("{pointer}/bool/{key}/1"),
                build,
                source_schema,
            )?;
            Ok(if conjunction {
                json!({"bool":{"filter":[left,right]}})
            } else {
                json!({"bool":{"should":[left,right],"minimum_should_match":1}})
            })
        }
        Expr::BinaryExpr(binary) => {
            let mut op = binary.op;
            let target = |e: &Expr| matches!(e,Expr::Column(c) if !source_schema.is_some_and(|s|s.has_column(c)));
            let (column, value) = if target(&binary.left) {
                (&*binary.left, &*binary.right)
            } else if target(&binary.right) {
                op = match op {
                    Operator::Lt => Operator::Gt,
                    Operator::LtEq => Operator::GtEq,
                    Operator::Gt => Operator::Lt,
                    Operator::GtEq => Operator::LtEq,
                    other => other,
                };
                (&*binary.right, &*binary.left)
            } else {
                return Err(format!("remote filter requires one indexed field: {expr}"));
            };
            let name = field(column)?;
            let comparison = if truth {
                op
            } else {
                match op {
                    Operator::Eq => Operator::NotEq,
                    Operator::NotEq => Operator::Eq,
                    Operator::Lt => Operator::GtEq,
                    Operator::LtEq => Operator::Gt,
                    Operator::Gt => Operator::LtEq,
                    Operator::GtEq => Operator::Lt,
                    _ => return Err(format!("unsupported remote predicate {expr}")),
                }
            };
            let escaped = escape_pointer(&name);
            let value_pointer = match comparison {
                Operator::Eq => format!("{pointer}/term/{escaped}"),
                Operator::NotEq => format!("{pointer}/bool/must_not/0/term/{escaped}"),
                Operator::Lt | Operator::LtEq | Operator::Gt | Operator::GtEq => format!(
                    "{pointer}/range/{escaped}/{}",
                    match comparison {
                        Operator::Lt => "lt",
                        Operator::LtEq => "lte",
                        Operator::Gt => "gt",
                        _ => "gte",
                    }
                ),
                _ => return Err(format!("unsupported remote predicate {expr}")),
            };
            let bound = build.value(value, value_pointer.clone())?;
            let is_literal = literal(value).is_ok();
            if is_literal && bound.is_null() {
                return Ok(json!({"match_none":{}}));
            }
            if !is_literal {
                build
                    .guards
                    .push(json!({"value":value_pointer,"clause":pointer}));
            }
            build.requirement(
                &name,
                if matches!(comparison, Operator::Eq | Operator::NotEq) {
                    "exact"
                } else {
                    "range"
                },
            );
            Ok(match comparison {
                Operator::Eq => json!({"term":{name:bound}}),
                Operator::NotEq => {
                    json!({"bool":{"filter":[{"exists":{"field":name}}],"must_not":[{"term":{name:bound}}]}})
                }
                other => {
                    json!({"range":{name:{match other{Operator::Lt=>"lt",Operator::LtEq=>"lte",Operator::Gt=>"gt",_=>"gte"}:bound}}})
                }
            })
        }
        _ => Err(format!(
            "remote index cannot preserve predicate semantics for {expr}"
        )),
    }
}
fn output(expr: &Expr, name: &str) -> Result<Value> {
    match expr {
        Expr::Column(c) => {
            Ok(json!({"name":name,"source":"field","path":c.name.split('.').collect::<Vec<_>>()}))
        }
        Expr::Literal(_, _) => Ok(json!({"name":name,"source":"literal","value":literal(expr)?})),
        Expr::Alias(a) => output(&a.expr, name),
        _ => Err(format!(
            "remote projection {expr} must remain outside the request operation"
        )),
    }
}

fn lower_ranked(engine: &str, search: &RankedJoin) -> Result<PreparedOperation> {
    if engine != "weaviate" && search.metric() != Some(SearchMetric::Bm25) {
        return Err(format!(
            "{engine} indexed relationships currently require text.bm25"
        ));
    }
    if engine != "weaviate" && (search.ascending || search.nulls_first) {
        return Err("BM25 retrieval requires descending score and nulls last".into());
    }
    #[cfg(feature = "weaviate")]
    if engine == "weaviate" {
        weaviate::validate_search(search)?;
    }
    let mut target =
        scan(&search.target)?.ok_or("remote BM25 target must resolve to a single index scan")?;
    if target.limit.is_some() || target.offset != 0 || !target.order.is_empty() {
        return Err("remote BM25 cannot move retrieval across target limit or ordering".into());
    }
    let metadata = search
        .source_metadata
        .as_ref()
        .ok_or("remote BM25 requires source metadata")?;
    let option = |name: &str| {
        search
            .index
            .as_ref()
            .and_then(|i| i.options.get(name))
            .or_else(|| metadata.options.get(name))
    };
    let index = option("collection")
        .or_else(|| option("index"))
        .and_then(Value::as_str)
        .unwrap_or(&target.table)
        .to_owned();
    let external = option("engine").is_some();
    let mut fields = BTreeMap::new();
    if let Some(mapping) = option("field_mapping") {
        for (name, value) in mapping
            .as_object()
            .ok_or("field_mapping must be an object")?
        {
            fields.insert(
                name.clone(),
                value
                    .as_str()
                    .ok_or("field_mapping values must be field names")?
                    .to_owned(),
            );
        }
    }
    if let Some(name) = option("key_field").and_then(Value::as_str) {
        if search.target_keys.len() != 1 {
            return Err(
                "key_field requires a single target key; use field_mapping for composite keys"
                    .into(),
            );
        }
        fields.insert(
            field(&resolve(&search.target_keys[0], &target)?)?,
            name.to_owned(),
        );
    }
    if let Some(name) = option(if engine == "weaviate" {
        "vector_field"
    } else {
        "text_field"
    })
    .and_then(Value::as_str)
    {
        fields.insert(
            field(&resolve(search.document(), &target)?)?,
            name.to_owned(),
        );
    }
    if !fields.is_empty() {
        use datafusion::common::tree_node::{Transformed, TreeNode};
        let remap = |expr: Expr| {
            expr.transform_up(|e| {
                if let Expr::Column(c) = &e {
                    if let Some(name) = fields.get(&c.name) {
                        return Ok(Transformed::yes(Expr::Column(Column::new_unqualified(
                            name,
                        ))));
                    }
                }
                Ok(Transformed::no(e))
            })
            .map(|t| t.data)
            .map_err(|e: datafusion::common::DataFusionError| e.to_string())
        };
        target.columns = target
            .columns
            .into_iter()
            .map(|(c, e)| Ok((c, remap(e)?)))
            .collect::<Result<_>>()?;
        target.filters = target
            .filters
            .into_iter()
            .map(remap)
            .collect::<Result<_>>()?;
    }
    let text = field(&resolve(search.document(), &target)?)?;
    let source_predicates = search
        .predicate
        .as_ref()
        .into_iter()
        .flat_map(datafusion::logical_expr::utils::split_conjunction)
        .filter(|e| {
            e.column_refs()
                .iter()
                .all(|c| search.source.schema().has_column(c))
        })
        .cloned()
        .collect::<Vec<_>>();
    let source =
        if let Some(predicate) = datafusion::logical_expr::utils::conjunction(source_predicates) {
            Arc::new(
                LogicalPlanBuilder::from(search.source.as_ref().clone())
                    .filter(predicate)
                    .map_err(|e| e.to_string())?
                    .build()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            search.source.clone()
        };
    let mut build = Build::new(Some(source));
    build.requirement(
        &text,
        if engine == "weaviate" {
            "vector"
        } else {
            "match"
        },
    );
    let query_pointer = if engine == "weaviate" {
        "/body/vector".to_owned()
    } else {
        format!(
            "/body/query/bool/must/0/match/{}/query",
            escape_pointer(&text)
        )
    };
    let query = build.value(search.query(), query_pointer.clone())?;
    let mut filters = target.filters.clone();
    if let Some(p) = &search.predicate {
        for clause in datafusion::logical_expr::utils::split_conjunction(p) {
            if clause
                .column_refs()
                .iter()
                .all(|c| search.source.schema().has_column(c))
            {
                continue;
            }
            // Resolve only target references; source references retain their schema identity.
            use datafusion::common::tree_node::{Transformed, TreeNode};
            let resolved = clause
                .clone()
                .transform_up(|e| {
                    if let Expr::Column(c) = &e {
                        if search.target.schema().has_column(c) {
                            return resolve(&e, &target)
                                .map(Transformed::yes)
                                .map_err(datafusion::common::DataFusionError::Plan);
                        }
                    }
                    Ok(Transformed::no(e))
                })
                .map_err(|e| e.to_string())?
                .data;
            filters.push(resolved);
        }
    }
    #[cfg(feature = "weaviate")]
    let score_filters = if engine == "weaviate" {
        weaviate::score_filters(&mut filters, search, &target, &mut build)?
    } else {
        vec![]
    };
    let filters = filters
        .iter()
        .enumerate()
        .map(|(i, e)| {
            predicate(
                e,
                true,
                &format!("/body/query/bool/filter/{i}"),
                &mut build,
                Some(search.source.schema()),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let mut sort = vec![json!({"_score":"desc"})];
    if search.exact {
        for key in &search.target_keys {
            let name = field(&resolve(key, &target)?)?;
            build.requirement(&name, "sort");
            sort.push(json!({name:{"order":"asc","missing":"_last"}}));
        }
        if engine == "quickwit" && sort.len() > 2 {
            return Err("Quickwit supports at most two sort criteria; exact BM25 requires a single target key".into());
        }
    }
    let mut columns = Vec::new();
    for (i, f) in search.source.schema().fields().iter().enumerate() {
        columns.push(json!({"name":f.name(),"source":"parameter","parameter":i}));
    }
    let mut replacement = None;
    let schema = if external {
        use arrow::datatypes::{Field, Schema};
        use datafusion::{
            common::DFSchema,
            logical_expr::{ExprSchemable, JoinType},
        };
        let mut fields = search
            .source
            .schema()
            .fields()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let mut left_keys = Vec::new();
        let mut right_keys = Vec::new();
        for (i, key) in search.target_keys.iter().enumerate() {
            let name = format!("__remote_target_key_{i}");
            let physical = resolve(key, &target)?;
            build.requirement(&field(&physical)?, "stored");
            columns.push(output(&physical, &name)?);
            fields.push(Arc::new(Field::new(
                &name,
                key.get_type(search.target.schema())
                    .map_err(|e| e.to_string())?,
                false,
            )));
            left_keys.push(Column::new_unqualified(name));
            let Expr::Column(key) = key else {
                return Err("external index target keys must be columns".into());
            };
            right_keys.push(key.clone());
        }
        let score_type = search
            .schema
            .field_with_unqualified_name(SCORE_COLUMN)
            .map_err(|e| e.to_string())?
            .data_type()
            .clone();
        fields.push(Arc::new(Field::new(SCORE_COLUMN, score_type, true)));
        let schema = Arc::new(DFSchema::try_from(Schema::new(fields)).map_err(|e| e.to_string())?);
        let result = crate::operations::RequestResult {
            schema: schema.clone(),
        }
        .into_plan();
        let mut joined = LogicalPlanBuilder::from(result)
            .join(
                search.target.as_ref().clone(),
                JoinType::Inner,
                (left_keys, right_keys),
                None,
            )
            .map_err(|e| e.to_string())?;
        if engine == "weaviate" {
            use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
            let mut filters = Vec::new();
            for predicate in search
                .predicate
                .as_ref()
                .into_iter()
                .flat_map(datafusion::logical_expr::utils::split_conjunction)
            {
                let mut scoring = false;
                predicate
                    .apply(|expression| {
                        if let Expr::ScalarFunction(function) = expression {
                            scoring |= crate::ir::functions::logical::definition(&function.func)
                                .is_some_and(|function| function.search_metric.is_some());
                        }
                        Ok(TreeNodeRecursion::Continue)
                    })
                    .map_err(|error| error.to_string())?;
                if !scoring {
                    filters.push(predicate.clone());
                }
            }
            if let Some(predicate) = datafusion::logical_expr::utils::conjunction(filters) {
                joined = joined
                    .filter(predicate)
                    .map_err(|error| error.to_string())?;
            }
        }
        let joined = joined
            .project(search.schema.columns().into_iter().map(Expr::Column))
            .map_err(|e| e.to_string())?
            .build()
            .map_err(|e| e.to_string())?;
        replacement = Some(joined);
        schema
    } else {
        for c in search.target.schema().columns() {
            let expression = resolve(&Expr::Column(c.clone()), &target)?;
            if let Expr::Column(field) = &expression {
                build.requirement(&field.name, "stored");
            }
            columns.push(output(&expression, &c.name)?);
        }
        search.schema.clone()
    };
    columns.push(json!({"name":SCORE_COLUMN,"source":"score"}));
    let operator = if engine == "quickwit" { "OR" } else { "or" };
    let request = json!({"version":1,"engine":engine,"operation":"search","api":"elastic","index":index,"body":{"query":{"bool":{"must":[{"match":{text:{"query":query,"operator":operator}}}],"filter":filters}},"sort":sort,"size":search.limit},"columns":columns,"limit":search.limit,"null_query_paths":[query_pointer]});
    #[cfg(feature = "weaviate")]
    let request = if engine == "weaviate" {
        json!({"version":1,"engine":engine,"operation":"search","api":"weaviate","index":index,
            "body":{"vector":query,"query":{"bool":{"filter":filters}},"score_filters":score_filters},
            "metric":search.metric(),"vector_name":option("target_vector"),"tenant":option("tenant"),
            "columns":columns,"limit":search.limit,"null_query_paths":[query_pointer]})
    } else {
        request
    };
    let mut operation = build.finish(engine, request, schema)?;
    operation.replacement = replacement;
    Ok(operation)
}

fn lower_scan(engine: &str, plan: &LogicalPlan) -> Result<Option<PreparedOperation>> {
    let Some(s) = scan(plan)? else {
        return Ok(None);
    };
    let mut build = Build::new(None);
    let filters = s
        .filters
        .iter()
        .enumerate()
        .map(|(i, e)| {
            predicate(
                e,
                true,
                &format!("/body/query/bool/filter/{i}"),
                &mut build,
                None,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let columns = plan
        .schema()
        .columns()
        .into_iter()
        .map(|c| output(&resolve(&Expr::Column(c.clone()), &s)?, &c.name))
        .collect::<Result<Vec<_>>>()?;
    let sort=s.order.iter().map(|(e,asc,nulls_first)|{let name=field(e)?;build.requirement(&name,"sort");Ok(json!({name:{"order":if *asc{"asc"}else{"desc"},"missing":if *nulls_first{"_first"}else{"_last"}}}))}).collect::<Result<Vec<_>>>()?;
    if engine == "quickwit" && sort.len() > 2 {
        return Err("Quickwit supports at most two sort criteria".into());
    }
    let mut body = json!({"query":and(filters)});
    if !sort.is_empty() {
        body["sort"] = json!(sort)
    }
    if let Some(limit) = s.limit {
        body["size"] = json!(limit)
    }
    let request = json!({"version":1,"engine":engine,"operation":"search","api":"elastic","index":s.table,"body":body,"columns":columns,"limit":s.limit,"offset":s.offset});
    build
        .finish(engine, request, plan.schema().clone())
        .map(Some)
}

/// Explicit engine operations expose backend request languages without hiding
/// their semantics behind nominally portable functions.
fn lower_function(engine: &str, function: &TableFunction) -> Result<Option<PreparedOperation>> {
    if function.name.len() != 2
        || function.name[0] != engine
        || !matches!(
            function.name[1].as_str(),
            "search" | "native_search" | "query" | "native_query"
        )
    {
        return Ok(None);
    }
    let native = function.name[1].starts_with("native_");
    let response = function.name[1].ends_with("query");
    if native && engine != "quickwit" {
        return Err("native operations are Quickwit operations".into());
    }
    if !matches!(function.arguments.len(), 2 | 3) {
        return Err(format!(
            "{engine}.{} requires optional engine name, index and request JSON",
            function.name[1]
        ));
    }
    if function.outer || function.ordinality.is_some() {
        return Err(
            "remote explicit operations currently require inner expansion without ordinality"
                .into(),
        );
    }
    let arguments = &function.arguments[function.arguments.len() - 2..];
    let index = literal(&arguments[0])?
        .as_str()
        .ok_or("remote search index must be a literal string")?
        .to_owned();
    let mut build = Build::new(function.source.clone());
    let body = build.value(&arguments[1], "/body".into())?;
    let body = if let Value::String(s) = body {
        serde_json::from_str(&s).map_err(|e| format!("invalid remote request JSON: {e}"))?
    } else {
        body
    };
    if !body.is_null() && !body.is_object() {
        return Err("remote request must be a JSON object".into());
    }
    let limit = body
        .get(if native { "max_hits" } else { "size" })
        .and_then(Value::as_u64);
    if native && !response && limit.is_none() {
        return Err("quickwit.native_search requires a literal JSON request with max_hits; use quickwit.search for cursor-backed enumeration".into());
    }
    let mut columns = Vec::new();
    if let Some(source) = &function.source {
        for (i, f) in source.schema().fields().iter().enumerate() {
            columns.push(json!({"name":f.name(),"source":"parameter","parameter":i}));
        }
    }
    if response {
        if function.output_schema.fields().len() != 1
            || !crate::ir::functions::domain::is_json(function.output_schema.field(0).data_type())
        {
            return Err(
                "remote query operations require exactly one JSON-domain output column".into(),
            );
        }
        columns.push(json!({"name":function.output_schema.field(0).name(),"source":"response"}));
    } else {
        for f in function.output_schema.fields() {
            if native && f.name() == "_score" {
                return Err(
                    "Quickwit native search does not expose score values; use quickwit.search"
                        .into(),
                );
            }
            columns.push(match f.name().as_str(){"_score"=>json!({"name":f.name(),"source":"score"}),"_id" if !native=>json!({"name":f.name(),"source":"id"}),_=>json!({"name":f.name(),"source":"field","path":f.name().split('.').collect::<Vec<_>>()})});
        }
    }
    let request = json!({"version":1,"engine":engine,"operation":if response{"query"}else{"search"},"api":if native{"native"}else{"elastic"},"index":index,"body":body,"columns":columns,"limit":limit,"explicit":true});
    build
        .finish(engine, request, function.schema.clone())
        .map(Some)
}

#[cfg(all(test, feature = "quickwit", feature = "elasticsearch"))]
mod tests {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::{
        common::DFSchema,
        datasource::{empty::EmptyTable, provider_as_source},
        logical_expr::{col, lit},
    };
    use std::ops::Not;
    fn table() -> LogicalPlan {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("tenant", DataType::Utf8, true),
            Field::new("body", DataType::Utf8, true),
        ]));
        LogicalPlanBuilder::scan(
            "documents",
            provider_as_source(Arc::new(EmptyTable::new(schema))),
            None,
        )
        .unwrap()
        .build()
        .unwrap()
    }
    #[test]
    fn filter_negation_preserves_sql_unknown() {
        let expression = col("tenant")
            .eq(lit("acme"))
            .or(col("id").gt(lit(10i64)))
            .not();
        let mut build = Build::new(None);
        let query = predicate(&expression, true, "/body/query", &mut build, None).unwrap();
        assert_eq!(
            query,
            json!({"bool":{"filter":[
                {"bool":{"filter":[{"exists":{"field":"tenant"}}],"must_not":[{"term":{"tenant":"acme"}}]}},
                {"range":{"id":{"lte":10}}}
            ]}})
        );
        let null = col("tenant").not_eq(lit(ScalarValue::Utf8(None)));
        assert_eq!(
            predicate(&null, true, "", &mut build, None).unwrap(),
            json!({"match_none":{}})
        );
        assert_eq!(
            predicate(&null, false, "", &mut build, None).unwrap(),
            json!({"match_none":{}})
        );
    }
    #[test]
    fn bound_filters_use_typed_parameters_and_null_guards() {
        let source = Arc::new(
            LogicalPlanBuilder::from(table())
                .project(vec![col("tenant").alias("source_tenant")])
                .unwrap()
                .build()
                .unwrap(),
        );
        let mut build = Build::new(Some(source.clone()));
        let query = predicate(
            &col("tenant").not_eq(col("source_tenant")),
            true,
            "/body/query",
            &mut build,
            Some(source.schema()),
        )
        .unwrap();
        let operation = build
            .finish(
                "elasticsearch",
                json!({"body":{"query":query}}),
                source.schema().clone(),
            )
            .unwrap();
        let bound = operation
            .template
            .bind(&[ScalarValue::Utf8(Some("a'\\\"b".into()))])
            .unwrap();
        assert_eq!(
            bound.pointer("/body/query/bool/must_not/0/term/tenant"),
            Some(&json!("a'\\\"b"))
        );
        assert_eq!(
            bound["null_guards"],
            json!([{"value":"/body/query/bool/must_not/0/term/tenant","clause":"/body/query"}])
        );
        assert!(
            operation
                .template
                .bind(&[ScalarValue::Utf8(None)])
                .unwrap()
                .pointer("/body/query/bool/must_not/0/term/tenant")
                .unwrap()
                .is_null()
        );
    }
    #[test]
    fn projection_lineage_and_limit_boundaries_are_preserved() {
        let plan = LogicalPlanBuilder::from(table())
            .project(vec![
                col("id").alias("document_id"),
                col("tenant").alias("owner"),
            ])
            .unwrap()
            .filter(col("owner").eq(lit("acme")))
            .unwrap()
            .sort(vec![col("document_id").sort(false, false)])
            .unwrap()
            .limit(2, Some(3))
            .unwrap()
            .build()
            .unwrap();
        let prepared = ElasticsearchAdapter.lower(&plan).unwrap().unwrap();
        assert_eq!(
            prepared.template.request["body"]["query"]["bool"]["filter"][0],
            json!({"term":{"tenant":"acme"}})
        );
        assert_eq!(
            prepared.template.request["columns"][0]["path"],
            json!(["id"])
        );
        assert_eq!(
            prepared.template.request["body"]["sort"],
            json!([{"id":{"order":"desc","missing":"_last"}}])
        );
        assert_eq!(prepared.template.request["offset"], json!(2));
        let filtered = LogicalPlanBuilder::from(plan)
            .filter(col("owner").eq(lit("other")))
            .unwrap()
            .build()
            .unwrap();
        assert!(ElasticsearchAdapter.lower(&filtered).unwrap().is_none());
    }
    #[test]
    fn backend_query_is_a_typed_composable_relation() {
        use crate::ir::rel::dependent::ArgumentBinding;
        let schema = Arc::new(
            DFSchema::try_from(Schema::new(vec![Field::new(
                "response",
                crate::ir::functions::domain::json_type(),
                false,
            )]))
            .unwrap(),
        );
        let function=TableFunction::new(vec!["quickwit".into(),"native_query".into()],vec![lit("text"),lit("documents"),lit(r#"{"query":"body:\"graph query\"","max_hits":2,"aggs":{"count":{"value_count":{"field":"id"}}}}"#)],None,ArgumentBinding::PrepareTime,schema).unwrap();
        let plan = function.into_plan();
        assert_eq!(QuickwitAdapter.owner(&plan).unwrap(), Some("text".into()));
        let operation = QuickwitAdapter.lower(&plan).unwrap().unwrap();
        assert_eq!(operation.template.request["api"], json!("native"));
        assert_eq!(operation.template.request["operation"], json!("query"));
        assert_eq!(
            operation.template.request["body"]["query"],
            json!("body:\"graph query\"")
        );
        assert_eq!(
            operation.template.request["columns"],
            json!([{"name":"response","source":"response"}])
        );
        assert!(operation.source.is_none());
    }
}
