use super::CypherRelationship;
use crate::ir::rel::{
    RelBackend, RelBackendOptions,
    mapping::{EdgeMapping, GraphMapping, KeyColumns, MappedSource},
};
use crate::ir::{catalog::PropertyGraph, expr::IrExpr, plan::*, policy::*, procedures::*};
use crate::language::cypher::{ast, parameters, parser, planner, procedures};
use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder};
use datafusion::prelude::{Expr, ExprFunctionExt, col};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) fn install(
    mapping: &mut GraphMapping,
    declarations: &[CypherRelationship],
    procedures: ProcedureCatalog,
) -> Result<(), String> {
    mapping.catalog_procedures = procedures;
    for rule in declarations {
        rule.validate()?;
        if mapping.edge(&rule.name).is_some() {
            return Err(format!("duplicate relationship {}", rule.name));
        }
        let source = mapping
            .node(&rule.source)
            .ok_or_else(|| format!("unknown source label {}", rule.source))?;
        let target = mapping
            .node(&rule.target)
            .ok_or_else(|| format!("unknown target label {}", rule.target))?;
        let src: Vec<_> = (0..source.id_column.len())
            .map(|i| format!("__src_{i}"))
            .collect();
        let dst: Vec<_> = (0..target.id_column.len())
            .map(|i| format!("__dst_{i}"))
            .collect();
        let mut edge = EdgeMapping::new(
            &rule.name,
            MappedSource::Cypher(Box::new(rule.clone())),
            KeyColumns::from(src),
            KeyColumns::from(dst),
            &rule.source,
            &rule.target,
        )
        .with_id("__edge_id");
        for (index, name) in rule.returns.properties.keys().enumerate() {
            edge = edge.property(name, format!("__property_{index}"));
        }
        mapping.map_edge(edge);
        mapping
            .cypher_relationships
            .insert(rule.name.clone(), rule.clone());
    }
    let dependencies = mapping
        .cypher_relationships
        .values()
        .map(|rule| {
            let query = parser::parse_query(&rule.cypher).map_err(|e| e.to_string())?;
            let mut names = BTreeSet::new();
            collect_dependencies(&query, &mapping.cypher_relationships, &mut names);
            Ok((rule.name.clone(), names))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    fn visit(
        name: &str,
        dependencies: &BTreeMap<String, BTreeSet<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if done.contains(name) {
            return Ok(());
        }
        if !active.insert(name.into()) {
            return Err(format!("cyclic relationship declaration `{name}`"));
        }
        if let Some(names) = dependencies.get(name) {
            for name in names {
                visit(name, dependencies, active, done)?;
            }
        }
        active.remove(name);
        done.insert(name.into());
        Ok(())
    }
    let mut done = BTreeSet::new();
    for name in dependencies.keys() {
        visit(name, &dependencies, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}
fn collect_dependencies(
    query: &ast::Query,
    declarations: &BTreeMap<String, CypherRelationship>,
    output: &mut BTreeSet<String>,
) {
    for clause in &query.clauses {
        match clause {
            ast::Clause::Match(m) => {
                for part in &m.patterns {
                    for chain in &part.element.chains {
                        if chain.relationship.types.is_empty() {
                            output.extend(declarations.keys().cloned());
                        }
                        output.extend(
                            chain
                                .relationship
                                .types
                                .iter()
                                .filter(|name| declarations.contains_key(*name))
                                .cloned(),
                        );
                    }
                }
            }
            ast::Clause::Call(call) if declarations.contains_key(&call.name) => {
                output.insert(call.name.clone());
            }
            _ => {}
        }
    }
    for branch in &query.unions {
        collect_dependencies(&branch.query, declarations, output);
    }
}
fn body(
    rule: &CypherRelationship,
    supplied: &BTreeMap<String, serde_json::Value>,
    mapping: &mut GraphMapping,
) -> Result<Node, String> {
    let values = rule
        .arguments(supplied)?
        .iter()
        .map(|(k, v)| Ok((k.clone(), crate::compiler::parameter(v)?)))
        .collect::<Result<_, String>>()?;
    let mut query = parser::parse_query(&rule.cypher).map_err(|e| e.to_string())?;
    super::relationship::validate_target_entity(&query, rule, &mapping.cypher_relationships)?;
    parameters::bind_parameters_with_diagnostics(&mut query, &values).map_err(|e| e.to_string())?;
    let mut catalog = mapping.catalog_procedures.clone();
    let mut bodies = BTreeMap::new();
    prepare_calls(&mut query, mapping, &mut catalog, &mut bodies)?;
    procedures::prepare(&mut query, &catalog).map_err(|e| e.to_string())?;
    let mut node =
        planner::lowering::lower_relationship(&query, &rule.source, &rule.returns.target)
            .map_err(|e| e.to_string())?;
    expand_calls(&mut node, &bodies)?;
    expand_table_procedures(&mut node, &mapping.catalog_procedures, &mut 0)?;
    fn bind_slices(node: &mut Node) -> Result<(), String> {
        for child in crate::ir::analysis::children_mut(node) {
            bind_slices(child)?;
        }
        if let Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } = node
        {
            let graph = PropertyGraph::new();
            let evaluate = |name, expr: &IrExpr| {
                crate::ir::runtime::ops::slice::evaluate_slice_bound(name, expr, &graph)
                    .map_err(|e| e.to_string())
            };
            let offset = offset
                .as_ref()
                .map(|expr| evaluate("SKIP", expr))
                .transpose()?
                .unwrap_or(0);
            let fetch = fetch
                .as_ref()
                .map(|expr| evaluate("LIMIT", expr))
                .transpose()?;
            *node = Node::GraphSlice {
                slice: Slice {
                    offset,
                    fetch,
                    tail: None,
                },
                input: std::mem::replace(input, Node::GraphOneRow.boxed()),
            };
        }
        Ok(())
    }
    bind_slices(&mut node)?;
    fn strip_returns(node: Node) -> Node {
        match node {
            Node::GraphReturn { input, .. } => *input,
            Node::GraphUnion {
                all,
                align,
                left,
                right,
            } => Node::GraphUnion {
                all,
                align,
                left: strip_returns(*left).boxed(),
                right: strip_returns(*right).boxed(),
            },
            other => other,
        }
    }
    crate::ir::analysis::validate_read_capabilities(
        &GraphPlan::new(GraphPlanPolicy::cypher(), node.clone()),
        crate::ir::analysis::ReadCapabilities::ALL_READS,
    )
    .map_err(|e| e.to_string())?;
    node = strip_returns(node);
    Ok(Node::GraphSlice {
        slice: Slice::NONE,
        input: node.boxed(),
    })
}
pub(crate) fn relationship_plan(
    mapping: &GraphMapping,
    rule: &CypherRelationship,
) -> Result<LogicalPlan, String> {
    thread_local! { static ACTIVE: std::cell::RefCell<BTreeSet<String>> = const { std::cell::RefCell::new(BTreeSet::new()) }; }
    if !ACTIVE.with_borrow_mut(|active| active.insert(rule.name.clone())) {
        return Err(format!("cyclic relationship declaration `{}`", rule.name));
    }
    struct Guard(String);
    impl Drop for Guard {
        fn drop(&mut self) {
            ACTIVE.with_borrow_mut(|active| active.remove(&self.0));
        }
    }
    let _guard = Guard(rule.name.clone());
    let mut mapping = mapping.clone();
    let right = body(rule, &BTreeMap::new(), &mut mapping)?;
    if let Some(plan) = super::retrieval::plan(&mapping, rule)? {
        validate_return_types(rule, plan.schema().as_ref())?;
        return Ok(plan);
    }
    mapped_plan(&mapping, rule, right)
}
fn mapped_plan(
    mapping: &GraphMapping,
    rule: &CypherRelationship,
    mut right: Node,
) -> Result<LogicalPlan, String> {
    let mut input_name = "catalog_source".to_string();
    while input_name == rule.returns.target || rule.returns.properties.contains_key(&input_name) {
        input_name.push('_');
    }
    correlate_source(&mut right, &input_name);
    let source = Node::node_scan("default", &input_name, LabelExpr::label(&rule.source));
    let outputs = std::iter::once(rule.returns.target.clone())
        .chain(rule.returns.properties.keys().cloned())
        .collect();
    let apply = Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec![input_name.clone()],
        outputs,
        optional_missing: OptionalMissing::Null,
        left: source.boxed(),
        right: right.boxed(),
    };
    let mut items = vec![
        ProjectionItem {
            alias: "__source_id".into(),
            expr: IrExpr::Binding(crate::ir::rel::id_col(&input_name)),
        },
        ProjectionItem {
            alias: "__target_id".into(),
            expr: IrExpr::Binding(crate::ir::rel::id_col(&rule.returns.target)),
        },
    ];
    items.extend(
        rule.returns
            .properties
            .keys()
            .enumerate()
            .map(|(index, name)| ProjectionItem {
                alias: format!("__property_{index}"),
                expr: IrExpr::Binding(name.clone()),
            }),
    );
    let fields = items.iter().map(|i| i.alias.clone()).collect();
    let root = Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items,
        error_policy: ProjectErrorPolicy::PropagateError,
        input: apply.boxed(),
    }
    .return_(fields, ResultForm::RowSet);
    let graph = PropertyGraph::new();
    let plan = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping.clone())),
        ..Default::default()
    })
    .lower(&GraphPlan::new(GraphPlanPolicy::cypher(), root), &graph)
    .map_err(|e| e.to_string())?
    .plan;
    validate_return_types(rule, plan.schema().as_ref())?;
    let mut projection = Vec::new();
    for (label, id, prefix) in [
        (&rule.source, "__source_id", "__src"),
        (&rule.target, "__target_id", "__dst"),
    ] {
        let count = mapping.node(label).unwrap().id_column.len();
        for i in 0..count {
            let expr = if count == 1 {
                col(id)
            } else {
                datafusion::functions::core::expr_fn::get_field(col(id), format!("k{i}"))
            };
            projection.push(expr.alias(format!("{prefix}_{i}")));
        }
    }
    projection.extend(
        rule.returns
            .properties
            .keys()
            .enumerate()
            .map(|(index, _)| col(format!("__property_{index}"))),
    );
    let row_number = datafusion::functions_window::row_number::row_number()
        .order_by(vec![
            col("__source_id").sort(true, true),
            col("__target_id").sort(true, true),
        ])
        .build()
        .map_err(|e| e.to_string())?;
    let plan = LogicalPlanBuilder::from(plan)
        .window(vec![row_number.alias("__edge_id")])
        .map_err(|e| e.to_string())?
        .project(
            projection
                .into_iter()
                .chain(std::iter::once(col("__edge_id")))
                .collect::<Vec<Expr>>(),
        )
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    Ok(plan)
}

