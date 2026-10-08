//! Code-first relational lowering shared by search, table functions, and future
//! JSON relation operations. Adapters return typed SQL ASTs or explicit execution
//! boundaries; no failed execution is retried using another access method.
use super::*;
use crate::ir::rel::dependent::{ArgumentBinding, TableFunction};
use datafusion::common::DFSchemaRef;
use datafusion::logical_expr::{Extension, UserDefinedLogicalNode};
use datafusion::sql::{
    sqlparser::{ast, parser::Parser},
    unparser::{
        Unparser,
        ast::{DerivedRelationBuilder, QueryBuilder, RelationBuilder, SelectBuilder},
        extension_unparser::{
            UnparseToStatementResult, UnparseWithinStatementResult, UserDefinedLogicalNodeUnparser,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweringMode {
    InIsland,
    Bound,
}
#[derive(Debug, Clone, Copy)]
pub struct LoweringContext {
    pub dialect: SqlDialect,
    pub mode: LoweringMode,
}
#[derive(Debug, Clone)]
pub enum RelationLowering {
    Sql(ast::Statement),
    Rewrite(LogicalPlan),
    Dependent {
        source: Arc<LogicalPlan>,
        template: SqlTemplate,
        schema: DFSchemaRef,
    },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationPlacement {
    pub source: Option<Arc<LogicalPlan>>,
    pub target: Option<Arc<LogicalPlan>>,
}
/// Describe the inputs whose ownership determines placement. New shared
/// relational nodes can participate without changing SQL-island execution.
pub fn placement(plan: &LogicalPlan) -> Option<RelationPlacement> {
    let LogicalPlan::Extension(extension) = plan else {
        return None;
    };
    if let Some(search) = extension
        .node
        .as_any()
        .downcast_ref::<crate::ir::rel::search::RankedJoin>()
    {
        return Some(RelationPlacement {
            source: Some(search.source.clone()),
            target: Some(search.target.clone()),
        });
    }
    extension
        .node
        .as_any()
        .downcast_ref::<TableFunction>()
        .map(|function| RelationPlacement {
            source: function.source.clone(),
            target: None,
        })
}
/// An adapter may describe ownership for its own extension nodes.
pub fn placement_for(
    plan: &LogicalPlan,
    dialect: SqlDialect,
) -> SqlResult<Option<RelationPlacement>> {
    let mut selected = placement(plan);
    let mut adapters = SqlDialect::registered_adapters()?;
    if let SqlDialect::Custom(adapter) = dialect {
        adapters.push(adapter);
    }
    for adapter in adapters {
        if let Some(candidate) = adapter.relation_placement(plan) {
            if selected
                .as_ref()
                .is_some_and(|existing| existing != &candidate)
            {
                return Err(SqlError::Unsupported(
                    "engine adapters supplied conflicting relational placement".into(),
                ));
            }
            selected = Some(candidate);
        }
    }
    Ok(selected)
}
pub fn lower_relation(
    plan: &LogicalPlan,
    ctx: &LoweringContext,
) -> SqlResult<Option<RelationLowering>> {
    let lowered = ctx.dialect.lower_relation(plan, ctx)?;
    match &lowered {
        Some(RelationLowering::Sql(statement))
            if !matches!(statement, ast::Statement::Query(_)) =>
        {
            return Err(SqlError::Conversion(
                "relational lowering must produce a query statement".into(),
            ));
        }
        Some(RelationLowering::Rewrite(replacement)) if replacement.schema() != plan.schema() => {
            return Err(SqlError::Conversion(
                "relational rewrite changed the output schema".into(),
            ));
        }
        Some(RelationLowering::Dependent {
            source,
            template,
            schema,
        }) => {
            if schema != plan.schema() {
                return Err(SqlError::Conversion(
                    "dependent lowering changed the output schema".into(),
                ));
            }
            if template.parameters != source.schema().fields().len() {
                return Err(SqlError::Conversion(
                    "dependent lowering parameter count must match source columns".into(),
                ));
            }
            if template.dialect != ctx.dialect.name() {
                return Err(SqlError::Conversion(
                    "dependent lowering changed owning engine dialect".into(),
                ));
            }
        }
        _ => {}
    }
    Ok(lowered)
}
/// Default rules used by the built-in engines and available to adapters that
/// intentionally compose them with their own transformations.
pub fn builtin_lower_relation(
    plan: &LogicalPlan,
    ctx: &LoweringContext,
) -> SqlResult<Option<RelationLowering>> {
    let LogicalPlan::Extension(extension) = plan else {
        return Ok(None);
    };
    if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>() {
        if let Some(lowered) = super::json_rows::lower(function, ctx)? {
            return Ok(Some(lowered));
        }
        if function.native_list.is_some() {
            return function
                .native_plan()
                .map(RelationLowering::Rewrite)
                .map(Some)
                .map_err(SqlError::from);
        }
        return lower_table_function(function, ctx).map(Some);
    }
    super::search::lower_relation(plan, ctx)
}
fn parse_statement(sql: &str, dialect: SqlDialect) -> SqlResult<ast::Statement> {
    let parser = dialect.parser_dialect();
    let mut statements = Parser::parse_sql(parser.as_ref(), sql)
        .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    if statements.len() != 1 {
        return Err(SqlError::Unsupported(
            "relational lowering requires one statement".into(),
        ));
    }
    Ok(statements.remove(0))
}
fn lower_table_function(
    function: &TableFunction,
    ctx: &LoweringContext,
) -> SqlResult<RelationLowering> {
    super::logical_functions::with_plan(&function.clone().into_plan(), ctx.dialect, || {
        lower_table_function_inner(function, ctx)
    })
}
fn lower_table_function_inner(
    function: &TableFunction,
    ctx: &LoweringContext,
) -> SqlResult<RelationLowering> {
    let dialect = ctx.dialect;
    let bound = function.source.is_some()
        && (ctx.mode == LoweringMode::Bound || function.binding == ArgumentBinding::PrepareTime);
    let sql_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(sql_dialect.as_ref());
    let mut arguments = Vec::new();
    for argument in &function.arguments {
        let expr = argument
            .clone()
            .transform_up(|expr| {
                if let Expr::Column(mut column) = expr {
                    column.relation = Some(datafusion::common::TableReference::bare(
                        "__relation_source",
                    ));
                    return Ok(Transformed::yes(Expr::Column(column)));
                }
                Ok(Transformed::no(expr))
            })?
            .data;
        let mut expression = unparser.expr_to_sql(&expr)?;
        super::functions::prepare_scoped_ast(&mut expression, dialect, false)?;
        arguments.push(expression.to_string());
    }
    let name = function
        .name
        .iter()
        .map(|part| dialect.quote_ident(part))
        .collect::<Vec<_>>()
        .join(".");
    let call = format!(
        "{name}({}){}",
        arguments.join(", "),
        if function.ordinality.is_some() {
            " WITH ORDINALITY"
        } else {
            ""
        }
    );
    let result_alias = format!(
        "__relation_result({})",
        function
            .output_schema
            .fields()
            .iter()
            .map(|f| dialect.quote_ident(f.name()))
            .chain(
                function
                    .ordinality
                    .iter()
                    .map(|name| dialect.quote_ident(name))
            )
            .collect::<Vec<_>>()
            .join(", ")
    );
    let output = function
        .output_schema
        .fields()
        .iter()
        .map(|field| format!("__relation_result.{}", dialect.quote_ident(field.name())))
        .chain(
            function
                .ordinality
                .iter()
                .map(|name| format!("__relation_result.{}", dialect.quote_ident(name))),
        )
        .collect::<Vec<_>>();
    let mut columns = Vec::new();
    let from =
        if let Some(source) = &function.source {
            columns.extend(
                source.schema().fields().iter().map(|field| {
                    format!("__relation_source.{}", dialect.quote_ident(field.name()))
                }),
            );
            let source_sql = if bound {
                format!(
                    "SELECT {}",
                    source
                        .schema()
                        .fields()
                        .iter()
                        .enumerate()
                        .map(|(i, field)| format!(
                            "${} AS {}",
                            i + 1,
                            dialect.quote_ident(field.name())
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            } else {
                super::recursive::unparse_plan(source.as_ref().clone(), dialect)?
            };
            // Bound operations substitute all column references below, making the
            // function arguments literals before the engine prepares this SQL.
            format!(
                "({source_sql}) AS __relation_source {} JOIN LATERAL {call} AS {result_alias}{}",
                if function.outer { "LEFT" } else { "CROSS" },
                if function.outer { " ON TRUE" } else { "" }
            )
        } else if function.outer {
            format!(
                "(SELECT 1) AS __relation_source LEFT JOIN LATERAL {call} AS {result_alias} ON TRUE"
            )
        } else {
            format!("{call} AS {result_alias}")
        };
    columns.extend(output);
    let mut statement = parse_statement(
        &format!("SELECT {} FROM {from}", columns.join(", ")),
        dialect,
    )?;
    if bound {
        let source = function.source.as_ref().unwrap();
        let _ = ast::visit_expressions_mut(&mut statement, |expr| {
            if let ast::Expr::CompoundIdentifier(parts) = expr {
                if parts.len() == 2 && parts[0].value == "__relation_source" {
                    if let Some(index) = source
                        .schema()
                        .fields()
                        .iter()
                        .position(|f| f.name() == &parts[1].value)
                    {
                        *expr = ast::Expr::Value(
                            ast::Value::Placeholder(format!("${}", index + 1)).into(),
                        );
                    }
                }
            }
            std::ops::ControlFlow::<()>::Continue(())
        });
        // Keep stable output names even when source projections become literals.
        if let ast::Statement::Query(query) = &mut statement {
            if let ast::SetExpr::Select(select) = query.body.as_mut() {
                for (item, field) in select.projection.iter_mut().zip(source.schema().fields()) {
                    if let ast::SelectItem::UnnamedExpr(expr) = item {
                        *item = ast::SelectItem::ExprWithAlias {
                            expr: expr.clone(),
                            alias: dialect.identifier(field.name()),
                        };
                    }
                }
            }
        }
        Ok(RelationLowering::Dependent {
            source: source.clone(),
            template: SqlTemplate {
                sql: statement.to_string(),
                parameters: source.schema().fields().len(),
                dialect: dialect.name().into(),
            },
            schema: function.schema.clone(),
        })
    } else {
        Ok(RelationLowering::Sql(statement))
    }
}

pub(super) struct RelationUnparser {
    dialect: SqlDialect,
    next_alias: std::cell::Cell<usize>,
}
impl RelationUnparser {
    pub fn new(dialect: SqlDialect) -> Self {
        Self {
            dialect,
            next_alias: std::cell::Cell::new(0),
        }
    }
}
impl RelationUnparser {
    fn statement(&self, node: &dyn UserDefinedLogicalNode) -> SqlResult<Option<ast::Statement>> {
        if let Some(sql) = node.as_any().downcast_ref::<SqlRelation>() {
            return Ok(Some(sql.statement.clone()));
        }
        let plan = LogicalPlan::Extension(Extension {
            node: Arc::from(node.with_exprs_and_inputs(
                node.expressions(),
                node.inputs().into_iter().cloned().collect(),
            )?),
        });
        match lower_relation(&plan,&LoweringContext {dialect:self.dialect,mode:LoweringMode::InIsland})? {
            Some(RelationLowering::Sql(statement))=>Ok(Some(statement)),
            Some(RelationLowering::Rewrite(plan))=>parse_statement(&super::recursive::unparse_plan(plan,self.dialect)?,self.dialect).map(Some),
            Some(RelationLowering::Dependent {..})=>Err(SqlError::Unsupported("relational operation requires bound source inputs; execute its dependent SQL island".into())),
            None=>Ok(None),
        }
    }
}
impl UserDefinedLogicalNodeUnparser for RelationUnparser {
    fn unparse(
        &self,
        node: &dyn UserDefinedLogicalNode,
        _unparser: &Unparser,
        _query: &mut Option<&mut QueryBuilder>,
        _select: &mut Option<&mut SelectBuilder>,
        relation: &mut Option<&mut RelationBuilder>,
    ) -> Result<UnparseWithinStatementResult, DataFusionError> {
        let Some(statement) = self
            .statement(node)
            .map_err(|e| DataFusionError::Plan(e.to_string()))?
        else {
            return Ok(UnparseWithinStatementResult::Unmodified);
        };
        let ast::Statement::Query(query) = statement else {
            return Err(DataFusionError::Plan(
                "relation lowering must produce a query".into(),
            ));
        };
        let target = relation
            .as_mut()
            .ok_or_else(|| DataFusionError::Plan("relation missing SQL context".into()))?;
        let mut derived = DerivedRelationBuilder::default();
        derived
            .lateral(false)
            .subquery(query)
            .alias(Some(ast::TableAlias {
                name: self.dialect.identifier(&format!("__lowered_relation_{}", {
                    let n = self.next_alias.get();
                    self.next_alias.set(n + 1);
                    n
                })),
                columns: vec![],
                explicit: true,
            }));
        target.derived(derived);
        Ok(UnparseWithinStatementResult::Modified)
    }
    fn unparse_to_statement(
        &self,
        node: &dyn UserDefinedLogicalNode,
        _unparser: &Unparser,
    ) -> Result<UnparseToStatementResult, DataFusionError> {
        Ok(
            match self
                .statement(node)
                .map_err(|e| DataFusionError::Plan(e.to_string()))?
            {
                Some(statement) => UnparseToStatementResult::Modified(statement),
                None => UnparseToStatementResult::Unmodified,
            },
        )
    }
}

/// A dependent SQL island. Positional parameters are source-row values, never
/// user-supplied SQL. Binding substitutes typed SQL AST literals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlTemplate {
    pub sql: String,
    pub parameters: usize,
    pub dialect: String,
}
impl SqlTemplate {
    pub fn bind(&self, values: &[ScalarValue]) -> SqlResult<String> {
        if values.len() != self.parameters {
            return Err(SqlError::Conversion("SQL parameter count mismatch".into()));
        }
        self.prepare()?.bind(values)
    }

    /// Parse the invariant statement once per execution. Only the AST is reused;
    /// bound values and query results never survive an input occurrence.
    pub(crate) fn prepare(&self) -> SqlResult<PreparedSqlTemplate> {
        let dialect = SqlDialect::resolve(&self.dialect)?;
        let parser = dialect.parser_dialect();
        let mut statements = Parser::parse_sql(parser.as_ref(), &self.sql)
            .map_err(|e| SqlError::Unsupported(e.to_string()))?;
        if statements.len() != 1 {
            return Err(SqlError::Unsupported(
                "dependent SQL must be one statement".into(),
            ));
        }
        let statement = statements.remove(0);
        if !matches!(statement, ast::Statement::Query(_)) {
            return Err(SqlError::Unsupported(
                "dependent SQL must be a query statement".into(),
            ));
        }
        Ok(PreparedSqlTemplate {
            statement,
            parameters: self.parameters,
            dialect,
        })
    }
}

#[derive(Debug)]
pub(crate) struct PreparedSqlTemplate {
    statement: ast::Statement,
    parameters: usize,
    dialect: SqlDialect,
}
impl PreparedSqlTemplate {
    pub(crate) fn bind(&self, values: &[ScalarValue]) -> SqlResult<String> {
        if values.len() != self.parameters {
            return Err(SqlError::Conversion("SQL parameter count mismatch".into()));
        }
        let dialect = self.dialect;
        let parser = dialect.parser_dialect();
        let mut statement = self.statement.clone();
        let literals = values
            .iter()
            .map(|v| {
                let text = super::exchange_literal(v.clone(), v.data_type(), dialect)?;
                Parser::new(parser.as_ref())
                    .try_with_sql(&text)
                    .map_err(|e| SqlError::Conversion(e.to_string()))?
                    .parse_expr()
                    .map_err(|e| SqlError::Conversion(e.to_string()))
            })
            .collect::<SqlResult<Vec<_>>>()?;
        let flow = ast::visit_expressions_mut(&mut statement, |expr| {
            if let ast::Expr::Value(value) = expr {
                if let ast::Value::Placeholder(name) = &value.value {
                    let index = name
                        .strip_prefix('$')
                        .and_then(|n| n.parse::<usize>().ok())
                        .and_then(|n| n.checked_sub(1));
                    match index.and_then(|i| literals.get(i)) {
                        Some(literal) => *expr = literal.clone(),
                        None => {
                            return std::ops::ControlFlow::Break(SqlError::Conversion(
                                "invalid SQL parameter".into(),
                            ));
                        }
                    }
                }
            }
            std::ops::ControlFlow::Continue(())
        });
        if let std::ops::ControlFlow::Break(e) = flow {
            return Err(e);
        }
        Ok(statement.to_string())
    }
}

/// A physical SQL subtree emitted by an adapter for an ordinary relational node.
/// It is opaque to subsequent relational rewrites: the adapter has already
/// consumed the subtree and must preserve its declared output schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SqlRelation {
    statement: ast::Statement,
    schema: DFSchemaRef,
}
impl PartialOrd for SqlRelation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl datafusion::logical_expr::UserDefinedLogicalNodeCore for SqlRelation {
    fn name(&self) -> &str {
        "SqlRelation"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SqlRelation: {}", self.statement)
    }
    fn with_exprs_and_inputs(
        &self,
        exprs: Vec<Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> datafusion::common::Result<Self> {
        if !exprs.is_empty() || !inputs.is_empty() {
            return Err(DataFusionError::Plan(
                "physical SQL relation cannot acquire inputs".into(),
            ));
        }
        Ok(self.clone())
    }
}
/// Give an engine code-first transformations over the original typed plan,
/// including extension nodes, before SQL scope/CTE normalization consumes
/// physical column lineage. Completed SQL subtrees are opaque to this pass.
pub(crate) fn rewrite_relations(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<LogicalPlan> {
    if !matches!(dialect, SqlDialect::Custom(_)) { return Ok(plan); }
    Ok(plan.transform_down_with_subqueries(|node| {
        match lower_relation(&node, &LoweringContext { dialect, mode: LoweringMode::InIsland })
            .map_err(|error| DataFusionError::Plan(error.to_string()))? {
            Some(RelationLowering::Rewrite(replacement)) => Ok(Transformed::yes(replacement)),
            _ => Ok(Transformed::no(node)),
        }
    })?.data)
}

pub(crate) fn transform_relations(
    plan: LogicalPlan,
    dialect: SqlDialect,
) -> SqlResult<LogicalPlan> {
    if !matches!(dialect, SqlDialect::Custom(_)) {
        return Ok(plan);
    }
    Ok(plan
        .transform_down_with_subqueries(|node| {
            if matches!(&node, LogicalPlan::Extension(extension) if extension.node.as_any().is::<SqlRelation>()) {
                return Ok(Transformed::no(node));
            }
            let lowered = lower_relation(
                &node,
                &LoweringContext {
                    dialect,
                    mode: LoweringMode::InIsland,
                },
            )
            .map_err(|e| DataFusionError::Plan(e.to_string()))?;
            match lowered {
                Some(RelationLowering::Sql(statement)) => {
                    Ok(Transformed::yes(LogicalPlan::Extension(Extension {
                        node: Arc::new(SqlRelation {
                            statement,
                            schema: node.schema().clone(),
                        }),
                    })))
                }
                Some(RelationLowering::Rewrite(replacement)) => Ok(Transformed::yes(replacement)),
                Some(RelationLowering::Dependent { .. }) => Err(DataFusionError::Plan(
                    "relational operation requires dependent placement before SQL rendering".into(),
                )),
                None => Ok(Transformed::no(node)),
            }
        })?
        .data)
}
