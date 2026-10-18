//! Structural Cypher predicates that do not materialize graph values.
//!
//! Local graph identity is (element kind, storage label, internal identity).
//! Properties and logical label sets are deliberately not equality keys.

use super::*;

impl<'a> LoweringContext<'a> {
    /// Choose a complete graph identity and payload from the first non-null
    /// binding. Componentwise COALESCE would incorrectly mix properties from
    /// different nodes when the selected node has an absent property.
    pub(super) fn graph_coalesce_projection(&self, plan: &LogicalPlan, alias: &str, expr: &IrExpr) -> RelResult<Option<Vec<Expr>>> {
        if self.language != Language::Cypher || self.options.mapping.is_some() { return Ok(None); }
        let IrExpr::Call { name, args } = expr else { return Ok(None); };
        if name != "cypher_coalesce" || args.is_empty() { return Ok(None); }
        let mut bindings = Vec::new();
        let mut shape = None;
        let mut columns = BTreeMap::new();
        for arg in args {
            if matches!(arg, IrExpr::Lit(Lit::Null)) { continue; }
            let IrExpr::Binding(binding) = arg else { return Ok(None); };
            if resolve_column_name(plan, binding).is_some() { return Ok(None); }
            let Some(kind) = has_binding_shape(plan, binding) else { return Ok(None); };
            if shape.is_some_and(|previous| previous != kind) { return Ok(None); }
            shape = Some(kind);
            for column in binding_column_names(plan, binding)? {
                let suffix = column.strip_prefix(binding).expect("binding column prefix").to_owned();
                let ty = plan.schema().field_with_unqualified_name(&column)?.data_type().clone();
                if let Some(previous) = columns.get(&suffix) {
                    if previous != &ty { return Ok(None); }
                }
                columns.insert(suffix, ty);
            }
            bindings.push(binding);
        }
        if bindings.is_empty() { return Ok(None); }
        let mut output = Vec::new();
        for (suffix, ty) in columns {
            let null = lit(ScalarValue::try_from(&ty)?);
            let arms = bindings.iter().map(|binding| {
                let column = format!("{binding}{suffix}");
                let value = if has_exact_col(plan, &column) { col_exact(column) } else { null.clone() };
                (Box::new(col_exact(id_col(binding)).is_not_null()), Box::new(value))
            }).collect();
            output.push(Expr::Case(Case::new(None, arms, Some(Box::new(null)))).alias(format!("{alias}{suffix}")));
        }
        Ok(Some(output))
    }