pub(crate) fn prepare_query(
    text: &str,
    values: &BTreeMap<String, crate::ir::value::Value>,
    mapping: &mut GraphMapping,
    native: bool,
) -> Result<GraphPlan, String> {
    if mapping.cypher_relationships.is_empty() {
        return crate::language::cypher::preparation::prepare(
            text,
            values,
            native.then_some(&mapping.catalog_procedures),
        )
        .map_err(|e| e.to_string());
    }
    let mut parsed = parser::parse_query(text).map_err(|e| e.to_string())?;
    parameters::bind_parameters_with_diagnostics(&mut parsed, values).map_err(|e| e.to_string())?;
    let mut catalog = mapping.catalog_procedures.clone();
    let mut bodies = BTreeMap::new();
    prepare_calls(&mut parsed, mapping, &mut catalog, &mut bodies)?;
    procedures::prepare(&mut parsed, &catalog).map_err(|e| e.to_string())?;
    let mut plan = planner::CypherPlanner::new()
        .plan(&parsed)
        .map_err(|e| e.to_string())?;
    expand_calls(&mut plan.root, &bodies)?;
    Ok(plan)
}
fn prepare_calls(
    query: &mut ast::Query,
    mapping: &mut GraphMapping,
    catalog: &mut ProcedureCatalog,
    bodies: &mut BTreeMap<String, Node>,
) -> Result<(), String> {
    prepare_patterns(query, mapping)?;
    for clause in &mut query.clauses {
        let ast::Clause::Call(call) = clause else {
            continue;
        };
        let Some(rule) = mapping.cypher_relationships.get(&call.name).cloned() else {
            continue;
        };
        if call.args.is_empty() || call.args.len() > rule.parameters.len() + 1 {
            return Err(format!(
                "{} requires a source node followed by its declared arguments",
                rule.name
            ));
        }
        let arguments = rule
            .parameters
            .iter()
            .zip(call.args.iter().skip(1))
            .map(|(p, arg)| Ok((p.name.clone(), constant(arg)?)))
            .collect::<Result<_, String>>()?;
        let right = body(&rule, &arguments, mapping)?;
        mapped_plan(mapping, &rule, right.clone())?;
        let name = format!("__orchid_catalog_call_{}", bodies.len());
        let field = |name: String, type_name: &str| ProcedureField {
            name,
            type_name: type_name.into(),
            nullable: false,
        };
        let outputs = std::iter::once(field(rule.returns.target.clone(), "NODE"))
            .chain(
                rule.returns
                    .properties
                    .keys()
                    .map(|name| field(name.clone(), "ANY")),
            )
            .collect();
        catalog.insert(
            name.clone(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![field("source".into(), "NODE")],
                    outputs,
                },
                rows: vec![],
            },
        );
        bodies.insert(name.clone(), right);
        call.name = name;
        call.args.truncate(1);
    }
    for branch in &mut query.unions {
        prepare_calls(&mut branch.query, mapping, catalog, bodies)?;
    }
    Ok(())
}
fn prepare_patterns(query: &mut ast::Query, mapping: &mut GraphMapping) -> Result<(), String> {
    fn pattern(part: &mut ast::PatternPart, mapping: &mut GraphMapping) -> Result<(), String> {
        if let Some(value) = &mut part.element.start.properties { expression(value, mapping)?; }
        for chain in &mut part.element.chains {
            let rel = &mut chain.relationship;
            if let Some(ast::Expr::Map(items)) = &mut rel.properties {
                let rules: Vec<_> = rel.types.iter().filter_map(|name| mapping.cypher_relationships.get(name)).collect();
                let mut arguments = BTreeMap::new();
                for (name, value) in items.iter() {
                    if rules.iter().any(|rule| rule.parameters.iter().any(|p| p.name == *name)) {
                        if rel.types.len() != 1 {
                            return Err("derived relationship arguments require one explicit relationship type".into());
                        }
                        if arguments.insert(name.clone(), constant(value)?).is_some() {
                            return Err(format!("duplicate relationship parameter `{name}`"));
                        }
                    }
                }
                if !arguments.is_empty() {
                    rel.types[0] = mapping.bind_edge_arguments(&rel.types[0], &arguments)?;
                    items.retain(|(name, _)| !arguments.contains_key(name));
                    if items.is_empty() { rel.properties = None; }
                }
            }
            if let Some(value) = &mut rel.properties { expression(value, mapping)?; }
            if let Some(recursive) = &mut rel.recursive {
                if let Some(value) = &mut recursive.predicate { expression(value, mapping)?; }
            }
            if let Some(value) = &mut chain.node.properties { expression(value, mapping)?; }
        }
        Ok(())
    }
    fn expression(value: &mut ast::Expr, mapping: &mut GraphMapping) -> Result<(), String> {
        use ast::Expr::*;
        match value {
            Property { target, .. } | LabelPredicate { target, .. } => expression(target, mapping)?,
            List(items) | Function { args: items, .. } => for item in items { expression(item, mapping)?; },
            Map(items) => for (_, item) in items { expression(item, mapping)?; },
            Unary { expr, .. } | IsNull(expr) | IsNotNull(expr) => expression(expr, mapping)?,
            Binary { lhs, rhs, .. } => { expression(lhs, mapping)?; expression(rhs, mapping)?; }
            StringPredicate { target, pattern, .. } => { expression(target, mapping)?; expression(pattern, mapping)?; }
            Case { case, arms, otherwise } => {
                if let Some(value) = case { expression(value, mapping)?; }
                for (a, b) in arms { expression(a, mapping)?; expression(b, mapping)?; }
                if let Some(value) = otherwise { expression(value, mapping)?; }
            }
            Exists(subquery) => {
                if let Some(query) = &mut subquery.query { prepare_patterns(query, mapping)?; }
                for part in &mut subquery.patterns { pattern(part, mapping)?; }
                if let Some(value) = &mut subquery.predicate { expression(value, mapping)?; }
            }
            PatternPredicate(parts) => for part in parts { pattern(part, mapping)?; },
            PatternComprehension { pattern: part, predicate, map, .. } => {
                pattern(part, mapping)?;
                if let Some(value) = predicate { expression(value, mapping)?; }
                expression(map, mapping)?;
            }
            ListComprehension { collection, predicate, map, .. } => {
                expression(collection, mapping)?;
                if let Some(value) = predicate { expression(value, mapping)?; }
                expression(map, mapping)?;
            }
            ListReduce { collection, map, .. } | ListTransform { collection, map, .. } => {
                expression(collection, mapping)?; expression(map, mapping)?;
            }
            ListFilter { collection, predicate, .. } | Quantifier { collection, predicate, .. } => {
                expression(collection, mapping)?; expression(predicate, mapping)?;
            }
            Star | Variable(_) | Parameter(_) | Literal(_) | CountStar => {}
        }
        Ok(())
    }
    fn projection(value: &mut ast::ProjectionBody, mapping: &mut GraphMapping) -> Result<(), String> {
        for item in &mut value.items { expression(&mut item.expr, mapping)?; }
        for item in &mut value.order_by { expression(&mut item.expr, mapping)?; }
        for value in [&mut value.skip, &mut value.limit].into_iter().flatten() { expression(value, mapping)?; }
        Ok(())
    }
    for clause in &mut query.clauses {
        match clause {
            ast::Clause::Match(value) => {
                for part in &mut value.patterns { pattern(part, mapping)?; }
                if let Some(value) = &mut value.predicate { expression(value, mapping)?; }
            }
            ast::Clause::With(value) => {
                projection(&mut value.projection, mapping)?;
                if let Some(value) = &mut value.predicate { expression(value, mapping)?; }
            }
            ast::Clause::Return(value) => projection(&mut value.projection, mapping)?,
            ast::Clause::Unwind(value) => expression(&mut value.expr, mapping)?,
            ast::Clause::Call(value) => {
                for arg in &mut value.args { expression(arg, mapping)?; }
                if let Some(value) = &mut value.predicate { expression(value, mapping)?; }
            }
            _ => {}
        }
    }
    for branch in &mut query.unions { prepare_patterns(&mut branch.query, mapping)?; }
    Ok(())
}

