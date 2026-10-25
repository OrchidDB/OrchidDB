//! Compiler-only list lambdas. The body is a typed expression, not a program
//! passed to a runtime UDF; adapters emit native SQL lambda syntax.
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
    pub body: Expr,
    pub captures: Vec<String>,
    pub result: DataType,
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
    /// Consumers of total, row-local mappings need not construct mapped lists.
    /// Keep potentially throwing/volatile bodies on the ordinary evaluation path.
    pub(super) fn lower_list_consumer(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Option<Expr>> {
        let IrExpr::Call { name, args } = expr else { return Ok(None); };
        let [input] = args.as_slice() else { return Ok(None); };
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
        if !identity && !constant { return Ok(None); }
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
        let (kind, list, item, body) = match expr {
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
        let source = self.lower_expr(plan, list)?;
        let source = if !matches!(source.get_type(plan.schema())?, DataType::List(_)) {
            native_values::typed::call(
                if matches!(kind, "transform" | "filter") {
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
        let mut columns = existing_columns(plan, &BTreeSet::new());
        columns.push(lit(ScalarValue::try_from(element.data_type())?).alias(&parameter));
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
            if scope.is_bound(item) {
                return;
            }
            match part {
                IrExpr::Binding(binding)
                | IrExpr::Property { binding, .. }
                | IrExpr::Id(binding)
                | IrExpr::Label(binding)
                | IrExpr::HasLabel { binding, .. }
                | IrExpr::IsBound(binding)
                    if binding == item =>
                {
                    *binding = parameter.clone()
                }
                _ => {}
            }
        });
        let source_body = body;
        let body = if kind == "transform" {
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
        let result = match kind {
            "transform" => DataType::List(Arc::new(Field::new(
                "item",
                body.get_type(scope.schema())?,
                true,
            ))),
            "filter" => source_type,
            _ => DataType::Boolean,
        };
        let total_body = constraints::harmless_expression(&body, scope.schema());
        let mut args = vec![source];
        let mut captures = BTreeMap::new();
        let mut capture_names = Vec::new();
        let mut reserved = BTreeSet::from([parameter.clone()]);
        let body = body
            .transform_up(|expr| {
                if let Expr::Column(column) = &expr {
                    if column.name == parameter {
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
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        (&kind, &parameter, &body, &result).hash(&mut hash);
        let function = Lambda {
            name: format!("__orchid_sql_lambda_{:x}", hash.finish()),
            kind: kind.into(),
            parameter,
            body,
            captures: capture_names,
            result,
            total_body,
            signature: Signature::any(args.len(), Volatility::Volatile),
        };
        Ok(ScalarUDF::from(function).call(args))
    }
}
