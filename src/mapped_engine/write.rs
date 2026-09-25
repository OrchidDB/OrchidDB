//! Mapped mutation boundaries. Read/expression regions are lowered to SQL;
//! writes address the exact tables and columns used by GraphMapping scans.
use super::{MappedGraphEngine, children, find_mutation, mutation_kind};
use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::Lit;
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::{
    CreateEdge, CreateNode, ProjectErrorPolicy, ProjectMode, ProjectionItem, SetMode,
    SetPropertyItem,
};
use crate::ir::rel::mapping::MappedSource;
use crate::ir::rel::sql::{SqlDialect, SqlExecutor, SqlValue, prepare_with_external};
use crate::ir::{GraphPlan, IrExpr, Node, Value};
use std::collections::{BTreeMap, BTreeSet};
type Row = BTreeMap<String, Value>;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone)]
struct Target {
    table: String,
    id: String,
    properties: BTreeMap<String, String>,
    edge: Option<(String, String, String, String)>,
}
fn quote(s: &str) -> String {
    SqlDialect::DuckDb.quote_ident(s)
}
fn table_name(s: &str) -> String {
    datafusion::common::TableReference::from(s)
        .to_vec()
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(".")
}
fn value(v: SqlValue) -> Result<Value> {
    Ok(match v {
        SqlValue::Null => Value::Null,
        SqlValue::Bool(v) => Value::Bool(v),
        SqlValue::Int(v) => Value::Int(v),
        SqlValue::Float(v) => Value::Float(v),
        SqlValue::Text(v) => Value::String(v),
        SqlValue::ExactNumber(v) => {
            Value::BigDecimal(v.parse().map_err(|_| "Invalid exact SQL number")?)
        }
        SqlValue::List(v) => Value::List(v.into_iter().map(value).collect::<Result<_>>()?),
    })
}
fn literal(v: &Value) -> Result<String> {
    Ok(match v {
        Value::Null => "NULL".into(),
        Value::Bool(v) => if *v { "TRUE" } else { "FALSE" }.into(),
        Value::Int(v) | Value::Long(v) => v.to_string(),
        Value::BigInt(v) => v.to_string(),
        Value::BigDecimal(v) => v.to_string(),
        Value::Float(v) if v.is_finite() => format!("{v:?}"),
        Value::String(v) if !v.contains('\0') => format!("'{}'", v.replace('\'', "''")),
        Value::String(v) => v
            .split('\0')
            .map(|s| format!("'{}'", s.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(" || chr(0) || "),
        Value::List(v) => format!(
            "[{}]",
            v.iter().map(literal).collect::<Result<Vec<_>>>()?.join(",")
        ),
        _ => return Err(format!("Mapped SQL cannot encode {}", v.type_name())),
    })
}
fn rows_node(rows: &[Row]) -> Node {
    let bindings = rows
        .iter()
        .flat_map(|r| r.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let values = rows
        .iter()
        .map(|r| {
            bindings
                .iter()
                .map(|k| r.get(k).cloned().unwrap_or(Value::Null))
                .collect()
        })
        .collect();
    Node::GraphValues {
        bindings,
        rows: values,
        bulk: None,
    }
}
fn projection(input: Node, items: Vec<ProjectionItem>) -> Node {
    Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items,
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Box::new(input),
    }
}
fn binding(expr: &IrExpr) -> Result<&str> {
    if let IrExpr::Binding(name) = expr {
        Ok(name)
    } else {
        Err("Mapped write target must be a bound element".into())
    }
}
fn input_mut(node: &mut Node) -> Option<&mut Box<Node>> {
    use Node::*;
    match node {
        GraphReturn { input, .. }
        | GraphFilter { input, .. }
        | GraphProject { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphBind { input, .. }
        | GraphExpand { input, .. }
        | GraphSetProperty { input, .. }
        | GraphCreate { input, .. }
        | GraphDelete { input, .. }
        | GraphSort { input, .. }
        | GraphSlice { input, .. }
        | GraphDistinct { input, .. }
        | GraphAggregate { input, .. }
        | GraphSelect { input, .. }
        | GraphBarrier { input, .. } => Some(input),
        GraphProcedureCall { input, .. } => input.as_mut(),
        GraphApply { left, right, .. } if find_mutation(right).is_none() => Some(left),
        _ => None,
    }
}
fn first_write(node: &Node) -> Result<Option<Node>> {
    if find_mutation(node).is_none() {
        return Ok(None);
    }
    let mut copy = node.clone();
    if let Some(input) = input_mut(&mut copy) {
        if let Some(found) = first_write(input)? {
            return Ok(Some(found));
        }
    } else if children(node).iter().any(|n| find_mutation(n).is_some()) {
        return Err("Mapped mutations require an ordered SQL write pipeline; correlated write branches are not SQL DML".into());
    }
    if mutation_kind(node).is_some() {
        Ok(Some(node.clone()))
    } else {
        Err("Unsupported mapped mutation boundary".into())
    }
}
fn replace(node: &mut Node, target: &Node, replacement: &Node) -> Result<()> {
    if node == target {
        *node = replacement.clone();
        return Ok(());
    }
    if let Some(input) = input_mut(node) {
        replace(input, target, replacement)
    } else {
        Err("Mapped mutation boundary disappeared".into())
    }
}
fn map_entries(expr: Option<&IrExpr>) -> Result<Vec<(String, IrExpr)>> {
    match expr {
        None | Some(IrExpr::Lit(Lit::Null)) => Ok(vec![]),
        Some(IrExpr::Call { name, args }) if name == "cypher_property_map" && args.len() == 1 => {
            // Preserve the Cypher property-domain contract while resolving
            // every key to its declared destination column.
            Ok(map_entries(Some(&args[0]))?.into_iter().map(|(key, value)| (key, IrExpr::Call {
                name: "cypher_property_value".into(), args: vec![value],
            })).collect())
        }
        Some(IrExpr::Call { name, args }) if name == "properties" && args.len() == 1 => {
            map_entries(Some(&args[0]))
        }
        Some(IrExpr::Call { name, args }) if name == "map" => args
            .chunks_exact(2)
            .map(|pair| {
                if let IrExpr::Lit(Lit::String(key)) = &pair[0] {
                    Ok((key.clone(), pair[1].clone()))
                } else {
                    Err("Mapped property keys must be strings".into())
                }
            })
            .collect(),
        Some(IrExpr::Call { name, args }) if name == "map_literal" => {
            let [IrExpr::List(keys), IrExpr::List(values)] = args.as_slice() else {
                return Err("Mapped properties require named columns".into());
            };
            keys.iter()
                .zip(values)
                .map(|(k, v)| {
                    if let IrExpr::Lit(Lit::String(k)) = k {
                        Ok((k.clone(), v.clone()))
                    } else {
                        Err("Mapped property keys must be strings".into())
                    }
                })
                .collect()
        }
        _ => Err("Mapped properties require a property map".into()),
    }
}
impl MappedGraphEngine {
    fn write_target(&self, label: &str, edge: bool) -> Result<Target> {
        let (source, id, properties, endpoints) = if edge {
            let m = self
                .mapping
                .edge(label)
                .ok_or_else(|| format!("No edge mapping for {label}"))?;
            (
                &m.source,
                m.id_column
                    .as_ref()
                    .ok_or("Mapped edge writes require an explicit ID column")?,
                &m.properties,
                Some((
                    m.src_column.clone(),
                    m.dst_column.clone(),
                    m.src_label.clone(),
                    m.dst_label.clone(),
                )),
            )
        } else {
            let m = self
                .mapping
                .node(label)
                .ok_or_else(|| format!("No node mapping for {label}"))?;
            (&m.source, &m.id_column, &m.properties, None)
        };
        let MappedSource::Table(table) = source else {
            return Err("Writes require a table-backed mapping; a query mapping has no unique write destination".into());
        };
        Ok(Target {
            table: table_name(table),
            id: id.clone(),
            properties: properties.clone(),
            edge: endpoints,
        })
    }
    async fn materialize_write_input(
        &mut self,
        node: Node,
        policy: &crate::ir::policy::GraphPlanPolicy,
    ) -> Result<Vec<Row>> {
        let plan = GraphPlan {
            root: Box::new(node),
            policy: policy.clone(),
        };
        let mut lowered = self.with_functions(|| {
            self.backend()
                .lower(&plan, &PropertyGraph::new())
                .map_err(|e| e.to_string())
        })?;
        // Keep renamed scan columns in SQL scope when materializing an entire
        // joined binding row (rather than just a projected query result).
        use datafusion::common::tree_node::{Transformed, TreeNode};
        use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder};
        let mut boundary = 0;
        lowered.plan = lowered
            .plan
            .transform_up(|node| {
                if let LogicalPlan::Join(mut join) = node {
                    boundary += 1;
                    join.left = std::sync::Arc::new(
                        LogicalPlanBuilder::from(join.left.as_ref().clone())
                            .alias(format!("__w_sql_cte_write_left_{boundary}"))?
                            .build()?,
                    );
                    join.right = std::sync::Arc::new(
                        LogicalPlanBuilder::from(join.right.as_ref().clone())
                            .alias(format!("__w_sql_cte_write_right_{boundary}"))?
                            .build()?,
                    );
                    let join = LogicalPlan::Join(join);
                    let mut fields = join
                        .schema()
                        .fields()
                        .iter()
                        .map(|f| {
                            datafusion::logical_expr::Expr::Column(
                                datafusion::common::Column::new_unqualified(f.name()),
                            )
                        })
                        .collect::<Vec<_>>();
                    fields.push(
                        datafusion::prelude::lit(1_i64)
                            .alias(format!("__w_write_guard_{boundary}")),
                    );
                    Ok(Transformed::yes(
                        LogicalPlanBuilder::from(join).project(fields)?.build()?,
                    ))
                } else {
                    Ok(Transformed::no(node))
                }
            })
            .map_err(|e: datafusion::common::DataFusionError| e.to_string())?
            .data;
        let fields = lowered
            .plan
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect::<Vec<_>>();
        let prepared = prepare_with_external(
            &lowered,
            SqlDialect::DuckDb,
            &self.mapping.physical_table_names(),
        )
        .await
        .map_err(|e| e.to_string())?;
        self.executor
            .run_with_tables(&prepared.tables, &prepared.setup, &prepared.query)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|row| {
                fields
                    .iter()
                    .cloned()
                    .zip(row)
                    .map(|(k, v)| Ok((k, value(v)?)))
                    .collect()
            })
            .collect()
    }
    fn run_dml(&mut self, sql: &str) -> Result<Vec<Vec<Value>>> {
        self.executor
            .run(&[], sql)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| r.into_iter().map(value).collect())
            .collect()
    }
    fn unique_id(&mut self, target: &Target, id: &Value) -> Result<()> {
        if matches!(id, Value::Null) {
            return Err("Mapped element ID must not be null".into());
        }
        let rows = self.run_dml(&format!(
            "SELECT count(*) FROM {} WHERE {}={}",
            target.table,
            quote(&target.id),
            literal(id)?
        ))?;
        if rows != vec![vec![Value::Int(1)]] {
            return Err("Mapped source must have exactly one row per graph identifier".into());
        }
        Ok(())
    }
    fn identity<'a>(&self, row: &'a Row, name: &str) -> Result<(Target, &'a Value)> {
        let Some(Value::String(label)) = row.get(&format!("{name}__label")) else {
            return Err(format!("Mapped target {name} has no label"));
        };
        let id = row
            .get(&format!("{name}__id"))
            .ok_or("Mapped target has no ID")?;
        Ok((
            self.write_target(label, row.contains_key(&format!("{name}__src_id")))?,
            id,
        ))
    }
    fn refresh_row(&mut self, row: &mut Row) -> Result<()> {
        let names = row
            .keys()
            .filter_map(|k| k.strip_suffix("__id").map(str::to_owned))
            .collect::<Vec<_>>();
        for name in names {
            if row.get(&format!("{name}__id")) == Some(&Value::Null) {
                continue;
            }
            let (target, id) = self.identity(row, &name)?;
            let id = id.clone();
            let cols = target
                .properties
                .values()
                .map(|v| quote(v))
                .collect::<Vec<_>>();
            if cols.is_empty() {
                continue;
            }
            let data = self.run_dml(&format!(
                "SELECT {} FROM {} WHERE {}={}",
                cols.join(","),
                target.table,
                quote(&target.id),
                literal(&id)?
            ))?;
            if data.len() > 1 {
                return Err("Mapped source has duplicate graph identifiers".into());
            }
            if let Some(values) = data.first() {
                for ((key, _), v) in target.properties.iter().zip(values) {
                    row.insert(format!("{name}__prop__{key}"), v.clone());
                }
            }
        }
        Ok(())
    }
    pub(super) async fn run_mutations(&mut self, plan: &GraphPlan) -> Result<ReturnedBatches> {
        let mut normalized = plan.clone();
        normalize_gremlin_writes(&mut normalized.root)?;
        let plan = &normalized;
        // Validate all control-flow boundaries before the first persistent write.
        let mut check = plan.root.as_ref().clone();
        while let Some(write) = first_write(&check)? {
            if !matches!(
                write,
                Node::GraphCreate { .. } | Node::GraphSetProperty { .. } | Node::GraphDelete { .. }
            ) {
                return Err("Mapped SQL mutation operator is not supported".into());
            }
            let mut w = write.clone();
            let input = input_mut(&mut w)
                .ok_or("Mutation requires input")?
                .as_ref()
                .clone();
            replace(&mut check, &write, &input)?;
        }
        let automatic = !self.executor.in_transaction();
        if automatic {
            self.executor.begin().map_err(|e| e.to_string())?;
        }
        let result = self.execute_write_pipeline(plan).await;
        match result {
            Ok(rows) => {
                if automatic {
                    if let Err(e) = self.executor.commit() {
                        let _ = self.executor.rollback();
                        return Err(e.to_string());
                    }
                }
                Ok(rows)
            }
            Err(e) => {
                let _ = self.executor.rollback();
                Err(e)
            }
        }
    }
    async fn execute_write_pipeline(&mut self, plan: &GraphPlan) -> Result<ReturnedBatches> {
        let mut root = plan.root.as_ref().clone();
        while let Some(write) = first_write(&root)? {
            let rows = match &write {
                Node::GraphCreate {
                    nodes,
                    edges,
                    input,
                    ..
                } => {
                    self.create_mapped(nodes, edges, input, &plan.policy)
                        .await?
                }
                Node::GraphSetProperty { items, input } => {
                    self.update_mapped(items, input, &plan.policy).await?
                }
                Node::GraphDelete {
                    targets,
                    detach,
                    input,
                } => {
                    self.delete_mapped(targets, *detach, input, &plan.policy)
                        .await?
                }
                _ => unreachable!(),
            };
            let replacement = if rows.is_empty() {
                self.empty_write_output(&write)?
            } else {
                rows_node(&rows)
            };
            replace(&mut root, &write, &replacement)?;
        }
        if matches!(&root,Node::GraphReturn{fields,..} if fields.is_empty()) {
            return Ok(ReturnedBatches {
                fields: vec![],
                result_form: plan.policy.result_form,
                batch: arrow::array::RecordBatch::new_empty(std::sync::Arc::new(
                    arrow::datatypes::Schema::empty(),
                )),
            });
        }
        self.run_read_plan(&GraphPlan {
            root: Box::new(root),
            policy: plan.policy.clone(),
        })
        .await
    }
    // Keep the relational input schema for no-match writes; an untyped empty
    // VALUES loses the bindings required by RETURN and downstream aggregates.
    fn empty_write_output(&self, write: &Node) -> Result<Node> {
        let mut copy = write.clone();
        let input = input_mut(&mut copy)
            .ok_or("Mutation requires input")?
            .as_ref()
            .clone();
        let mut fields = Vec::new();
        if let Node::GraphCreate { nodes, edges, .. } = write {
            for (bind, label, edge) in nodes
                .iter()
                .map(|n| {
                    (
                        n.bind.as_deref().unwrap_or("__mapped_created"),
                        n.label.as_str(),
                        false,
                    )
                })
                .chain(edges.iter().map(|e| {
                    (
                        e.bind.as_deref().unwrap_or("__mapped_created_edge"),
                        e.rel_type.as_str(),
                        true,
                    )
                }))
            {
                let target = self.write_target(label, edge)?;
                fields.push(ProjectionItem {
                    alias: format!("{bind}__id"),
                    expr: IrExpr::Lit(Lit::Int(0)),
                });
                fields.push(ProjectionItem {
                    alias: format!("{bind}__label"),
                    expr: IrExpr::lit_str(label),
                });
                for key in target.properties.keys() {
                    fields.push(ProjectionItem {
                        alias: format!("{bind}__prop__{key}"),
                        expr: IrExpr::Lit(Lit::Null),
                    });
                }
                if let Some((_, _, src, dst)) = target.edge {
                    for (side, label) in [("src", src), ("dst", dst)] {
                        fields.push(ProjectionItem {
                            alias: format!("{bind}__{side}_id"),
                            expr: IrExpr::Lit(Lit::Int(0)),
                        });
                        fields.push(ProjectionItem {
                            alias: format!("{bind}__{side}_label"),
                            expr: IrExpr::lit_str(label),
                        });
                    }
                }
            }
        }
        let input = Node::GraphSlice {
            slice: crate::ir::plan::Slice { offset: 0, fetch: Some(0), tail: None },
            input: Box::new(input),
        };
        Ok(if fields.is_empty() {
            input
        } else {
            projection(input, fields)
        })
    }
    async fn create_mapped(
        &mut self,
        nodes: &[CreateNode],
        edges: &[CreateEdge],
        input: &Node,
        policy: &crate::ir::policy::GraphPlanPolicy,
    ) -> Result<Vec<Row>> {
        let mut rows = self.materialize_write_input(input.clone(), policy).await?;
        for node in nodes {
            if node.labels.as_ref().is_some_and(|labels| labels.as_slice() != [node.label.clone()]) {
                return Err("Mapped creation requires declared storage for unlabelled or multiple labels".into());
            }
        }
        for node in nodes {
            let target = self.write_target(&node.label, false)?;
            rows = self
                .insert_mapped(
                    rows,
                    &target,
                    node.bind.as_deref().unwrap_or("__mapped_created"),
                    &node.label,
                    map_entries(node.properties.as_ref())?,
                    None,
                    policy,
                )
                .await?;
        }
        for edge in edges {
            let target = self.write_target(&edge.rel_type, true)?;
            rows = self
                .insert_mapped(
                    rows,
                    &target,
                    edge.bind.as_deref().unwrap_or("__mapped_created_edge"),
                    &edge.rel_type,
                    map_entries(edge.properties.as_ref())?,
                    Some((&edge.src, &edge.dst)),
                    policy,
                )
                .await?;
        }
        Ok(rows)
    }
    async fn insert_mapped(
        &mut self,
        rows: Vec<Row>,
        target: &Target,
        bind: &str,
        label: &str,
        props: Vec<(String, IrExpr)>,
        endpoints: Option<(&str, &str)>,
        policy: &crate::ir::policy::GraphPlanPolicy,
    ) -> Result<Vec<Row>> {
        let mut columns = Vec::new();
        let mut items = Vec::new();
        for (i, (key, expr)) in props.iter().enumerate() {
            let col = if key == "__mapped_id" {
                &target.id
            } else {
                target
                    .properties
                    .get(key)
                    .ok_or_else(|| format!("Unmapped insert property {key}"))?
            };
            if columns.contains(col) {
                return Err("Two insert properties address the same mapped column".into());
            }
            columns.push(col.clone());
            items.push(ProjectionItem {
                alias: format!("__mapped_value_{i}"),
                expr: expr.clone(),
            });
        }
        if rows.is_empty() {
            return Ok(rows);
        }
        let mut rows = self
            .materialize_write_input(projection(rows_node(&rows), items), policy)
            .await?;
        for row in &mut rows {
            let mut cols = columns.clone();
            let mut values = (0..props.len())
                .map(|i| {
                    row.remove(&format!("__mapped_value_{i}"))
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            if let (Some((src, dst)), Some((sc, dc, sl, dl))) = (endpoints, &target.edge) {
                for (binding, column, expected) in [(src, sc, sl), (dst, dc, dl)] {
                    if row.get(&format!("{binding}__label"))
                        != Some(&Value::String(expected.clone()))
                    {
                        return Err(
                            "Inserted edge endpoints do not match their node mappings".into()
                        );
                    }
                    let (node, id) = self.identity(row, binding)?;
                    self.unique_id(&node, id)?;
                    if cols.contains(column) {
                        return Err("Edge endpoint columns cannot be assigned as properties".into());
                    }
                    cols.push(column.clone());
                    values.push(id.clone());
                }
            }
            if !cols.contains(&target.id) {
                // The mapping defines integer identity. Allocate within the write transaction.
                let result = self.run_dml(&format!(
                    "SELECT coalesce(max({}),0)+1 FROM {}",
                    quote(&target.id),
                    target.table
                ))?;
                cols.push(target.id.clone());
                values.push(result[0][0].clone());
            }
            let id = values[cols.iter().position(|c| c == &target.id).unwrap()].clone();
            if !matches!(id, Value::Int(_) | Value::Long(_)) {
                return Err("Mapped graph IDs must be non-null integers".into());
            }
            let duplicate = self.run_dml(&format!(
                "SELECT count(*) FROM {} WHERE {}={}",
                target.table,
                quote(&target.id),
                literal(&id)?
            ))?;
            if duplicate != vec![vec![Value::Int(0)]] {
                return Err("Inserted graph ID already exists in the mapped table".into());
            }
            self.run_dml(&format!(
                "INSERT INTO {} ({}) VALUES ({})",
                target.table,
                cols.iter().map(|s| quote(s)).collect::<Vec<_>>().join(","),
                values
                    .iter()
                    .map(literal)
                    .collect::<Result<Vec<_>>>()?
                    .join(",")
            ))?;
            row.retain(|key, _| key != bind && !key.starts_with(&format!("{bind}__")));
            row.insert(format!("{bind}__id"), id);
            row.insert(format!("{bind}__label"), Value::String(label.into()));
            if let Some((sc, dc, sl, dl)) = &target.edge {
                for (suffix, col, lab) in [("src", sc, sl), ("dst", dc, dl)] {
                    row.insert(
                        format!("{bind}__{suffix}_id"),
                        values[cols.iter().position(|c| c == col).unwrap()].clone(),
                    );
                    row.insert(
                        format!("{bind}__{suffix}_label"),
                        Value::String(lab.clone()),
                    );
                }
            }
            self.refresh_row(row)?;
        }
        Ok(rows)
    }
    async fn update_mapped(
        &mut self,
        items: &[SetPropertyItem],
        input: &Node,
        policy: &crate::ir::policy::GraphPlanPolicy,
    ) -> Result<Vec<Row>> {
        let mut rows = self.materialize_write_input(input.clone(), policy).await?;
        for item in items {
            let name = binding(&item.target)?;
            let props = match item.mode {
                SetMode::AddLabels | SetMode::RemoveLabels => return Err("Mapped label updates require declared mutable label storage".into()),
                SetMode::Property => vec![(item.key.clone(), item.value.clone())],
                SetMode::Merge | SetMode::Replace => map_entries(Some(&item.value))?,
            };
            let mut seen = BTreeSet::new();
            for row in &rows {
                if row.get(&format!("{name}__id")) == Some(&Value::Null) {
                    continue;
                }
                let (target, id) = self.identity(row, name)?;
                self.unique_id(&target, id)?;
                if !seen.insert((target.table, literal(id)?)) {
                    return Err("Mapped update matched duplicate graph identifiers".into());
                }
            }
            if rows.is_empty() {
                continue;
            }
            for row in &mut rows {
                self.refresh_row(row)?;
            }
            // Evaluate the whole RHS map before replacing any of its source columns.
            let exprs = props
                .iter()
                .enumerate()
                .map(|(i, (_, expr))| ProjectionItem {
                    alias: format!("__mapped_update_{i}"),
                    expr: expr.clone(),
                })
                .collect();
            rows = self
                .materialize_write_input(projection(rows_node(&rows), exprs), policy)
                .await?;
            for row in &mut rows {
                if row.get(&format!("{name}__id")) == Some(&Value::Null) {
                    for i in 0..props.len() {
                        row.remove(&format!("__mapped_update_{i}"));
                    }
                    continue;
                }
                let (target, id) = self.identity(row, name)?;
                let id = id.clone();
                let protected = |column: &String| {
                    column == &target.id
                        || target
                            .edge
                            .as_ref()
                            .is_some_and(|(a, b, _, _)| column == a || column == b)
                };
                let mut assignments = BTreeMap::new();
                if item.mode == SetMode::Replace {
                    for col in target.properties.values() {
                        if !protected(col) {
                            assignments.insert(col.clone(), Value::Null);
                        }
                    }
                }
                let mut specified = BTreeSet::new();
                for (i, (key, _)) in props.iter().enumerate() {
                    let col = target
                        .properties
                        .get(key)
                        .ok_or_else(|| format!("Unmapped update property {key}"))?;
                    if protected(col) {
                        return Err(
                            "Mapped updates cannot change graph identifiers or edge endpoints"
                                .into(),
                        );
                    }
                    if !specified.insert(col.clone()) {
                        return Err("Two update properties address the same mapped column".into());
                    }
                    assignments.insert(
                        col.clone(),
                        row.remove(&format!("__mapped_update_{i}"))
                            .unwrap_or(Value::Null),
                    );
                }
                if !assignments.is_empty() {
                    let assignments = assignments
                        .iter()
                        .map(|(col, v)| Ok(format!("{}={}", quote(col), literal(v)?)))
                        .collect::<Result<Vec<_>>>()?
                        .join(",");
                    self.run_dml(&format!(
                        "UPDATE {} SET {assignments} WHERE {}={}",
                        target.table,
                        quote(&target.id),
                        literal(&id)?
                    ))?;
                }
                self.refresh_row(row)?;
            }
        }
        Ok(rows)
    }
    async fn delete_mapped(
        &mut self,
        targets: &[IrExpr],
        detach: bool,
        input: &Node,
        policy: &crate::ir::policy::GraphPlanPolicy,
    ) -> Result<Vec<Row>> {
        let rows = self.materialize_write_input(input.clone(), policy).await?;
        let mut elements = BTreeMap::new();
        for expr in targets {
            let name = binding(expr)?;
            for row in &rows {
                if row.get(&format!("{name}__id")) == Some(&Value::Null) {
                    continue;
                }
                let (target, id) = self.identity(row, name)?;
                self.unique_id(&target, id)?;
                elements.insert(
                    (target.table.clone(), literal(id)?),
                    (target, id.clone(), row[&format!("{name}__label")].clone()),
                );
            }
        }
        // Edges first, so DELETE n,r can remove the connected node without DETACH.
        let mut ordered = elements.into_values().collect::<Vec<_>>();
        ordered.sort_by_key(|(t, _, _)| t.edge.is_none());
        for (target, id, label) in ordered {
            if target.edge.is_none() {
                for kind in self.mapping.rel_types() {
                    let m = self.mapping.edge(&kind).unwrap();
                    let mut predicates = Vec::new();
                    if label == Value::String(m.src_label.clone()) {
                        predicates.push(format!("{}={}", quote(&m.src_column), literal(&id)?));
                    }
                    if label == Value::String(m.dst_label.clone()) {
                        predicates.push(format!("{}={}", quote(&m.dst_column), literal(&id)?));
                    }
                    if predicates.is_empty() {
                        continue;
                    }
                    let MappedSource::Table(table) = &m.source else {
                        return Err(
                            "Incident edges require a table-backed mapping for deletion".into()
                        );
                    };
                    let edge_table = table_name(table);
                    let predicate = predicates.join(" OR ");
                    if detach {
                        self.run_dml(&format!("DELETE FROM {} WHERE {predicate}", edge_table))?;
                    } else if self.run_dml(&format!(
                        "SELECT count(*) FROM {} WHERE {predicate}",
                        edge_table
                    ))? != vec![vec![Value::Int(0)]]
                    {
                        return Err(
                            "Cannot delete a mapped node with incident edges; use DETACH DELETE"
                                .into(),
                        );
                    }
                }
            }
            self.run_dml(&format!(
                "DELETE FROM {} WHERE {}={}",
                target.table,
                quote(&target.id),
                literal(&id)?
            ))?;
        }
        Ok(rows)
    }
}

// The production Gremlin frontend uses write procedures for its richer property
// model. Resolve their ordinary table operations during mapped SQL planning.
fn constant(expr: &IrExpr, node: &Node) -> Option<String> {
    match expr {
        IrExpr::Lit(Lit::String(s)) => Some(s.clone()),
        IrExpr::Call { name, args }
            if name == "gremlin_token_literal" && args == &vec![IrExpr::lit_str("id")] =>
        {
            Some("__mapped_id".into())
        }
        IrExpr::Binding(name) => {
            if let Node::GraphProject { items, input, .. } = node {
                if let Some(item) = items.iter().find(|i| &i.alias == name) {
                    return constant(&item.expr, input);
                }
            }
            children(node).into_iter().find_map(|n| constant(expr, n))
        }
        _ => None,
    }
}
fn property_map(entries: Vec<(String, IrExpr)>) -> IrExpr {
    IrExpr::Call {
        name: "map".into(),
        args: entries
            .into_iter()
            .flat_map(|(k, v)| [IrExpr::lit_str(k), v])
            .collect(),
    }
}
fn normalize_gremlin_writes(node: &mut Node) -> Result<()> {
    if let Some(input) = input_mut(node) {
        normalize_gremlin_writes(input)?;
    }
    if let Node::GraphProject { items, .. } = node {
        for item in items {
            if matches!(&item.expr,IrExpr::Call{name,args} if name=="gremlin_token_literal"&&args==&vec![IrExpr::lit_str("id")])
            {
                item.expr = IrExpr::lit_str("__mapped_id");
            }
        }
    }
    let Node::GraphProcedureCall {
        name,
        args,
        input: Some(input),
        ..
    } = node
    else {
        return Ok(());
    };
    let values = args.iter().map(|a| a.value.clone()).collect::<Vec<_>>();
    match (name.as_str(), values.as_slice()) {
        ("gremlin.mutation.add_vertex", [label, props]) => {
            let label =
                constant(label, input).ok_or("Mapped vertex creation requires a mapped label")?;
            *node = Node::GraphCreate {
                graph: "default".into(),
                nodes: vec![CreateNode {
                    bind: Some("current".into()),
                    label,
                    labels: None,
                    properties: Some(props.clone()),
                }],
                edges: vec![],
                input: input.clone(),
            };
        }
        ("gremlin.mutation.add_edge", [label, src, dst, props]) => {
            let label = constant(label, input)
                .ok_or("Mapped edge creation requires a mapped relationship type")?;
            *node = Node::GraphCreate {
                graph: "default".into(),
                nodes: vec![],
                edges: vec![CreateEdge {
                    bind: Some("current".into()),
                    rel_type: label,
                    src: binding(src)?.into(),
                    dst: binding(dst)?.into(),
                    properties: Some(props.clone()),
                }],
                input: input.clone(),
            };
        }
        ("gremlin.mutation.property_native", [target, key, value, cardinality, meta])
        | ("gremlin.mutation.property", [target, key, value, cardinality, meta]) => {
            if !map_entries(Some(meta))?.is_empty() {
                return Err("Mapped columns do not have meta-property records".into());
            }
            let key =
                constant(key, input).ok_or("Mapped property requires a known property key")?;
            let binding = binding(target)?;
            let mut folded = input.as_ref().clone();
            if let Node::GraphCreate { nodes, edges, .. } = &mut folded {
                let props = if nodes.len() == 1 && nodes[0].bind.as_deref() == Some(binding) {
                    Some(&mut nodes[0].properties)
                } else if edges.len() == 1 && edges[0].bind.as_deref() == Some(binding) {
                    Some(&mut edges[0].properties)
                } else {
                    None
                };
                if let Some(props) = props {
                    let mut entries = map_entries(props.as_ref())?;
                    if entries.iter().any(|(k, _)| k == &key) {
                        return Err("Mapped scalar columns cannot store duplicate properties".into());
                    }
                    entries.push((key, value.clone()));
                    *props = Some(property_map(entries));
                    *node = folded;
                    return Ok(());
                }
            }
            if constant(cardinality, input).as_deref() != Some("single") {
                return Err("Mapped property updates require single cardinality".into());
            }
            *node = Node::GraphSetProperty {
                items: vec![SetPropertyItem {
                    target: target.clone(),
                    key,
                    mode: SetMode::Property,
                    value: value.clone(),
                }],
                input: input.clone(),
            };
        }
        ("gremlin.mutation.property", [target, key, value]) => {
            let key =
                constant(key, input).ok_or("Mapped property requires a known property key")?;
            *node = Node::GraphSetProperty {
                items: vec![SetPropertyItem {
                    target: target.clone(),
                    key,
                    mode: SetMode::Property,
                    value: value.clone(),
                }],
                input: input.clone(),
            };
        }
        _ => {}
    }
    Ok(())
}