pub(super) fn constant(expr: &ast::Expr) -> Result<serde_json::Value, String> {
    use ast::{Expr, Literal};
    Ok(match expr {
        Expr::Literal(Literal::Null) => serde_json::Value::Null,
        Expr::Literal(Literal::Bool(v)) => (*v).into(),
        Expr::Literal(Literal::String(v)) => v.clone().into(),
        Expr::Literal(Literal::Integer(v)) => serde_json::from_str(v).map_err(|e| e.to_string())?,
        Expr::Literal(Literal::Float(v)) => serde_json::json!(v),
        Expr::List(items) => {
            serde_json::Value::Array(items.iter().map(constant).collect::<Result<_, _>>()?)
        }
        Expr::Map(items) => serde_json::Value::Object(
            items
                .iter()
                .map(|(k, v)| Ok((k.clone(), constant(v)?)))
                .collect::<Result<_, String>>()?,
        ),
        Expr::Unary {
            op: ast::UnaryOp::Neg,
            expr,
        } => {
            let value = constant(expr)?;
            if let Some(value) = value.as_i64() {
                value
                    .checked_neg()
                    .ok_or("integer argument overflow")?
                    .into()
            } else {
                serde_json::json!(-value.as_f64().ok_or("numeric argument required")?)
            }
        }
        _ => return Err("relationship arguments must be literals or query parameters".into()),
    })
}
fn expand_calls(node: &mut Node, bodies: &BTreeMap<String, Node>) -> Result<(), String> {
    for child in crate::ir::analysis::children_mut(node) {
        expand_calls(child, bodies)?;
    }
    if let Node::GraphProcedureCall {
        name,
        args,
        yields,
        input,
        ..
    } = node
    {
        if let Some(body) = bodies.get(name) {
            let Some(ProcedureArg {
                value: IrExpr::Binding(source),
                ..
            }) = args.first()
            else {
                return Err("relationship source must be a node variable".into());
            };
            let source = source.clone();
            let mut right = body.clone();
            correlate_source(&mut right, &source);
            *node = Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation: vec![source],
                outputs: yields.clone(),
                optional_missing: OptionalMissing::Null,
                left: input
                    .take()
                    .ok_or("relationship calls require a source node")?,
                right: right.boxed(),
            };
        }
    }
    Ok(())
}

