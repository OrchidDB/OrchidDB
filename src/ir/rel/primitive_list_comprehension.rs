//! Ordered per-row primitive list reconstruction in a correlated scalar query.
//! Only total scalar expressions are admitted: SQL may reorder/repeat them.
use super::*;
use datafusion::common::ExprSchema;
use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::logical_expr::Subquery;

fn total(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(Lit::Null | Lit::Bool(_) | Lit::Int(_) | Lit::Float(_) | Lit::String(_))
        | IrExpr::Binding(_) => true,
        IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => total(e),
        IrExpr::Binary { op, lhs, rhs }
            if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Neq
                    | BinaryOp::Lt
                    | BinaryOp::Lte
                    | BinaryOp::Gt
                    | BinaryOp::Gte
                    | BinaryOp::And
                    | BinaryOp::Or
            ) =>
        {
            total(lhs) && total(rhs)
        }
        IrExpr::Call { name, args } if is_mod_function(name) => matches!(args.as_slice(),
            [value, IrExpr::Lit(Lit::Int(divisor))] if *divisor > 0 && total(value)),
        IrExpr::Call { name, args }
            if matches!(
                name.as_str(),
                "cypher_eq"
                    | "cypher_neq"
                    | "cypher_lt"
                    | "cypher_lte"
                    | "cypher_gt"
                    | "cypher_gte"
                    | "xor"
            ) =>
        {
            args.iter().all(total)
        }
        _ => false,
    }
}
fn primitive(ty: &DataType) -> bool {
    matches!(
        ty,
        DataType::Null
            | DataType::Boolean
            | DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float32
            | DataType::Float64
            | DataType::Utf8
            | DataType::LargeUtf8
            | DataType::Utf8View
    )
}
fn rename(expr: &mut IrExpr, item: &str, local: &str) {
    match expr {
        IrExpr::Binding(binding) if binding == item => *binding = local.to_owned(),
        IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => rename(e, item, local),
        IrExpr::Binary { lhs, rhs, .. } => {
            rename(lhs, item, local);
            rename(rhs, item, local);
        }
        IrExpr::Call { args, .. } => {
            for arg in args {
                rename(arg, item, local);
            }
        }
        _ => {}
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_primitive_list_comprehension(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        if self.options.native_values || !super::list_comparison::immutable_source_plan(plan) {
            return Ok(None);
        }
        let (mut source, mapping) = match expr {
            IrExpr::ListTransform { list, item, map } => {
                (list.as_ref(), Some((item.as_str(), map.as_ref())))
            }
            IrExpr::ListFilter { .. } => (expr, None),
            _ => return Ok(None),
        };
        let filtering = if let IrExpr::ListFilter {
            list,
            item,
            predicate,
        } = source
        {
            source = list;
            Some((item.as_str(), predicate.as_ref()))
        } else {
            None
        };
        if mapping.is_some_and(|(_, map)| !total(map))
            || filtering.is_some_and(|(_, pred)| !total(pred))
        {
            return Ok(None);
        }
        // Nested comprehensions keep their established evaluator until a
        // separate proof establishes evaluation and correlation at each level.
        if matches!(
            source,
            IrExpr::ListTransform { .. } | IrExpr::ListFilter { .. }
        ) {
            return Ok(None);
        }
        let list = self.lower_list_operand(plan, source)?;
        let DataType::List(element) = list.get_type(plan.schema())? else {
            return Ok(None);
        };
        if !primitive(element.data_type()) || !super::list_comparison::repeatable(plan, &list) {
            return Ok(None);
        }
        let nullable = super::list_comparison::list_nullable(plan, &list)?;
        let mut reserved = BTreeSet::new();
        for (name, _) in mapping.into_iter().chain(filtering) {
            reserved.insert(name.to_owned());
        }
        let mut fresh = |base: &str| {
            loop {
                let name = unique_internal_alias(plan, &reserved, base);
                reserved.insert(name.clone());
                let guard = format!("{name}_guard");
                if !has_exact_col(plan, &guard) && reserved.insert(guard) {
                    break name;
                }
            }
        };
        let local = fresh("__comprehension_item");
        let index = fresh("__comprehension_index");
        let scope_name = fresh("__comprehension_items");
        let input_scope = fresh("__comprehension_input");
        let output = fresh("__comprehension_result");
        let mut outer = Vec::new();
        let mut correlate = |expr: Expr| -> RelResult<Expr> {
            Ok(expr
                .transform_up(|expr| {
                    if let Expr::Column(column) = expr {
                        let field = plan.schema().field_from_column(&column)?.clone();
                        let reference = Expr::OuterReferenceColumn(field, column);
                        if !outer.contains(&reference) {
                            outer.push(reference.clone());
                        }
                        Ok(Transformed::yes(reference))
                    } else {
                        Ok(Transformed::no(expr))
                    }
                })?
                .data)
        };
        let correlated = correlate(list.clone())?;
        let indices = datafusion::functions_nested::expr_fn::gen_series(
            lit(1_i64),
            Expr::Cast(Cast::new(
                Box::new(datafusion::functions_nested::expr_fn::array_length(
                    correlated.clone(),
                )),
                DataType::Int64,
            )),
            lit(1_i64),
        );
        let listed = LogicalPlanBuilder::empty(true)
            .project(vec![indices.alias(&index)])?
            .build()?;
        let listed = collections::unnest_scope(listed, input_scope)?;
        let listed = collections::unnest_input(listed)?;
        let expanded = LogicalPlanBuilder::from(listed)
            .unnest_column_with_options(
                Column::new_unqualified(&index),
                datafusion::common::UnnestOptions {
                    preserve_nulls: false,
                    ..Default::default()
                },
            )?
            .build()?;
        // Inline only: a global CTE would lose the outer row's list binding.
        let expanded = collections::unnest_scope(expanded, scope_name)?;
        let value =
            datafusion::functions_nested::expr_fn::array_element(correlated, col_exact(&index));
        let mut fields = existing_columns(plan, &BTreeSet::new());
        fields.push(lit(ScalarValue::try_from(element.data_type())?).alias(&local));
        let virtual_scope = LogicalPlanBuilder::from(plan.clone())
            .project(fields)?
            .build()?;
        let mut lower = |item: &str, expr: &IrExpr, condition: bool| -> RelResult<Option<Expr>> {
            let mut expr = expr.clone();
            rename(&mut expr, item, &local);
            let expr = if condition {
                self.lower_case_condition(&virtual_scope, &expr)?
            } else {
                self.lower_expr(&virtual_scope, &expr)?
            };
            // This proof covers scalar outer bindings at one correlation
            // depth only. A list comparison can synthesize a nested subquery;
            // never capture its references while relocating the lambda.
            let mut scalar = true;
            expr.apply(|node| {
                if matches!(
                    node,
                    Expr::ScalarSubquery(_)
                        | Expr::Exists(_)
                        | Expr::InSubquery(_)
                        | Expr::OuterReferenceColumn(..)
                ) {
                    scalar = false;
                }
                if let Expr::Column(column) = node {
                    if !primitive(
                        virtual_scope
                            .schema()
                            .field_from_column(column)?
                            .data_type(),
                    ) {
                        scalar = false;
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !scalar {
                return Ok(None);
            }
            // Correlate all real outer bindings before substituting the local
            // element: its ordinal belongs to this subquery, not the caller.
            Ok(Some(
                expr.transform_up(|expr| {
                    if let Expr::Column(column) = expr {
                        if column.name == local {
                            return Ok(Transformed::yes(value.clone()));
                        }
                        let field = plan.schema().field_from_column(&column)?.clone();
                        let reference = Expr::OuterReferenceColumn(field, column);
                        if !outer.contains(&reference) {
                            outer.push(reference.clone());
                        }
                        Ok(Transformed::yes(reference))
                    } else {
                        Ok(Transformed::no(expr))
                    }
                })?
                .data,
            ))
        };
        let expanded = if let Some((item, predicate)) = filtering {
            let Some(predicate) = lower(item, predicate, true)? else {
                return Ok(None);
            };
            if !matches!(
                predicate.get_type(expanded.schema())?,
                DataType::Boolean | DataType::Null
            ) {
                return Ok(None);
            }
            LogicalPlanBuilder::from(expanded)
                .filter(Expr::IsTrue(Box::new(predicate)))?
                .build()?
        } else {
            expanded
        };
        let mapped = if let Some((item, map)) = mapping {
            let Some(mapped) = lower(item, map, false)? else {
                return Ok(None);
            };
            mapped
        } else {
            value
        };
        let ty = mapped.get_type(expanded.schema())?;
        if !primitive(&ty) {
            return Ok(None);
        }
        let collected = df_array_agg(mapped)
            .order_by(vec![col_exact(&index).sort(true, false)])
            .build()?
            .alias(&output);
        let aggregate = LogicalPlanBuilder::from(expanded)
            .aggregate(Vec::<Expr>::new(), vec![collected])?
            .build()?;
        let empty = lit(ScalarValue::List(ScalarValue::new_list_nullable(&[], &ty)));
        let reconstructed = df_core::coalesce(vec![col_exact(&output), empty]);
        let query = LogicalPlanBuilder::from(aggregate)
            .project(vec![reconstructed.alias(&output)])?
            .build()?;
        let result_type = query.schema().field(0).data_type().clone();
        let scalar = Expr::ScalarSubquery(Subquery {
            subquery: Arc::new(query),
            outer_ref_columns: outer,
            spans: Default::default(),
        });
        Ok(Some(if nullable {
            datafusion::logical_expr::when(
                list.is_null(),
                lit(ScalarValue::try_from(&result_type)?),
            )
            .otherwise(scalar)?
        } else {
            scalar
        }))
    }
}
