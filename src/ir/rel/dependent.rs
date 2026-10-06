//! Relational functions and dependent operations, independent of search and engine.
use super::sql::lowering::SqlTemplate;
use datafusion::{
    common::{DFSchemaRef, DataFusionError, Result},
    logical_expr::{Expr, ExprSchemable, Extension, LogicalPlan, UserDefinedLogicalNodeCore},
};
use std::{fmt, sync::Arc};

/// Whether a function accepts SQL correlation or needs values at prepare time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum ArgumentBinding {
    Correlated,
    PrepareTime,
}

/// A row-producing operation. Predicates remain above this boundary unless an
/// adapter explicitly proves and implements a valid pushdown transformation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableFunction {
    pub name: Vec<String>,
    pub arguments: Vec<Expr>,
    pub source: Option<Arc<LogicalPlan>>,
    pub binding: ArgumentBinding,
    pub output_schema: DFSchemaRef,
    /// Native row-producing expression, a list of structs matching output_schema.
    pub native_list: Option<Expr>,
    /// Preserve a source row when the function produces no rows.
    pub outer: bool,
    /// Optional one-based row position; null on synthetic outer rows.
    pub ordinality: Option<String>,
    pub schema: DFSchemaRef,
}
impl TableFunction {
    pub fn new(
        name: Vec<String>,
        arguments: Vec<Expr>,
        source: Option<Arc<LogicalPlan>>,
        binding: ArgumentBinding,
        output_schema: DFSchemaRef,
    ) -> Result<Self> {
        if name.is_empty() || name.iter().any(String::is_empty) {
            return Err(DataFusionError::Plan(
                "table function name must be nonempty".into(),
            ));
        }
        let empty = datafusion::common::DFSchema::empty();
        let input = source
            .as_ref()
            .map(|p| p.schema().as_ref())
            .unwrap_or(&empty);
        for arg in &arguments {
            arg.get_type(input)?;
        }
        // Bound SQL parameters and exchange output are addressed by flat
        // column names. Reject ambiguous schemas before a name can select the
        // wrong source value; callers can project explicit aliases first.
        let mut names = std::collections::BTreeSet::new();
        for field in input.fields().iter().chain(output_schema.fields()) {
            if !names.insert(field.name()) {
                return Err(DataFusionError::Plan("table function source and output columns must have unique names; project unique aliases before creating the function".into()));
            }
        }
        let schema = Arc::new(input.join(output_schema.as_ref())?);
        Ok(Self {
            name,
            arguments,
            source,
            binding,
            output_schema,
            native_list: None,
            outer: false,
            ordinality: None,
            schema,
        })
    }
    pub fn with_native_list(mut self, expression: Expr) -> Result<Self> {
        let empty = datafusion::common::DFSchema::empty();
        let input = self
            .source
            .as_ref()
            .map(|p| p.schema().as_ref())
            .unwrap_or(&empty);
        let ty = expression.get_type(input)?;
        let arrow::datatypes::DataType::List(item) = ty else {
            return Err(DataFusionError::Plan(
                "native row function must return a list of structs".into(),
            ));
        };
        let arrow::datatypes::DataType::Struct(fields) = item.data_type() else {
            return Err(DataFusionError::Plan(
                "native row function must return a list of structs".into(),
            ));
        };
        if fields.len() != self.output_schema.fields().len()
            || fields
                .iter()
                .zip(self.output_schema.fields())
                .any(|(a, b)| a.data_type() != b.data_type())
        {
            return Err(DataFusionError::Plan(
                "native row function schema differs from declared output".into(),
            ));
        }
        self.native_list = Some(expression);
        Ok(self)
    }
    pub fn with_expansion(mut self, outer: bool, ordinality: Option<String>) -> Result<Self> {
        use arrow::datatypes::{DataType, Field, Schema};
        self.outer = outer;
        let output = self
            .output_schema
            .fields()
            .iter()
            .map(|f| f.as_ref().clone().with_nullable(outer || f.is_nullable()))
            .collect::<Vec<_>>();
        self.output_schema = Arc::new(datafusion::common::DFSchema::try_from(Schema::new(output))?);
        let empty = datafusion::common::DFSchema::empty();
        let input = self
            .source
            .as_ref()
            .map(|p| p.schema().as_ref())
            .unwrap_or(&empty);
        let mut schema = input.join(self.output_schema.as_ref())?;
        if let Some(name) = &ordinality {
            if name.is_empty() || schema.has_column_with_unqualified_name(name) {
                return Err(DataFusionError::Plan(
                    "ordinality requires a distinct nonempty output name".into(),
                ));
            }
            schema = schema.join(&datafusion::common::DFSchema::try_from(Schema::new(vec![
                Field::new(name, DataType::Int64, true),
            ]))?)?;
        }
        self.ordinality = ordinality;
        self.schema = Arc::new(schema);
        Ok(self)
    }
    pub fn native_plan(&self) -> Result<LogicalPlan> {
        use arrow::datatypes::DataType;
        use datafusion::{
            common::{Column, UnnestOptions},
            logical_expr::{LogicalPlanBuilder, lit},
        };
        let list = self.native_list.as_ref().ok_or_else(|| {
            DataFusionError::Plan(format!(
                "{} has no native row implementation",
                self.name.join(".")
            ))
        })?;
        let source = self
            .source
            .as_ref()
            .map(|p| p.as_ref().clone())
            .unwrap_or(LogicalPlanBuilder::empty(true).build()?);
        let DataType::List(item) = list.get_type(source.schema())? else {
            unreachable!()
        };
        let DataType::Struct(fields) = item.data_type() else {
            unreachable!()
        };
        let source_cols = source
            .schema()
            .columns()
            .into_iter()
            .map(Expr::Column)
            .collect::<Vec<_>>();
        static NEXT_SCOPE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let scope = NEXT_SCOPE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let row_name = format!("__orchiddb_native_function_row_{scope}");
        let position_name = format!("__orchiddb_native_function_position_{scope}");
        let row = row_name.as_str();
        let position = position_name.as_str();
        let mut projection = source_cols.clone();
        projection.push(list.clone().alias(row));
        let listed = LogicalPlanBuilder::from(source)
            .project(projection)?
            .build()?;
        let mut projection = source_cols.clone();
        let list = datafusion::logical_expr::col(row);
        projection.push(
            if self.outer {
                super::collections::outer_list(list.clone(), &DataType::List(item.clone()))
                    .map_err(|e| DataFusionError::Plan(e.to_string()))?
            } else {
                list.clone()
            }
            .alias(row),
        );
        let mut unnest = vec![Column::new_unqualified(row)];
        if self.ordinality.is_some() {
            let positions = datafusion::functions_nested::expr_fn::range(
                lit(1_i64),
                datafusion::logical_expr::cast(
                    datafusion::functions_nested::expr_fn::array_length(list),
                    DataType::Int64,
                ) + lit(1_i64),
                lit(1_i64),
            );
            let ty = positions.get_type(listed.schema())?;
            projection.push(
                if self.outer {
                    super::collections::outer_list(positions, &ty)
                        .map_err(|e| DataFusionError::Plan(e.to_string()))?
                } else {
                    positions
                }
                .alias(position),
            );
            unnest.push(Column::new_unqualified(position));
        }
        let projected = LogicalPlanBuilder::from(listed)
            .project(projection)?
            .build()?;
        let projected = super::collections::unnest_scope(
            projected,
            format!("__w_sql_cte_function_input_{scope}"),
        )
        .map_err(|e| DataFusionError::Plan(e.to_string()))?;
        let projected = super::collections::unnest_input(projected)
            .map_err(|e| DataFusionError::Plan(e.to_string()))?;
        let expanded = LogicalPlanBuilder::from(projected)
            .unnest_columns_with_options(
                unnest,
                UnnestOptions {
                    preserve_nulls: self.outer,
                    ..Default::default()
                },
            )?
            .build()?;
        let expanded = super::collections::unnest_scope(
            expanded,
            format!("__w_sql_cte_function_rows_{scope}"),
        )
        .map_err(|e| DataFusionError::Plan(e.to_string()))?;
        let mut output = source_cols
            .into_iter()
            .map(|expr| {
                let Expr::Column(column) = expr else {
                    unreachable!()
                };
                super::col_exact(&column.name).alias_qualified(column.relation, column.name)
            })
            .collect::<Vec<_>>();
        output.extend(
            fields
                .iter()
                .zip(self.output_schema.fields())
                .enumerate()
                .map(|(index, (_, output))| {
                    native_row_field(
                        datafusion::logical_expr::col(row),
                        item.data_type(),
                        index,
                        output.data_type(),
                    )
                    .alias(output.name())
                }),
        );
        if let Some(name) = &self.ordinality {
            output.push(datafusion::logical_expr::col(position).alias(name));
        }
        LogicalPlanBuilder::from(expanded).project(output)?.build()
    }
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(Extension {
            node: Arc::new(self),
        })
    }
}
impl PartialOrd for TableFunction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl UserDefinedLogicalNodeCore for TableFunction {
    fn name(&self) -> &str {
        "TableFunction"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        self.source.iter().map(AsRef::as_ref).collect()
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        self.arguments
            .iter()
            .cloned()
            .chain(self.native_list.clone())
            .collect()
    }
    fn prevent_predicate_push_down_columns(&self) -> std::collections::HashSet<String> {
        // A deterministic expansion operates independently on each input row.
        // Source-only predicates may move below it; predicates involving emitted
        // fields or ordinality must preserve the expansion boundary.
        if self
            .native_list
            .as_ref()
            .is_some_and(|expr| !expr.is_volatile())
        {
            self.output_schema
                .fields()
                .iter()
                .map(|field| field.name().clone())
                .chain(self.ordinality.clone())
                .collect()
        } else {
            self.schema
                .fields()
                .iter()
                .map(|field| field.name().clone())
                .collect()
        }
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "TableFunction: {} {:?}, {:?}",
            self.name.join("."),
            self.arguments,
            self.binding
        )
    }
    fn with_exprs_and_inputs(&self, exprs: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        if inputs.len() != usize::from(self.source.is_some())
            || exprs.len() != self.arguments.len() + usize::from(self.native_list.is_some())
        {
            return Err(DataFusionError::Plan(
                "table function inputs changed".into(),
            ));
        }
        let mut node = Self::new(
            self.name.clone(),
            exprs[..self.arguments.len()].to_vec(),
            inputs.into_iter().next().map(Arc::new),
            self.binding,
            self.output_schema.clone(),
        )?;
        if self.native_list.is_some() {
            node = node.with_native_list(exprs.last().unwrap().clone().unalias())?;
        }
        node.with_expansion(self.outer, self.ordinality.clone())
    }
}