    /// Align the physical transport layout by name; DISTINCT compares logical
    /// graph identities rather than the metadata/property payload. Differing
    /// layouts remain fenced until the transport has a typed missing-field
    /// contract, rather than allowing UNION's implicit coercion.
    pub(super) fn lower_graph_union(&self, all: bool, left: LoweredNode, right: LoweredNode) -> RelResult<LoweredNode> {
        let unsupported = || RelError::Unsupported("Cypher graph UNION requires matching typed identity layouts".into());
        if self.options.mapping.is_some() || left.fields != right.fields { return Err(unsupported()); }
        let fields = left.fields.as_ref().ok_or_else(unsupported)?;
        for field in fields {
            let a = has_binding_shape(&left.plan, field);
            let b = has_binding_shape(&right.plan, field);
            if a != b { return Err(unsupported()); }
            if a.is_none() {
                let t = plan_column_type(&left.plan, field).ok_or_else(unsupported)?;
                if !matches!(t, DataType::Null | DataType::Boolean | DataType::Int8 | DataType::Int16
                    | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16
                    | DataType::UInt32 | DataType::UInt64 | DataType::Float32 | DataType::Float64
                    | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) { return Err(unsupported()); }
            }
        }
        let types = |plan: &LogicalPlan| plan.schema().fields().iter()
            .map(|f| (f.name().clone(), f.data_type().clone())).collect::<BTreeMap<_, _>>();
        let columns = types(&left.plan);
        if columns != types(&right.plan) { return Err(unsupported()); }
        let project = || columns.keys().map(col_exact).collect::<Vec<_>>();
        let a = LogicalPlanBuilder::from(left.plan.clone()).project(project())?.build()?;
        let b = LogicalPlanBuilder::from(right.plan.clone()).project(project())?.build()?;
        let mut plan = LogicalPlanBuilder::from(a).union(b)?.build()?;
        if !all {
            // Correlated UNION belongs to each outer occurrence independently.
            // Duplicate outer values must not share a DISTINCT set.
            let mut groups = Vec::new();
            let mut hidden = BTreeSet::new();
            for column in apply_correlation_key_columns(&plan) {
                let alias = unique_internal_alias(&plan, &hidden, format!("__cypher_union_key_{}", hidden.len()));
                hidden.insert(alias.clone());
                groups.push(col_exact(column).alias(alias));
            }
            for field in fields {
                let keys = match self.graph_group_keys(&plan, &IrExpr::Binding(field.clone()))? {
                    Some(keys) => keys,
                    None if has_binding_shape(&plan, field).is_some() => return Err(unsupported()),
                    None => vec![language_functions::key(col_exact(field), &plan)?],
                };
                for key in keys {
                    let alias = unique_internal_alias(&plan, &hidden, format!("__cypher_union_key_{}", hidden.len()));
                    hidden.insert(alias.clone());
                    groups.push(key.alias(alias));
                }
            }
            let representatives: Vec<_> = columns.keys().map(|column|
                datafusion::functions_aggregate::first_last::first_value(col_exact(column), vec![]).alias(column)
            ).collect();
            plan = LogicalPlanBuilder::from(plan).aggregate(groups, representatives)?.project(project())?.build()?;
        }
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode { plan, islands, fields: left.fields, result_form: left.result_form })
    }

    /// A structural binding has a fixed kind and a snapshot-stable payload.
    /// Nullable identities all represent the same Cypher null regardless of
    /// any retained label metadata. Provider identity domains remain fenced.
    pub(super) fn graph_group_keys(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Vec<Expr>>> {
        if self.language != Language::Cypher || self.options.mapping.is_some() {
            return Ok(None);
        }
        let IrExpr::Binding(binding) = expr else { return Ok(None) };
        if resolve_column_name(plan, binding).is_some() || has_binding_shape(plan, binding).is_none() {
            return Ok(None);
        }
        let id = col_exact(id_col(binding));
        if matches!(id.get_type(plan.schema())?, DataType::Struct(_) | DataType::List(_) | DataType::LargeList(_)) {
            return Ok(None);
        }
        let label = datafusion::logical_expr::when(id.clone().is_null(), lit(ScalarValue::Utf8(None)))
            .otherwise(col_exact(label_col(binding)))?;
        Ok(Some(vec![id, label]))
    }

    /// Graph results use the same flat identity layout as relational islands.
    /// `LoweredPlan.fields` retains logical field names; the runtime boundary
    /// reconstructs references with batch_to_bindings and finalize_return.
    /// This is output transport, not opaque native scalar evaluation.
    pub(super) fn graph_return_columns(
        &self,
        plan: &LogicalPlan,
        binding: &str,
    ) -> RelResult<Vec<Expr>> {
        binding_column_names(plan, binding).map(|columns| {
            columns.into_iter().map(col_exact).collect()
        })
    }

    fn graph_property_keys(&self, plan: &LogicalPlan, binding: &str, shape: BindingShape) -> RelResult<Expr> {
        use datafusion::functions_nested::expr_fn::{array_concat, make_array};
        let empty = lit(ScalarValue::List(ScalarValue::new_list(&[], &DataType::Utf8, true)));
        let labels = match (&self.options.mapping, shape) {
            (Some(mapping), BindingShape::Node) => mapping.labels(),
            (Some(mapping), BindingShape::Edge) => mapping.rel_types(),
            (None, BindingShape::Node) => self.graph.labels(),
            (None, BindingShape::Edge) => self.graph.rel_types(),
        };
        let mut cases = Vec::new();
        for label in labels {
            // Declared mappings are authoritative even when their current
            // tables are empty. Never infer physical properties from fixtures.
            let keys = match (&self.options.mapping, shape) {
                (Some(mapping), BindingShape::Node) => mapping.node(&label)
                    .map(|node| node.properties.keys().cloned().collect()).unwrap_or_default(),
                (Some(mapping), BindingShape::Edge) => mapping.edge_partitions(&label).into_iter()
                    .flat_map(|edge| edge.properties.keys().cloned()).collect::<BTreeSet<_>>().into_iter().collect(),
                (None, BindingShape::Node) => self.graph.node_property_keys_with_id(&label),
                (None, BindingShape::Edge) => self.graph.edge_property_keys(&label),
            };
            let mut pieces = Vec::new();
            for key in keys {
                if key == STRUCT_ORDER_KEY || key == STRUCT_TYPES_KEY { continue; }
                let column = prop_col(binding, &key);
                if !has_exact_col(plan, &column) { continue; }
                pieces.push(datafusion::logical_expr::when(col_exact(column).is_not_null(), make_array(vec![lit(key)]))
                    .otherwise(empty.clone())?);
            }
            let keys = match pieces.len() {
                0 => empty.clone(),
                1 => pieces.remove(0),
                _ => array_concat(pieces),
            };
            cases.push((Box::new(col_exact(label_col(binding)).eq(lit(label))), Box::new(keys)));
        }
        let keys = if cases.is_empty() { empty.clone() } else {
            Expr::Case(Case::new(None, cases, Some(Box::new(empty.clone()))))
        };
        let null = lit(ScalarValue::try_from(&empty.get_type(plan.schema())?)?);
        Ok(datafusion::logical_expr::when(col_exact(id_col(binding)).is_null(), null).otherwise(keys)?)
    }

    /// Recognize only proved structural bindings. A scalar shadowing a binding
    /// takes precedence, and mapped/provider identities retain their own path.
    pub(super) fn lower_graph_scalar(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        if self.language != Language::Cypher {
            return Ok(None);
        }
        let binding = |expr: &IrExpr| -> Option<(String, BindingShape)> {
            let IrExpr::Binding(name) = expr else { return None };
            if resolve_column_name(plan, name).is_some() {
                return None;
            }
            has_binding_shape(plan, name).map(|shape| (name.clone(), shape))
        };
        match expr {
            // Constructor scalarization can expose a graph reference beneath
            // the generic property call used for non-variable AST targets.
            IrExpr::Call { name, args } if name == "property" => {
                if let [value, IrExpr::Lit(Lit::String(key))] = args.as_slice()
                    && let Some((binding, _)) = binding(value)
                {
                    return self.lower_expr(plan, &IrExpr::Property {
                        binding,
                        name: key.clone(),
                        policy: crate::ir::policy::PropertyMissing::NullOnMissing,
                    }).map(Some);
                }
            }
            IrExpr::IsNull(value) | IrExpr::IsNotNull(value) => {
                if let Some((name, _)) = binding(value) {
                    let id = col_exact(id_col(&name));
                    return Ok(Some(if matches!(expr, IrExpr::IsNull(_)) {
                        id.is_null()
                    } else {
                        id.is_not_null()
                    }));
                }
            }
            IrExpr::Call { name, args } if name == "keys" => {
                if let [value] = args.as_slice()
                    && let Some((name, shape)) = binding(value)
                {
                    return self.graph_property_keys(plan, &name, shape).map(Some);
                }
            }
            IrExpr::Call { name, args } if name == "cypher_type" => {
                if let [value] = args.as_slice()
                    && let Some((name, BindingShape::Edge)) = binding(value)
                {
                    return Ok(Some(datafusion::logical_expr::when(
                        col_exact(id_col(&name)).is_null(),
                        lit(ScalarValue::Utf8(None)),
                    ).otherwise(col_exact(label_col(&name)))?));
                }
            }
            _ => {}
        }
        let (left, right, unequal) = match expr {
            IrExpr::Call { name, args } if args.len() == 2
                && matches!(name.as_str(), "cypher_eq" | "cypher_neq") =>
                (&args[0], &args[1], name == "cypher_neq"),
            IrExpr::Binary { op, lhs, rhs } if matches!(op, BinaryOp::Eq | BinaryOp::Neq) =>
                (lhs.as_ref(), rhs.as_ref(), *op == BinaryOp::Neq),
            _ => return Ok(None),
        };
        let (Some((left, left_kind)), Some((right, right_kind))) =
            (binding(left), binding(right)) else { return Ok(None) };
        let lhs = col_exact(id_col(&left));
        let rhs = col_exact(id_col(&right));
        // Do not let SQL coerce unrelated identity domains or compare opaque
        // identity union structs using SQL's compound-null equality rules.
        let lt = lhs.get_type(plan.schema())?;
        let rt = rhs.get_type(plan.schema())?;
        if lt != rt || matches!(lt, DataType::Struct(_) | DataType::List(_) | DataType::LargeList(_)) {
            return Ok(None);
        }
        // Mapped scalar keys are unique within their storage label and retain
        // their declared SQL equality semantics. Compare keys directly, never
        // rendered node/edge strings (which also include unrelated payloads).
        // Composite/provider identity envelopes still take their existing path.
        if self.options.mapping.is_some() && !mapping::is_scalar_identity_type(&lt) {
            return Ok(None);
        }
        let nullable = lhs.nullable(plan.schema())? || rhs.nullable(plan.schema())?;
        let null = lhs.clone().is_null().or(rhs.clone().is_null());
        let equal = if left_kind != right_kind {
            lit(false)
        } else {
            lhs.eq(rhs).and(col_exact(label_col(&left)).eq(col_exact(label_col(&right))))
        };
        let compared = if unequal { Expr::Not(Box::new(equal)) } else { equal };
        if !nullable { return Ok(Some(compared)); }
        Ok(Some(datafusion::logical_expr::when(null, lit(ScalarValue::Boolean(None)))
            .otherwise(compared)?))
    }
}

