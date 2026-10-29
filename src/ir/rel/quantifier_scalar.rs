//! Per-row quantification over a primitive list, without a global row identity.
//! Total primitive scalar predicates may be repeated by aggregate SQL; effectful
//! expressions retain the general evaluation path. Nested quantifiers preserve
//! their lexical scope as scalar subqueries, without occurrence joins.
//!
//! SQL audit [L15](../../../tools/diagnostics/sql-lowering-audit.md#l15-quantifiers-and-cardinality-without-list-construction): Scalar correlated reductions avoid a global occurrence window only
//! with repeatable inputs and total typed predicates; retain the null truth table.
use super::*;
use arrow::array::Array;
use datafusion::common::ExprSchema;
use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
use datafusion::logical_expr::Subquery;

fn primitive(ty: &DataType) -> bool {
    matches!(ty, DataType::Null | DataType::Boolean | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Float32 | DataType::Float64 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
}

fn scalar_collection(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Binding(_) | IrExpr::Lit(_) => true,
        IrExpr::List(items) => items.iter().all(scalar_collection),
        _ => false,
    }
}

fn list_element(ty: &DataType) -> bool {
    primitive(ty) || matches!(ty, DataType::Null)
        || matches!(ty, DataType::List(field) if list_element(field.data_type()))
}

fn length_function(name: &str) -> bool {
    matches!(name, "size" | "length" | "char_length" | "character_length")
}

// Length is total only for the types accepted by Cypher. In particular,
// SQL's permissive numeric-to-string cast must not justify this fast path.
fn length_operand(expr: &IrExpr, plan: &LogicalPlan, allow_list: bool) -> RelResult<bool> {
    let accepted = |ty: &DataType| matches!(ty,
        DataType::Null | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
        || (allow_list && matches!(ty,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)));
    Ok(match expr {
        IrExpr::Binding(name) => accepted(plan.schema().field_from_column(&Column::new_unqualified(name))?.data_type()),
        IrExpr::Lit(Lit::Null | Lit::String(_)) => true,
        IrExpr::Lit(Lit::Scalar(value)) => accepted(&value.data_type()),
        IrExpr::List(items) if allow_list => items.iter().all(scalar_collection),
        _ => false,
    })
}

fn contains_length(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Call { name, args } => length_function(name) || args.iter().any(contains_length),
        IrExpr::Not(expr) | IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) => contains_length(expr),
        IrExpr::Binary { lhs, rhs, .. } => contains_length(lhs) || contains_length(rhs),
        _ => false,
    }
}

// This is only a syntactic candidate filter. Typed length checks and checked
// arithmetic interval proofs below must pass before the scalar path is used.
fn candidate_predicate(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(Lit::Null | Lit::Bool(_) | Lit::Int(_) | Lit::Float(_) | Lit::String(_)) => true,
        IrExpr::Binding(_) => true,
        IrExpr::Quantifier { list, predicate, .. } => scalar_collection(list) && candidate_predicate(predicate),
        IrExpr::Not(expr) | IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) => candidate_predicate(expr),
        IrExpr::Binary { op, lhs, rhs } if matches!(op, BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte | BinaryOp::And | BinaryOp::Or | BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) => candidate_predicate(lhs) && candidate_predicate(rhs),
        IrExpr::Call { name, args } if length_function(name) =>
            matches!(args.as_slice(), [value] if scalar_collection(value)),
        IrExpr::Call { name, args } if name == "abs" =>
            matches!(args.as_slice(), [value] if candidate_predicate(value)),
        IrExpr::Call { name, args } if is_mod_function(name) => matches!(args.as_slice(),
            [value, divisor] if candidate_predicate(value) && candidate_predicate(divisor)
                && !matches!(divisor, IrExpr::Lit(Lit::Int(value)) if *value <= 0)),
        IrExpr::Call { name, args } if matches!(name.as_str(), "cypher_eq" | "cypher_neq" | "cypher_lt" | "cypher_lte" | "cypher_gt" | "cypher_gte" | "xor") => args.iter().all(|arg| candidate_predicate(arg)),
        _ => false,
    }
}

