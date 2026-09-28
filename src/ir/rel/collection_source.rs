//! Named, read-only relations obtained by expanding one native list column.
use super::{RelError, RelResult};
use arrow::datatypes::{DataType, Schema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSource {
    pub name: String,
    pub table: String,
    pub column: String,
    /// Output alias -> parent column.
    #[serde(default)]
    pub parent_columns: BTreeMap<String, String>,
    /// Output column for a scalar list element. Mutually exclusive with fields.
    #[serde(default)]
    pub element: Option<String>,
    /// Output alias -> field of a struct list element.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}
impl CollectionSource {
    pub(crate) fn plan(
        &self,
        parent: datafusion::logical_expr::LogicalPlan,
    ) -> RelResult<datafusion::logical_expr::LogicalPlan> {
        use super::{col_exact, collections};
        use datafusion::{
            common::{Column, UnnestOptions},
            logical_expr::LogicalPlanBuilder,
        };
        let mut projection = self
            .parent_columns
            .values()
            .enumerate()
            .map(|(i, c)| col_exact(c).alias(format!("__parent_{i}")))
            .collect::<Vec<_>>();
        projection.push(col_exact(&self.column).alias("__element"));
        let input = LogicalPlanBuilder::from(parent)
            .project(projection)?
            .build()?;
        let input = collections::unnest_scope(
            input,
            format!("__w_sql_cte_collection_input_{}", self.name),
        )?;
        let input = collections::unnest_input(input)?;
        let options = UnnestOptions {
            preserve_nulls: false,
            ..Default::default()
        };
        let expanded = LogicalPlanBuilder::from(input)
            .unnest_column_with_options(Column::new_unqualified("__element"), options)?
            .build()?;
        let expanded = collections::unnest_scope(
            expanded,
            format!("__w_sql_cte_collection_rows_{}", self.name),
        )?;
        let mut output = self
            .parent_columns
            .keys()
            .enumerate()
            .map(|(i, name)| col_exact(format!("__parent_{i}")).alias(name))
            .collect::<Vec<_>>();
        if let Some(name) = &self.element {
            output.push(col_exact("__element").alias(name));
        }
        for (name, field) in &self.fields {
            output.push(
                datafusion::functions::core::expr_fn::get_field(
                    col_exact("__element"),
                    field.as_str(),
                )
                .alias(name),
            );
        }
        Ok(LogicalPlanBuilder::from(expanded)
            .project(output)?
            .build()?)
    }

    pub(crate) fn validate(&self, schema: &Schema) -> RelResult<()> {
        let invalid =
            |s: &str| RelError::Unsupported(format!("collection source `{}`: {s}", self.name));
        if self.name.is_empty() || self.table.is_empty() || self.name == self.table {
            return Err(invalid("requires distinct nonempty source and table names"));
        }
        let field = schema
            .field_with_name(&self.column)
            .map_err(|_| invalid("missing list column"))?;
        let element = match field.data_type() {
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                f.data_type()
            }
            _ => return Err(invalid("column must be a native list")),
        };
        let scalar = |t: &DataType| !t.is_nested();
        for column in self.parent_columns.values() {
            let f = schema
                .field_with_name(column)
                .map_err(|_| invalid("missing parent column"))?;
            if !scalar(f.data_type()) {
                return Err(invalid("parent columns must be scalar"));
            }
        }
        match (&self.element, element) {
            (Some(_), t) if self.fields.is_empty() && scalar(t) => {}
            (None, DataType::Struct(fields)) if !self.fields.is_empty() => {
                for field in self.fields.values() {
                    let f = fields
                        .iter()
                        .find(|f| f.name() == field)
                        .ok_or_else(|| invalid("missing element struct field"))?;
                    if !scalar(f.data_type()) {
                        return Err(invalid("element fields must be scalar"));
                    }
                }
            }
            _ => {
                return Err(invalid(
                    "choose element for scalar lists or fields for struct lists",
                ));
            }
        }
        let mut names = BTreeSet::new();
        for name in self
            .parent_columns
            .keys()
            .chain(self.element.iter())
            .chain(self.fields.keys())
        {
            if name.is_empty() || !names.insert(name) {
                return Err(invalid("output column names must be nonempty and distinct"));
            }
        }
        Ok(())
    }
}