#[cfg(all(test, feature = "duckdb"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn flat_terminal_columns_decode_back_to_graph_references() {
        let graph = PropertyGraph::new();
        let a = graph.insert_node("A", BTreeMap::new());
        let b = graph.insert_node("B", BTreeMap::new());
        let edge = graph.insert_edge("T", &a, &b, BTreeMap::new()).unwrap();
        for (query, expected_fields, expected) in [
            ("MATCH (a:A)-[r:T]->(b:B) RETURN b,r,a", vec!["b", "r", "a"], vec![b.clone(), edge, a]),
            ("MATCH (b:B) OPTIONAL MATCH (b)-[r:T]->(n) RETURN n,r,b", vec!["n", "r", "b"], vec![Value::Null, Value::Null, b]),
        ] {
            let ast = crate::language::cypher::parser::parse_query(query).unwrap();
            let plan = crate::language::cypher::planner::CypherPlanner::new().plan(&ast).unwrap();
            let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
            let returned = sql::execute_lowered_sql(&mut sql::DuckDbExecutor::default(), &lowered).await.unwrap();
            let (fields, rows) = crate::ir::exec::batch_to_bindings(&returned).unwrap();
            assert_eq!(fields, expected_fields);
            assert_eq!(rows, vec![expected]);
        }
    }
}