fn validate_return_types(
    rule: &CypherRelationship,
    schema: &datafusion::common::DFSchema,
) -> Result<(), String> {
    use arrow::datatypes::DataType;
    fn accepts(schema: &serde_json::Value, name: &str) -> bool {
        if schema == &serde_json::Value::Bool(false) {
            return false;
        }
        if let Some(types) = schema.get("type") {
            let matches = |value: &serde_json::Value| {
                value
                    .as_str()
                    .is_some_and(|t| t == name || t == "number" && name == "integer")
            };
            if !types
                .as_array()
                .map(|types| types.iter().any(matches))
                .unwrap_or_else(|| matches(types))
            {
                return false;
            }
        }
        if let Some(branches) = schema.get("allOf").and_then(serde_json::Value::as_array) {
            if !branches.iter().all(|schema| accepts(schema, name)) {
                return false;
            }
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(branches) = schema.get(key).and_then(serde_json::Value::as_array) {
                if !branches.iter().any(|schema| accepts(schema, name)) {
                    return false;
                }
            }
        }
        true
    }
    for (index, (name, contract)) in rule.returns.properties.iter().enumerate() {
        let field = schema
            .field_with_unqualified_name(&format!("__property_{index}"))
            .map_err(|e| e.to_string())?;
        let kind = match field.data_type() {
            DataType::Null => "null",
            DataType::Boolean => "boolean",
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64 => "integer",
            DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal128(..)
            | DataType::Decimal256(..) => "number",
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "string",
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(..) => "array",
            DataType::Struct(_) | DataType::Map(..) => "object",
            other => {
                return Err(format!(
                    "relationship {} property {name} has non-JSON result type {other}",
                    rule.name
                ));
            }
        };
        if !accepts(contract, kind) {
            return Err(format!(
                "relationship {} property {name} has type {kind}, incompatible with its return schema",
                rule.name
            ));
        }
    }
    Ok(())
}