fn rename_item(expr: &mut IrExpr, item: &str, local: &str) {
    match expr {
        IrExpr::Binding(binding) if binding == item => *binding = local.to_owned(),
        IrExpr::Not(expr) | IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) => rename_item(expr,item,local),
        IrExpr::Binary { lhs,rhs,.. } => { rename_item(lhs,item,local); rename_item(rhs,item,local); }
        IrExpr::Call { args,.. } | IrExpr::List(args) => { for arg in args { rename_item(arg,item,local); } }
        IrExpr::Quantifier { list, item: nested_item, predicate, .. } => {
            // The collection belongs to the enclosing scope. Only the body
            // introduces (and may shadow) the nested iterator.
            rename_item(list,item,local);
            if nested_item != item { rename_item(predicate,item,local); }
        }
        _ => {}
    }
}

// Reserve user lexical names as well as row columns: an internal alias must
// never be captured by a nested iterator bearing an internal-looking name.
fn lexical_names(expr: &IrExpr, names: &mut BTreeSet<String>) {
    match expr {
        IrExpr::Binding(name) => { names.insert(name.clone()); }
        IrExpr::Quantifier { list, item, predicate, .. } => {
            names.insert(item.clone()); lexical_names(list,names); lexical_names(predicate,names);
        }
        IrExpr::Not(expr) | IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) => lexical_names(expr,names),
        IrExpr::Binary { lhs,rhs,.. } => { lexical_names(lhs,names); lexical_names(rhs,names); }
        IrExpr::Call { args,.. } | IrExpr::List(args) => for arg in args { lexical_names(arg,names); },
        _ => {}
    }
}

fn primitive_predicate_bindings(expr: &IrExpr, plan: &LogicalPlan) -> RelResult<bool> {
    Ok(match expr {
        IrExpr::Binding(name) => primitive(plan.schema().field_from_column(&Column::new_unqualified(name))?.data_type()),
        // Each child validates its own scalar body after introducing its item.
        IrExpr::Quantifier { .. } => true,
        IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) if matches!(expr.as_ref(), IrExpr::Binding(_)) => true,
        IrExpr::Not(expr) | IrExpr::IsNull(expr) | IrExpr::IsNotNull(expr) => primitive_predicate_bindings(expr,plan)?,
        IrExpr::Binary { lhs,rhs,.. } => primitive_predicate_bindings(lhs,plan)? && primitive_predicate_bindings(rhs,plan)?,
        IrExpr::Call { name, args } if length_function(name) => match args.as_slice() {
            [value] => length_operand(value,plan,name == "size")?,
            _ => false,
        },
        IrExpr::Call { name, args } if matches!(name.as_str(), "cypher_eq" | "cypher_neq" | "cypher_lt" | "cypher_lte" | "cypher_gt" | "cypher_gte") => {
            let mut valid=true;
            for arg in args {
                valid &= if let IrExpr::Binding(name) = arg {
                    list_element(plan.schema().field_from_column(&Column::new_unqualified(name))?.data_type())
                } else { primitive_predicate_bindings(arg,plan)? };
            }
            valid
        }
        IrExpr::Call { args,.. } => {
            let mut valid=true;
            for arg in args { valid &= primitive_predicate_bindings(arg,plan)?; }
            valid
        }
        _ => true,
    })
}

