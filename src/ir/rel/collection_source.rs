//! Named read-only relations produced by collection expressions and row functions.
use super::{RelError, RelResult};
use arrow::datatypes::{DataType, Schema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionSource {
    pub name: String,
    pub table: String,
    /// Legacy native-list column. Mutually exclusive with expand.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub column: String,
    /// Immutable list-valued expression, including registered row functions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand: Option<String>,
    /// Row alias used by field expressions, for example item.value.
    #[serde(default, rename = "as", skip_serializing_if = "Option::is_none")]
    pub row_alias: Option<String>,
    #[serde(default)]
    pub outer: bool,
    /// One-based row position, null for a synthetic outer row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinality: Option<String>,
    /// Output alias -> parent column.
    #[serde(default)]
    pub parent_columns: BTreeMap<String, String>,
    #[serde(default)]
    pub element: Option<String>,
    /// Legacy column mode: struct field names. Expression mode: scalar expressions.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}
impl CollectionSource {
    pub(crate) fn plan(
        &self,
        parent: datafusion::logical_expr::LogicalPlan,
        mapping: &super::mapping::GraphMapping,
    ) -> RelResult<datafusion::logical_expr::LogicalPlan> {
        use super::{col_exact, collections};
        use datafusion::{
            common::{Column, DFSchema, UnnestOptions},
            logical_expr::{Expr, ExprSchemable, LogicalPlanBuilder, lit},
        };
        use std::sync::Arc;
        let expression = match &self.expand {
            Some(text) => mapping.bind_scalar(text, parent.schema())?,
            None => col_exact(&self.column),
        };
        let ty = expression.get_type(parent.schema())?;
        let element = match &ty {
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                f.data_type()
            }
            _ => {
                return Err(RelError::Unsupported(format!(
                    "collection source `{}` expansion must return a list",
                    self.name
                )));
            }
        };
        let row_alias = self.row_alias.as_deref().unwrap_or("item");
        if self.expand.is_some() {
            if let (Expr::ScalarFunction(call), DataType::Struct(fields)) = (&expression, element) {
                let function = crate::ir::functions::logical::definition(&call.func)
                    .map(|f| f.logical_name().to_owned())
                    .unwrap_or_else(|| {
                        call.func
                            .aliases()
                            .iter()
                            .find(|alias| alias.contains('.'))
                            .cloned()
                            .unwrap_or_else(|| call.func.name().to_owned())
                    });
                let renamed = fields
                    .iter()
                    .map(|field| {
                        field.as_ref().clone().with_name(format!(
                            "__collection_{}_{}",
                            self.name,
                            field.name()
                        ))
                    })
                    .collect::<Vec<_>>();
                let aliases = fields
                    .iter()
                    .zip(&renamed)
                    .map(|(field, output)| (field.name().clone(), output.name().clone()))
                    .collect::<BTreeMap<_, _>>();
                let output = Arc::new(DFSchema::try_from(Schema::new(renamed))?);
                let position = self
                    .ordinality
                    .as_ref()
                    .map(|_| format!("__collection_{}_position", self.name));
                let function = super::dependent::TableFunction::new(
                    function.split('.').map(str::to_owned).collect(),
                    call.args.clone(),
                    Some(Arc::new(parent)),
                    super::dependent::ArgumentBinding::Correlated,
                    output,
                )?
                .with_native_list(expression)?
                .with_expansion(self.outer, position.clone())?;
                let expanded = function.into_plan();
                let mut output = self
                    .parent_columns
                    .iter()
                    .map(|(name, column)| col_exact(column).alias(name))
                    .collect::<Vec<_>>();
                for (name, text) in &self.fields {
                    output.push(
                        mapping
                            .bind_scalar(
                                &rewrite_fields(text, row_alias, &aliases)?,
                                expanded.schema(),
                            )?
                            .alias(name),
                    );
                }
                if let Some(name) = &self.ordinality {
                    output.push(col_exact(position.as_ref().unwrap()).alias(name));
                }
                if self.element.is_some() {
                    return Err(RelError::Unsupported("row functions expose fields; select the value field explicitly instead of element".into()));
                }
                return Ok(LogicalPlanBuilder::from(expanded)
                    .project(output)?
                    .build()?);
            }
        }
        // Ordinary native lists use the same UNNEST contract. No JSON-specific
        // execution or source adapter is introduced for these expressions.
        let mut projection = self
            .parent_columns
            .values()
            .enumerate()
            .map(|(i, column)| col_exact(column).alias(format!("__parent_{i}")))
            .collect::<Vec<_>>();
        projection.push(expression.alias(row_alias));
        let listed = LogicalPlanBuilder::from(parent)
            .project(projection)?
            .build()?;
        let mut projection = self
            .parent_columns
            .values()
            .enumerate()
            .map(|(i, _)| col_exact(format!("__parent_{i}")))
            .collect::<Vec<_>>();
        let list = col_exact(row_alias);
        projection.push(
            if self.outer {
                collections::outer_list(list.clone(), &ty)?
            } else {
                list.clone()
            }
            .alias(row_alias),
        );
        let mut columns = vec![Column::new_unqualified(row_alias)];
        if let Some(name) = &self.ordinality {
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
                    collections::outer_list(positions, &ty)?
                } else {
                    positions
                }
                .alias(name),
            );
            columns.push(Column::new_unqualified(name));
        }
        let input = LogicalPlanBuilder::from(listed)
            .project(projection)?
            .build()?;
        let input = collections::unnest_scope(
            input,
            format!("__w_sql_cte_collection_input_{}", self.name),
        )?;
        let input = collections::unnest_input(input)?;
        let expanded = LogicalPlanBuilder::from(input)
            .unnest_columns_with_options(
                columns,
                UnnestOptions {
                    preserve_nulls: self.outer,
                    ..Default::default()
                },
            )?
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
            output.push(col_exact(row_alias).alias(name));
        }
        for (name, field) in &self.fields {
            let expr = if self.expand.is_some() {
                mapping.bind_scalar(field, expanded.schema())?
            } else {
                datafusion::functions::core::expr_fn::get_field(
                    col_exact(row_alias),
                    field.as_str(),
                )
            };
            output.push(expr.alias(name));
        }
        if let Some(name) = &self.ordinality {
            output.push(col_exact(name));
        }
        Ok(LogicalPlanBuilder::from(expanded)
            .project(output)?
            .build()?)
    }
    pub(crate) fn validate(&self, schema: &Schema) -> RelResult<()> {
        let invalid = |message: &str| {
            RelError::Unsupported(format!("collection source `{}`: {message}", self.name))
        };
        if self.name.is_empty() || self.table.is_empty() || self.name == self.table {
            return Err(invalid("requires distinct nonempty source and table names"));
        }
        if self.column.is_empty() == self.expand.is_none() {
            return Err(invalid("choose exactly one column or expand expression"));
        }
        if self.row_alias.as_ref().is_some_and(String::is_empty) {
            return Err(invalid("row alias must be nonempty"));
        }
        for column in self.parent_columns.values() {
            schema
                .field_with_name(column)
                .map_err(|_| invalid("missing parent column"))?;
        }
        if self.element.is_some() && !self.fields.is_empty() {
            return Err(invalid("choose element or fields"));
        }
        if self.expand.is_none() {
            let field = schema
                .field_with_name(&self.column)
                .map_err(|_| invalid("missing list column"))?;
            let element = match field.data_type() {
                DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                    f.data_type()
                }
                _ => return Err(invalid("column must be a native list")),
            };
            match (&self.element, element) {
                (Some(_), _) if self.fields.is_empty() => {}
                (None, DataType::Struct(fields)) if !self.fields.is_empty() => {
                    for field in self.fields.values() {
                        if !fields.iter().any(|f| f.name() == field) {
                            return Err(invalid("missing element struct field"));
                        }
                    }
                }
                _ => {
                    return Err(invalid(
                        "choose element for list items or fields for struct lists",
                    ));
                }
            }
        }
        let mut names = BTreeSet::new();
        for name in self
            .parent_columns
            .keys()
            .chain(self.element.iter())
            .chain(self.fields.keys())
            .chain(self.ordinality.iter())
        {
            if name.is_empty() || !names.insert(name) {
                return Err(invalid("output column names must be nonempty and distinct"));
            }
        }
        Ok(())
    }
}
fn rewrite_fields(text: &str, alias: &str, fields: &BTreeMap<String, String>) -> RelResult<String> {
    use datafusion::sql::sqlparser::{
        ast, dialect::GenericDialect, parser::Parser, tokenizer::Token,
    };
    let mut parser = Parser::new(&GenericDialect {})
        .try_with_sql(text)
        .map_err(|e| RelError::Unsupported(e.to_string()))?;
    let mut expr = parser
        .parse_expr()
        .map_err(|e| RelError::Unsupported(e.to_string()))?;
    if parser.peek_token().token != Token::EOF {
        return Err(RelError::Unsupported(
            "collection field expressions cannot contain statements".into(),
        ));
    }
    let _ = ast::visit_expressions_mut(&mut expr, |expr| {
        if let ast::Expr::CompoundIdentifier(parts) = expr {
            if parts.len() == 2 && parts[0].value == alias {
                if let Some(name) = fields.get(&parts[1].value) {
                    *expr = ast::Expr::Identifier(ast::Ident::with_quote('"', name));
                }
            }
        }
        std::ops::ControlFlow::<()>::Continue(())
    });
    Ok(expr.to_string())
}