fn correlate_source(node: &mut Node, source: &str) {
    if let Node::GraphCorrelate { bindings } = node {
        if bindings == &["source"] {
            *node = Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items: vec![ProjectionItem {
                    alias: "source".into(),
                    expr: IrExpr::Binding(source.into()),
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: Node::GraphCorrelate {
                    bindings: vec![source.into()],
                }
                .boxed(),
            };
            return;
        }
    }
    for child in crate::ir::analysis::children_mut(node) {
        correlate_source(child, source);
    }
}

fn expand_table_procedures(
    node: &mut Node,
    catalog: &ProcedureCatalog,
    counter: &mut usize,
) -> Result<(), String> {
    for child in crate::ir::analysis::children_mut(node) {
        expand_table_procedures(child, catalog, counter)?;
    }
    let Node::GraphProcedureCall {
        name,
        args,
        yields,
        mode: ProcedureMode::Read,
        input,
    } = node
    else {
        return Ok(());
    };
    let Some(procedure) = catalog.get(name) else {
        return Ok(());
    };
    let prefix = format!("__catalog_procedure_{}", *counter);
    *counter += 1;
    let input_fields = procedure
        .signature
        .inputs
        .iter()
        .enumerate()
        .map(|(i, _)| format!("{prefix}_{i}"))
        .collect::<Vec<_>>();
    let selected = yields
        .iter()
        .map(|name| {
            procedure
                .signature
                .outputs
                .iter()
                .position(|field| &field.name == name)
                .ok_or_else(|| format!("unknown procedure result {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rows = procedure
        .rows
        .iter()
        .map(|row| {
            row[..input_fields.len()]
                .iter()
                .cloned()
                .chain(selected.iter().map(|i| row[input_fields.len() + i].clone()))
                .collect()
        })
        .collect();
    let values = Node::GraphValues {
        bindings: input_fields
            .iter()
            .cloned()
            .chain(yields.iter().cloned())
            .collect(),
        rows,
        bulk: None,
    };
    let conditions = args
        .iter()
        .zip(&input_fields)
        .map(|(arg, field)| {
            let right = IrExpr::Binding(field.clone());
            IrExpr::Binary {
                op: crate::ir::expr::BinaryOp::Or,
                lhs: Box::new(IrExpr::eq(arg.value.clone(), right.clone())),
                rhs: Box::new(IrExpr::and(vec![
                    IrExpr::IsNull(Box::new(arg.value.clone())),
                    IrExpr::IsNull(Box::new(right)),
                ])),
            }
        })
        .collect();
    *node = Node::GraphJoin {
        kind: JoinKind::Inner,
        left: input.take().unwrap_or_else(|| Node::GraphOneRow.boxed()),
        right: values.boxed(),
        condition: Some(IrExpr::and(conditions)),
    };
    Ok(())
}