impl LoweringContext<'_> {
    pub(super) fn lower_scalar_quantifier(
        &self, plan: &LogicalPlan, kind: QuantifierKind, item: &str,
        list: Expr, predicate: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        self.lower_scalar_collection_reduce(plan, Some(kind), item, list, predicate)
    }

    pub(super) fn lower_scalar_filtered_count(
        &self, plan: &LogicalPlan, item: &str, list: Expr, predicate: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        self.lower_scalar_collection_reduce(plan, None, item, list, predicate)
    }

    fn lower_scalar_collection_reduce(
        &self, plan: &LogicalPlan, kind: Option<QuantifierKind>, item: &str,
        list: Expr, predicate: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        if self.options.native_values || !candidate_predicate(predicate)
            || !super::list_comparison::immutable_source_plan(plan)
            || !super::list_comparison::repeatable(plan,&list) { return Ok(None); }
        let DataType::List(element) = list.get_type(plan.schema())? else { return Ok(None); };
        if !list_element(element.data_type()) { return Ok(None); }
        // List nullability is distinct from nullable elements. A non-null
        // constructor may contain NULL items and still needs unknown counting.
        let nullable_list = super::list_comparison::list_nullable(plan,&list)?;
        let mut outer = Vec::new();
        let correlated = list.clone().transform_up(|expr| {
            if let Expr::Column(column) = expr {
                let field = plan.schema().field_from_column(&column)?.clone();
                let reference = Expr::OuterReferenceColumn(field,column);
                outer.push(reference.clone());
                return Ok(Transformed::yes(reference));
            }
            Ok(Transformed::no(expr))
        })?.data;
        let mut reserved = BTreeSet::from([item.to_owned()]);
        lexical_names(predicate, &mut reserved);
        let mut fresh = |base: &str| {
            loop {
                let name = unique_internal_alias(plan,&reserved,base);
                reserved.insert(name.clone());
                let guard = format!("{name}_guard");
                if !has_exact_col(plan,&guard) && reserved.insert(guard) { break name; }
            }
        };
        let local_item = fresh("__quantifier_item");
        let input_scope = fresh("__quantifier_input");
        let item_scope = fresh("__quantifier_items");
        let yes = fresh("__quantifier_true");
        let no = fresh("__quantifier_false");
        let unknown = fresh("__quantifier_null");
        let result = fresh("__quantifier_result");
        let predicate_name = fresh("__quantifier_predicate");
        let predicate_scope = fresh("__quantifier_predicates");
        // Inline aliases are essential: hoisting these to global CTEs would
        // detach the correlated list from its original input occurrence.
        let listed = LogicalPlanBuilder::empty(true).project(vec![correlated.alias(&local_item)])?.build()?;
        // Preserve the list alias before UNNEST: its SQL emitter substitutes
        // projection expressions and otherwise loses the local item name.
        // This inline boundary needs no additional guard column.
        let listed = LogicalPlanBuilder::from(listed).alias(input_scope)?.build()?;
        let listed = collections::unnest_input(listed)?;
        let expanded = LogicalPlanBuilder::from(listed).unnest_column_with_options(
            Column::new_unqualified(&local_item), datafusion::common::UnnestOptions { preserve_nulls:false, ..Default::default() })?.build()?;
        let expanded = collections::unnest_scope(expanded,item_scope)?;
        // Keep a user item named like its outer list from capturing the
        // correlated reference in SQL (any(xs IN xs WHERE ...)).
        let mut predicate = predicate.clone();
        rename_item(&mut predicate,item,&local_item);
        // Resolve user bindings before entering the scalar query. Its schema
        // contains only the element; real outer bindings become explicit refs.
        let mut fields = existing_columns(plan,&BTreeSet::new());
        fields.push(list.clone().alias(&local_item));
        // This analysis-only expansion preserves the iterator's actual domain
        // for checked arithmetic proofs. It is never emitted or executed.
        let virtual_scope = LogicalPlanBuilder::from(plan.clone()).project(fields)?
            .unnest_column_with_options(Column::new_unqualified(&local_item),
                datafusion::common::UnnestOptions { preserve_nulls: false, ..Default::default() })?
            .build()?;
        if !primitive_predicate_bindings(&predicate,&virtual_scope)? { return Ok(None); }
        let evaluate_once = contains_length(&predicate);
        let predicate = self.lower_case_condition(&virtual_scope,&predicate)?;
        if !super::list_comparison::arithmetic_is_total(&virtual_scope,&predicate) { return Ok(None); }
        let mut scalar = true;
        let mut uses_item = false;
        let mut nested = false;
        predicate.apply(|expr| {
            if let Expr::ScalarSubquery(query) = expr {
                // Nested quantifiers and validated list equality may introduce
                // scalar subqueries here. Their own local refs stay
                // inside the child query; refs to this iterator bind here.
                nested = true;
                for reference in &query.outer_ref_columns {
                    if let Expr::OuterReferenceColumn(_, column) = reference {
                        if column.name == local_item { uses_item = true; }
                        else if !outer.contains(reference) { outer.push(reference.clone()); }
                    }
                }
            }
            if matches!(expr, Expr::Exists(_) | Expr::InSubquery(_) | Expr::SetComparison(_) | Expr::OuterReferenceColumn(..)) {
                scalar = false;
            }
            if let Expr::Column(column) = expr {
                uses_item |= column.name == local_item;
                scalar &= list_element(virtual_scope.schema().field_from_column(column)?.data_type());
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        if !scalar { return Ok(None); }
        if !uses_item && kind.is_some() {
            let kind = kind.unwrap();
            // A total predicate independent of the iterator has the same
            // truth value for every item. Only cardinality matters: do not
            // expand and regroup a potentially huge list to repeat it.
            // Keeping this in the caller's scope also avoids SQL binding an
            // outer-only aggregate at the wrong query level.
            let length = datafusion::functions_nested::expr_fn::array_length(list.clone());
            let null = lit(ScalarValue::Boolean(None));
            let nonempty = match kind {
                QuantifierKind::All | QuantifierKind::Any => predicate,
                QuantifierKind::None => Expr::Not(Box::new(predicate)),
                QuantifierKind::Single => datafusion::logical_expr::when(
                    Expr::IsFalse(Box::new(predicate.clone())), lit(false))
                    .when(predicate.is_null(), null.clone())
                    .otherwise(length.clone().eq(lit(1_u64)))?,
            };
            let value = datafusion::logical_expr::when(length.eq(lit(0_u64)),
                lit(matches!(kind, QuantifierKind::All | QuantifierKind::None)))
                .otherwise(nonempty)?;
            return Ok(Some(if nullable_list {
                datafusion::logical_expr::when(list.is_null(), null).otherwise(value)?
            } else { value }));
        }
        let predicate = predicate.transform_up(|expr| {
            if let Expr::Column(column) = expr {
                if column.name == local_item {
                    return Ok(Transformed::yes(col_exact(&local_item)));
                }
                let field = plan.schema().field_from_column(&column)?.clone();
                let reference = Expr::OuterReferenceColumn(field,column);
                if !outer.contains(&reference) { outer.push(reference.clone()); }
                return Ok(Transformed::yes(reference));
            }
            Ok(Transformed::no(expr))
        })?.data;
        // Evaluate nested queries and length predicates once per item.
        // Aggregate their Boolean result rather than duplicating list/string
        // traversal in each truth/unknown count (and SINGLE's result test).
        let (expanded, predicate) = if nested || evaluate_once {
            let projected = LogicalPlanBuilder::from(expanded)
                .project(vec![predicate.alias(&predicate_name)])?.build()?;
            let scoped = collections::unnest_scope(projected, predicate_scope)?;
            (scoped, col_exact(&predicate_name))
        } else { (expanded, predicate) };
        let indicator = |condition| datafusion::logical_expr::when(condition,lit(1_i64))
            .otherwise(lit(ScalarValue::Int64(None)));
        let mut aggregates = if kind.is_some() { vec![df_count(indicator(predicate.clone().is_null())?).alias(&unknown)] } else { vec![] };
        let decisive = if matches!(kind,Some(QuantifierKind::All)) {
            aggregates.push(df_count(indicator(Expr::IsFalse(Box::new(predicate)))?).alias(&no));
            col_exact(&no)
        } else {
            aggregates.push(df_count(indicator(Expr::IsTrue(Box::new(predicate)))?).alias(&yes));
            col_exact(&yes)
        };
        let aggregate = LogicalPlanBuilder::from(expanded).aggregate(Vec::<Expr>::new(),aggregates)?.build()?;
        let null = lit(if kind.is_some() { ScalarValue::Boolean(None) } else { ScalarValue::Int64(None) });
        let truth = match kind {
            None => decisive,
            Some(QuantifierKind::All) => datafusion::logical_expr::when(decisive.gt(lit(0_i64)),lit(false))
                .when(col_exact(&unknown).gt(lit(0_i64)),null.clone()).otherwise(lit(true))?,
            Some(QuantifierKind::Any) => datafusion::logical_expr::when(decisive.gt(lit(0_i64)),lit(true))
                .when(col_exact(&unknown).gt(lit(0_i64)),null.clone()).otherwise(lit(false))?,
            Some(QuantifierKind::None) => datafusion::logical_expr::when(decisive.gt(lit(0_i64)),lit(false))
                .when(col_exact(&unknown).gt(lit(0_i64)),null.clone()).otherwise(lit(true))?,
            Some(QuantifierKind::Single) => datafusion::logical_expr::when(decisive.clone().gt(lit(1_i64)),lit(false))
                .when(col_exact(&unknown).gt(lit(0_i64)),null.clone()).otherwise(decisive.eq(lit(1_i64)))?,
        };
        let subquery = LogicalPlanBuilder::from(aggregate).project(vec![truth.alias(&result)])?.build()?;
        let value = Expr::ScalarSubquery(Subquery { subquery:Arc::new(subquery), outer_ref_columns:outer, spans:Default::default() });
        Ok(Some(if nullable_list {
            datafusion::logical_expr::when(list.is_null(),null).otherwise(value)?
        } else { value }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
use arrow::array::Array;
    #[test]
    fn effectful_and_throwing_predicates_stay_out_of_scalar_path() {
        let item=IrExpr::Binding("x".into());
        for predicate in [
            IrExpr::Call{name:"rand".into(),args:vec![]},
            IrExpr::Call{name:"mod".into(),args:vec![item.clone(),IrExpr::Lit(Lit::Int(0))]},
            IrExpr::Call{name:"mod".into(),args:vec![item.clone(),IrExpr::Lit(Lit::Int(-1))]},
            IrExpr::Binary{op:BinaryOp::Div,lhs:Box::new(item.clone()),rhs:Box::new(IrExpr::Lit(Lit::Int(1)))},
        ] { assert!(!candidate_predicate(&predicate),"{predicate:?}"); }
    }
    #[test]
    fn length_fast_path_requires_collection_types_and_pure_operands() {
        let plan=LogicalPlanBuilder::empty(true).project(vec![
            lit(7_i64).alias("number"),lit("é").alias("text"),
            datafusion::functions_nested::expr_fn::make_array(vec![lit(1_i64)]).alias("items"),
        ]).unwrap().build().unwrap();
        assert!(!length_operand(&IrExpr::Binding("number".into()),&plan,true).unwrap());
        assert!(length_operand(&IrExpr::Binding("text".into()),&plan,false).unwrap());
        assert!(length_operand(&IrExpr::Binding("items".into()),&plan,true).unwrap());
        assert!(!length_operand(&IrExpr::Binding("items".into()),&plan,false).unwrap());
        assert!(!candidate_predicate(&IrExpr::Call{name:"size".into(),args:vec![
            IrExpr::Call{name:"rand".into(),args:vec![]}
        ]}));
    }
    #[test]
    fn column_sources_with_nonimmutable_producers_are_rejected() {
        use datafusion::logical_expr::Volatility;
        for volatility in [Volatility::Volatile, Volatility::Stable, Volatility::Immutable] {
            let function=datafusion::logical_expr::create_udf("source_list",vec![],
                DataType::List(Arc::new(Field::new("item",DataType::Int64,true))), volatility,
                Arc::new(|_| panic!("must not evaluate diagnostic function")));
            let plan=LogicalPlanBuilder::empty(true).project(vec![function.call(vec![]).alias("xs")])
                .unwrap().build().unwrap();
            assert!(super::super::list_comparison::repeatable(&plan,&col_exact("xs")));
            assert_eq!(super::super::list_comparison::immutable_source_plan(&plan),volatility==Volatility::Immutable);
        }
    }
    #[test]
    fn constructor_nullability_does_not_trust_function_names() {
        let spoof=datafusion::logical_expr::create_udf("make_array",vec![],
            DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
            datafusion::logical_expr::Volatility::Immutable,
            Arc::new(|_| panic!("must not evaluate diagnostic function")));
        let plan=LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(super::super::list_comparison::list_nullable(&plan,&spoof.call(vec![])).unwrap());
    }
    #[test]
    fn effectful_list_functions_are_not_repeatable_inputs() {
        let function=datafusion::logical_expr::create_udf("side_effect_list",vec![],
            DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| panic!("must not evaluate diagnostic function")));
        let plan=LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(!super::super::list_comparison::repeatable(&plan,&function.call(vec![])));
    }
}

/// Quantifying the booleans produced by collect needs no list materialization.
/// Keep the original list output until ordinary demand pruning proves it dead.
pub(super) fn fuse_boolean_collect(
    plan: &LogicalPlan, column: &Column, kind: QuantifierKind, output: &str,
) -> RelResult<Option<LogicalPlan>> {
    if matches!(kind,QuantifierKind::Single) { return Ok(None); }
    fn source(e: &Expr) -> Option<&Column> {
        match e {
            Expr::Column(c)=>Some(c), Expr::Alias(a)=>source(&a.expr),
            Expr::ScalarFunction(f) if f.func.name()=="coalesce" && f.args.len()==2 => source(&f.args[0]),
            Expr::Case(c) if c.expr.is_none() && c.when_then_expr.len()==1 => {
                let (p,v)=&c.when_then_expr[0];
                if matches!(p.as_ref(),Expr::IsNotNull(e) if e==v) {source(v)} else {None}
            }
            _=>None,
        }
    }
    let index=plan.schema().index_of_column(column)?;
    match plan {
        LogicalPlan::Projection(p)=> {
            let Some(c)=source(&p.expr[index]) else {return Ok(None);};
            // Only a collect's empty-list default may be bypassed.
            fn default_empty(e:&Expr)->bool {
                match e {
                    Expr::Column(_)=>true, Expr::Alias(a)=>default_empty(&a.expr),
                    Expr::ScalarFunction(f) if f.func.name()=="coalesce" && f.args.len()==2 => empty(&f.args[1]),
                    Expr::Case(c)=>c.else_expr.as_deref().is_some_and(empty), _=>false,
                }
            }
            fn empty(e:&Expr)->bool {
                match e {
                    Expr::Literal(ScalarValue::List(a),_)=>a.len()==1 && !a.is_null(0) && a.value_length(0)==0,
                    Expr::Cast(c)=>empty(&c.expr),
                    Expr::ScalarFunction(f)=>f.func.name()=="make_array" && f.args.is_empty(), _=>false,
                }
            }
            if !default_empty(&p.expr[index]) {return Ok(None);}
            let Some(input)=fuse_boolean_collect(&p.input,c,kind,output)? else {return Ok(None);};
            let mut exprs=p.expr.clone();exprs.push(col_exact(output));
            Ok(Some(LogicalPlanBuilder::from(input).project(exprs)?.build()?))
        }
        LogicalPlan::SubqueryAlias(a)=> {
            let Some(input)=fuse_boolean_collect(&a.input,&a.input.schema().columns()[index],kind,output)? else {return Ok(None);};
            Ok(Some(LogicalPlanBuilder::from(input).alias(a.alias.clone())?.build()?))
        }
        LogicalPlan::Aggregate(a) if index>=a.group_expr.len()=> {
            let e=&a.aggr_expr[index-a.group_expr.len()];
            let e=if let Expr::Alias(x)=e {x.expr.as_ref()} else {e};
            let Expr::AggregateFunction(call)=e else {return Ok(None);};
            if !call.func.inner().as_any().is::<datafusion::functions_aggregate::array_agg::ArrayAgg>() {return Ok(None);}
            let [value]=call.params.args.as_slice() else {return Ok(None);};
            if value.get_type(a.input.schema())?!=DataType::Boolean || value.is_volatile()
                || !super::list_comparison::arithmetic_is_total(&a.input,value) {return Ok(None);}
            fn excludes_null(e:&Expr,value:&Expr)->bool {
                match e {
                    Expr::IsNotNull(v)=>v.as_ref()==value,
                    Expr::BinaryExpr(b) if b.op==datafusion::logical_expr::Operator::And => excludes_null(&b.left,value)||excludes_null(&b.right,value),
                    _=>false,
                }
            }
            if value.nullable(a.input.schema())? && !call.params.filter.as_deref().is_some_and(|p|excludes_null(p,value)) {return Ok(None);}
            if call.params.filter.as_deref().is_some_and(|p|p.is_volatile() || !super::list_comparison::arithmetic_is_total(&a.input,p)) {return Ok(None);}
            let condition=if matches!(kind,QuantifierKind::All) {Expr::IsFalse(Box::new(value.clone()))}
                else {Expr::IsTrue(Box::new(value.clone()))};
            let indicator=datafusion::logical_expr::when(condition,lit(1_i64)).otherwise(lit(ScalarValue::Int64(None)))?;
            let mut count=df_count(indicator);
            if let Expr::AggregateFunction(count)=&mut count {count.params.filter=call.params.filter.clone();}
            let name=unique_internal_alias(plan,&BTreeSet::from([output.to_owned()]),"__collect_truth_count");
            let mut aggs=a.aggr_expr.clone();aggs.push(count.alias(&name));
            let input=LogicalPlanBuilder::from(a.input.as_ref().clone()).aggregate(a.group_expr.clone(),aggs)?.build()?;
            let truth=if matches!(kind,QuantifierKind::Any) {col_exact(&name).gt(lit(0_i64))} else {col_exact(&name).eq(lit(0_i64))};
            let mut cols=plan.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>();cols.push(truth.alias(output));
            Ok(Some(LogicalPlanBuilder::from(input).project(cols)?.build()?))
        }
        _=>Ok(None),
    }
}