// The generic row-expansion kernel extracts a field after UNNEST. A normal
// get_field call is incorrectly moved below list-to-struct UNNEST by DF53's
// leaf-projection rule; this typed native kernel preserves that cardinality
// boundary while retaining the parent's validity mask.
fn native_row_field(
    input: Expr,
    row_type: &arrow::datatypes::DataType,
    index: usize,
    output_type: &arrow::datatypes::DataType,
) -> Expr {
    use arrow::array::Array;
    use datafusion::common::ScalarValue;
    use datafusion::logical_expr::{
        ColumnarValue, ScalarUDF, Signature, Volatility, expr_fn::SimpleScalarUDF,
    };
    let output_type = output_type.clone();
    ScalarUDF::from(SimpleScalarUDF::new_with_signature(
        format!("__orchiddb_row_field_{index}"),
        Signature::exact(vec![row_type.clone()], Volatility::Immutable),
        output_type.clone(),
        Arc::new(move |args| {
            let scalar = matches!(args[0], ColumnarValue::Scalar(_));
            let arrays = ColumnarValue::values_to_arrays(args)?;
            let rows = arrays[0]
                .as_any()
                .downcast_ref::<arrow::array::StructArray>()
                .ok_or_else(|| {
                    DataFusionError::Internal("row field requires expanded struct".into())
                })?;
            let values = (0..rows.len())
                .map(|i| {
                    if rows.is_null(i) {
                        ScalarValue::try_from(&output_type)
                    } else {
                        ScalarValue::try_from_array(rows.column(index), i)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            if scalar {
                return Ok(ColumnarValue::Scalar(
                    values
                        .into_iter()
                        .next()
                        .ok_or_else(|| DataFusionError::Internal("empty scalar row".into()))?,
                ));
            }
            Ok(ColumnarValue::Array(if values.is_empty() {
                arrow::array::new_empty_array(&output_type)
            } else {
                ScalarValue::iter_to_array(values)?
            }))
        }),
    ))
    .call(vec![input])
}

/// Execute a typed SQL template once for each source row, using the operation's
/// owning engine session. Its output schema includes any source columns needed
/// by the parent; the template explicitly projects them.
/// Execution is incremental: the first occurrence is emitted immediately, then
/// bounded groups amortize scheduling. A downstream limit or dropped stream can
/// stop further requests. Blocking driver calls already in flight may finish.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependentOperation {
    pub source: Arc<LogicalPlan>,
    pub template: SqlTemplate,
    pub schema: DFSchemaRef,
}
impl PartialOrd for DependentOperation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl DependentOperation {
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(Extension {
            node: Arc::new(self),
        })
    }
}
impl UserDefinedLogicalNodeCore for DependentOperation {
    fn name(&self) -> &str {
        "DependentOperation"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![&self.source]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependentOperation: {}", self.template.sql)
    }
    fn with_exprs_and_inputs(&self, exprs: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        if !exprs.is_empty() || inputs.len() != 1 {
            return Err(DataFusionError::Plan(
                "dependent operation inputs changed".into(),
            ));
        }
        Ok(Self {
            source: Arc::new(inputs[0].clone()),
            ..self.clone()
        })
    }
}

/// Engine-independent DataFusion execution of bound relational operations.
pub mod exec;

/// Expand native row implementations only after SQL-island placement (or when
/// explicitly using the native engine). SQL adapters see the original operator.
pub fn native(plan: LogicalPlan) -> Result<LogicalPlan> {
    use datafusion::common::tree_node::Transformed;
    Ok(plan
        .transform_up_with_subqueries(|plan| {
            if let LogicalPlan::Extension(extension) = &plan {
                if let Some(function) = extension.node.as_any().downcast_ref::<TableFunction>() {
                    return Ok(Transformed::yes(function.native_plan()?));
                }
            }
            Ok(Transformed::no(plan))
        })?
        .data)
}
