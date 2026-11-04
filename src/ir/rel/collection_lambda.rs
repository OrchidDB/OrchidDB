//! Compiler-only list lambdas. The body is a typed expression, not a program
//! passed to a runtime UDF; adapters emit native SQL lambda syntax.
//!
//! SQL audit [L14](../../../tools/diagnostics/sql-lowering-audit.md#l14-native-sql-list-lambdas): This native-values route precedes scalar relational comprehensions.
//! A dialect lacking lambda emission does not automatically retry that alternative.
use super::*;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::{
    any::Any,
    hash::{Hash, Hasher},
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Lambda {
    pub name: String,
    pub kind: String,
    pub parameter: String,
    pub accumulator: Option<String>,
    pub body: Expr,
    pub body_schema: datafusion::common::DFSchemaRef,
    pub captures: Vec<String>,
    pub result: DataType,
    pub source: DataType,
    pub total_body: bool,
    signature: Signature,
}
impl ScalarUDFImpl for Lambda {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(self.result.clone())
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::NotImplemented(
            "List lambda is emitted as native SQL".into(),
        ))
    }
}
pub(super) fn definition(function: &ScalarUDF) -> Option<&Lambda> {
    function.inner().as_any().downcast_ref::<Lambda>()
}

impl LoweringContext<'_> {
    // Scalar list consumers can retain a native list without invoking the
    // legacy display/reconstruction alternative for a returned collection.
    pub(super) fn lower_typed_list_input(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        self.lower_expr(plan, expr).or_else(|error| {
            if !matches!(error, RelError::Semantic(_))
                && matches!(expr, IrExpr::ListTransform { .. } | IrExpr::ListFilter { .. }) {
                self.lower_sql_lambda(plan, expr)
            } else { Err(error) }
        })
    }
    /// Consumers of total, row-local mappings need not construct mapped lists.
    /// Keep potentially throwing/volatile bodies on the ordinary evaluation path.
    pub(super) fn lower_list_consumer(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Option<Expr>> {
        let IrExpr::Call { name, args } = expr else { return Ok(None); };
        let [input] = args.as_slice() else { return Ok(None); };
        let simplified_head = name == "head" && matches!(input,
            IrExpr::ListTransform { item, map, .. }
                if matches!(map.as_ref(), IrExpr::Binding(binding) if binding == item)
                    || matches!(map.as_ref(), IrExpr::Lit(Lit::Null | Lit::Bool(_) | Lit::Int(_) | Lit::Float(_) | Lit::String(_))));
        if matches!(name.as_str(), "head" | "last") && !simplified_head {
            let source = self.lower_typed_list_input(plan, input)?;
            if matches!(source.get_type(plan.schema())?, DataType::List(_)) {
                return Ok(Some(datafusion::functions_nested::expr_fn::array_element(
                    source, lit(if name == "head" { 1_i64 } else { -1_i64 }),
                )));
            }
        }
        if name == "size" && matches!(input, IrExpr::ListFilter { .. }) {
            let Expr::ScalarFunction(call) = self.lower_sql_lambda(plan, input)? else { return Ok(None); };
            let Some(mut lambda) = definition(&call.func).cloned() else { return Ok(None); };
            lambda.kind = "count".into();
            lambda.result = DataType::Int64;
            lambda.name.push_str("_count");
            return Ok(Some(ScalarUDF::from(lambda).call(call.args)));
        }
        let IrExpr::ListTransform { list, item, map } = input else { return Ok(None); };
        if !matches!(name.as_str(), "size" | "head") { return Ok(None); }
        let identity = matches!(map.as_ref(), IrExpr::Binding(binding) if binding == item);
        let constant = matches!(map.as_ref(), IrExpr::Lit(Lit::Null | Lit::Bool(_) | Lit::Int(_) | Lit::Float(_) | Lit::String(_)));
        if !identity && !constant {
            if name == "size" {
                // Evaluate the mapped values before counting: the body may
                // throw, and counting only the source would hide that error.
                let mapped = self.lower_sql_lambda(plan, input)?;
                return Ok(Some(datafusion::functions_nested::expr_fn::array_length(mapped)
                    .cast_to(&DataType::Int64, plan.schema())?));
            }
            return Ok(None);
        }
        // A filter can be counted directly, even through the frontend's identity map.
        if name == "size" && matches!(list.as_ref(), IrExpr::ListFilter { .. }) {
            return self.lower_list_consumer(plan, &IrExpr::Call { name: name.clone(), args: vec![*list.clone()] });
        }
        let source = self.lower_expr(plan, list)?;
        if !matches!(source.get_type(plan.schema())?, DataType::List(_)) { return Ok(None); }
        let length = datafusion::functions_nested::expr_fn::array_length(source.clone());
        if name == "size" { return Ok(Some(length.cast_to(&DataType::Int64, plan.schema())?)); }
        if identity {
            return Ok(Some(datafusion::functions_nested::expr_fn::array_element(source, lit(1_i64))));
        }
        let value = self.lower_expr(plan, map)?;
        Ok(Some(Expr::Case(Case::new(None, vec![(Box::new(length.gt(lit(0_u64))), Box::new(value))], None))))
    }

    pub(super) fn lower_sql_lambda(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        let sum_item = IrExpr::Binding("__sum_item".into());
        let (kind, list, item, body) = match expr {
            IrExpr::Call { name, args } if name == "list_sum" && args.len() == 1 => {
                ("sum", &args[0], "__sum_item", &sum_item)
            }
            IrExpr::ListReduce { collection, item, map, .. } => {
                ("reduce", collection.as_ref(), item.as_str(), map.as_ref())
            }
            IrExpr::ListTransform { list, item, map } => {
                ("transform", list.as_ref(), item.as_str(), map.as_ref())
            }
            IrExpr::ListFilter {
                list,
                item,
                predicate,
            } => ("filter", list.as_ref(), item.as_str(), predicate.as_ref()),
            IrExpr::Quantifier {
                kind,
                list,
                item,
                predicate,
            } => (
                match kind {
                    QuantifierKind::All => "all",
                    QuantifierKind::Any => "any",
                    QuantifierKind::None => "none",
                    QuantifierKind::Single => "single",
                },
                list.as_ref(),
                item.as_str(),
                predicate.as_ref(),
            ),
            _ => return Err(RelError::Unsupported("Expected list lambda".into())),
        };
        let source = self.lower_typed_list_input(plan, list)?;
        let source_type = source.get_type(plan.schema())?;
        let source = if kind == "sum" && (source_type == DataType::Null
            || matches!(&source_type, DataType::List(field) if field.data_type() == &DataType::Null)) {
            source.cast_to(&DataType::List(Arc::new(Field::new("item", DataType::Int64, true))), plan.schema())?
        } else { source };
        // An untyped empty/all-null list must not acquire a VARCHAR element
        // type from a dialect's empty-array literal. Its elements retain the
        // Cypher null domain until the body establishes its result type.
        let kind_type = source.get_type(plan.schema())?;
        let source = if !matches!(&kind_type, DataType::List(field) if field.data_type() != &DataType::Null) {
            native_values::typed::call(
                if matches!(kind, "transform" | "filter" | "reduce") {
                    "list"
                } else {
                    "quantifier_list"
                },
                vec![source],
            )
        } else {
            source
        };
        let source_type = source.get_type(plan.schema())?;
        let DataType::List(element) = &source_type else {
            return Err(RelError::Unsupported(
                "List lambda requires a typed list".into(),
            ));
        };
        if kind == "transform" && matches!(body, IrExpr::Binding(binding) if binding == item) {
            return Ok(source);
        }
        let parameter = unique_internal_alias(plan, &BTreeSet::new(), "__orchid_lambda_item");
        let accumulator = if let IrExpr::ListReduce { accumulator, .. } = expr {
            Some((accumulator.as_str(), unique_internal_alias(plan,
                &BTreeSet::from([parameter.clone()]), "__orchid_lambda_accumulator")))
        } else { None };
        let mut columns = existing_columns(plan, &BTreeSet::new());
        columns.push(lit(ScalarValue::try_from(element.data_type())?).alias(&parameter));
        if let Some((_, accumulator)) = &accumulator {
            columns.push(lit(ScalarValue::try_from(element.data_type())?).alias(accumulator));
        }
        let schema = LogicalPlanBuilder::from(plan.clone())
            .project(columns)?
            .build()?
            .schema()
            .clone();
        // A lambda parameter is an unknown input, not the NULL used to declare
        // its type. Constant-domain analysis must not fold its body to NULL.
        let scope = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
            produce_one_row: false,
            schema,
        });
        let mut body = body.clone();
        crate::ir::rules::graph::walk_expr(&mut body, &Default::default(), &mut |part, scope| {
            match part {
                IrExpr::Binding(binding)
                | IrExpr::Property { binding, .. }
                | IrExpr::Id(binding)
                | IrExpr::Label(binding)
                | IrExpr::HasLabel { binding, .. }
                | IrExpr::IsBound(binding)
                    if binding == item && !scope.is_bound(item) =>
                { *binding = parameter.clone(); }
                IrExpr::Binding(binding) | IrExpr::Property { binding, .. }
                | IrExpr::Id(binding) | IrExpr::Label(binding)
                | IrExpr::HasLabel { binding, .. } | IrExpr::IsBound(binding)
                    if accumulator.as_ref().is_some_and(|(name, _)| binding == name && !scope.is_bound(name)) =>
                { *binding = accumulator.as_ref().unwrap().1.clone(); }
                _ => {}
            }
        });
        let source_body = body;
        let body = if kind == "reduce" && native_values::is_value(element.data_type()) {
            self.native_expr(&scope, &source_body)?
        } else if matches!(kind, "transform" | "reduce" | "sum") {
            self.lower_expr(&scope, &source_body)?
        } else {
            self.lower_case_condition(&scope, &source_body)?
        };
        // DuckDB forbids scalar subqueries inside lambda bodies. Comparisons
        // that normally bind operands in a scalar SELECT use the focused
        // typed comparison kernel here; the list iteration remains native SQL.
        let body = if body.exists(|expr| {
            Ok(matches!(
                expr,
                Expr::ScalarSubquery(_)
                    | Expr::Exists(_)
                    | Expr::InSubquery(_)
                    | Expr::SetComparison(_)
            ))
        })? {
            self.native_expr(&scope, &source_body)?
        } else {
            body
        };
        if kind == "reduce" && !native_values::is_value(element.data_type())
            && body.get_type(scope.schema())? != *element.data_type()
        {
            // SQL folds use the first element as their accumulator type. A
            // Cypher body may change that type; carry both items and accumulator
            // losslessly instead of letting SQL cast the result back to the seed.
            let source = native_values::typed::call("items", vec![
                native_values::typed::call("pack", vec![source]),
            ]);
            let binding = unique_internal_alias(plan, &BTreeSet::new(), "__reduce_typed_input");
            let mut columns = existing_columns(plan, &BTreeSet::new());
            columns.push(source.clone().alias(&binding));
            let typed_scope = LogicalPlanBuilder::from(plan.clone()).project(columns)?.build()?;
            let IrExpr::ListReduce { accumulator, item, map, .. } = expr else { unreachable!() };
            let rewritten = IrExpr::ListReduce {
                collection: Box::new(IrExpr::Binding(binding.clone())),
                accumulator: accumulator.clone(), item: item.clone(), map: map.clone(),
            };
            return Ok(self.lower_sql_lambda(&typed_scope, &rewritten)?.transform_up(|expr| {
                if matches!(&expr, Expr::Column(column) if column.name == binding) {
                    Ok(Transformed::yes(source.clone()))
                } else { Ok(Transformed::no(expr)) }
            })?.data);
        }
        let body = if kind == "reduce" && native_values::is_value(element.data_type())
            && !native_values::is_value(&body.get_type(scope.schema())?) {
            native_values::typed::call("pack", vec![body])
        } else { body };
        let result = match kind {
            "sum" => match element.data_type() {
                DataType::Null | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => DataType::Int64,
                DataType::Float32 | DataType::Float64 => DataType::Float64,
                other => return Err(RelError::Unsupported(format!("list_sum requires a signed integer or floating-point list, got {other}"))),
            },
            "transform" => DataType::List(Arc::new(Field::new(
                "item",
                body.get_type(scope.schema())?,
                true,
            ))),
            "filter" => source_type.clone(),
            "reduce" => body.get_type(scope.schema())?,
            _ => DataType::Boolean,
        };
        let total_body = constraints::harmless_expression(&body, scope.schema());
        let mut args = vec![source];
        let mut captures = BTreeMap::new();
        let mut capture_names = Vec::new();
        let mut reserved = BTreeSet::from([parameter.clone()]);
        if let Some((_, name)) = &accumulator { reserved.insert(name.clone()); }
        let body = body
            .transform_up(|expr| {
                if let Expr::Column(column) = &expr {
                    if column.name == parameter || accumulator.as_ref().is_some_and(|(_, name)| &column.name == name) {
                        return Ok(Transformed::no(expr));
                    }
                    let next = args.len();
                    let index = *captures.entry(column.clone()).or_insert_with(|| {
                        args.push(expr.clone());
                        let name = unique_internal_alias(
                            &scope,
                            &reserved,
                            &format!("{parameter}_capture_{next}"),
                        );
                        reserved.insert(name.clone());
                        capture_names.push(name);
                        next
                    });
                    return Ok(Transformed::yes(col_exact(
                        capture_names[index - 1].clone(),
                    )));
                }
                Ok(Transformed::no(expr))
            })?
            .data;
        let mut body_schema = scope.schema().as_ref().clone();
        for (name, argument) in capture_names.iter().zip(args.iter().skip(1)) {
            body_schema.merge(&datafusion::common::DFSchema::try_from(Schema::new(vec![
                Field::new(name, argument.get_type(plan.schema())?, true),
            ]))?);
        }
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (&kind, &parameter, &accumulator, &body, &result).hash(&mut hash);
        let function = Lambda {
            name: format!("__orchid_sql_lambda_{:x}", hash.finish()),
            kind: kind.into(),
            parameter,
            accumulator: accumulator.map(|(_, name)| name),
            body,
            body_schema: Arc::new(body_schema),
            captures: capture_names,
            result,
            source: source_type,
            total_body,
            signature: Signature::any(args.len(), Volatility::Volatile),
        };
        Ok(ScalarUDF::from(function).call(args))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/collection_lambda_boundaries.rs"]
mod boundary_tests;
