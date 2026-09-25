//! Cypher semantic defaults and AST analysis used before Graph IR lowering.

use crate::language::cypher::planner::CypherSemanticError;
mod expression_types;
use expression_types::{
    projected_expr_kind, unwind_element_kind, validate_expr_kinds, validate_list_source,
    validate_literal_list_types, validate_regexp_replace_option, validate_static_expression_types,
};
mod validation;
use validation::{
    ProcedureMode, is_variable_length, pattern_binding_names, procedure_mode, procedure_yields,
    validate_node_binding, validate_order_by_supported, validate_path_binding,
    validate_pattern_predicate_scope, validate_relationship_binding, validate_union_outputs,
    validate_unique, validate_with_projection_aliases,
};
mod references;
use references::{collect_free_variables, remove_local_exists_bindings, scope_from_candidates};
mod aggregates;
use aggregates::{contains_aggregate, validate_iteration_aggregate};

use std::collections::{BTreeMap, BTreeSet};

use crate::language::cypher::ast::{
    BinaryOp, Clause, ExistsSubquery, Expr, Literal, NodePattern, PatternElement, PatternPart,
    ProjectionBody, Query, UnaryOp,
};
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};

pub const DEFAULT_GRAPH: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingKind {
    Unknown,
    Node,
    Relationship,
    RecursiveRelationship,
    Bool,
    Int,
    Float,
    String,
    Date,
    Timestamp,
    TimestampMs,
    Interval,
    InternalId,
    ListInt,
    FixedListInt,
    ListNode,
    ListRelationship,
    StructA,
    StructInt,
    StructListInt,
    StructDescription,
    MapStringInt,
    UnionMovieGrade,
    Value,
}

impl BindingKind {
    pub(crate) const fn cypher_type_name(self) -> &'static str {
        match self {
            BindingKind::Unknown => "ANY",
            BindingKind::Node => "NODE",
            BindingKind::Relationship => "REL",
            BindingKind::RecursiveRelationship => "RECURSIVE_REL",
            BindingKind::Bool => "BOOL",
            BindingKind::Int => "INT64",
            BindingKind::Float => "DOUBLE",
            BindingKind::String => "STRING",
            BindingKind::Date => "DATE",
            BindingKind::Timestamp => "TIMESTAMP",
            BindingKind::TimestampMs => "TIMESTAMP_MS",
            BindingKind::Interval => "INTERVAL",
            BindingKind::InternalId => "INTERNAL_ID",
            BindingKind::ListInt => "INT64[]",
            BindingKind::FixedListInt => "INT64[4]",
            BindingKind::ListNode => "NODE[]",
            BindingKind::ListRelationship => "REL[]",
            BindingKind::StructA => "STRUCT(a INT64)",
            BindingKind::StructInt => "STRUCT(x INT64)",
            BindingKind::StructListInt => "STRUCT(x INT64[])",
            BindingKind::StructDescription => {
                "STRUCT(rating DOUBLE, stars INT8, views INT64, release TIMESTAMP, release_ns TIMESTAMP_NS, release_ms TIMESTAMP_MS, release_sec TIMESTAMP_SEC, release_tz TIMESTAMP_TZ, film DATE, u8 UINT8, u16 UINT16, u32 UINT32, u64 UINT64, hugedata INT128)"
            }
            BindingKind::MapStringInt => "MAP(STRING, INT64)",
            BindingKind::UnionMovieGrade => "UNION(credit BOOL, grade1 DOUBLE, grade2 INT64)",
            BindingKind::Value => "ANY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzedQuery<'a> {
    pub query: &'a Query,
    pub output_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticOutput {
    name: String,
    kind: BindingKind,
}

#[derive(Debug, Clone, Default)]
struct SemanticScope {
    bindings: BTreeMap<String, BindingKind>,
}

impl SemanticScope {
    fn contains(&self, binding: &str) -> bool {
        self.bindings.contains_key(binding)
    }

    fn insert(&mut self, binding: impl Into<String>, kind: BindingKind) {
        self.bindings.insert(binding.into(), kind);
    }

    fn kind(&self, binding: &str) -> Option<BindingKind> {
        self.bindings.get(binding).copied()
    }

    fn fields(&self) -> Vec<SemanticOutput> {
        self.bindings
            .iter()
            .map(|(name, kind)| SemanticOutput {
                name: name.clone(),
                kind: *kind,
            })
            .collect()
    }

    fn field_set(&self) -> BTreeSet<String> {
        self.bindings.keys().cloned().collect()
    }

    fn replace(&mut self, outputs: Vec<SemanticOutput>) {
        self.bindings = outputs
            .into_iter()
            .map(|output| (output.name, output.kind))
            .collect();
    }
}

pub fn analyze_query(query: &Query) -> CypherPlanResult<AnalyzedQuery<'_>> {
    let mut analyzer = SemanticAnalyzer::default();
    let mut scope = SemanticScope::default();
    let outputs = analyzer.analyze_query_with_scope(query, &mut scope)?;
    let output_fields = outputs.into_iter().map(|output| output.name).collect();
    Ok(AnalyzedQuery {
        query,
        output_fields,
    })
}

#[derive(Debug, Default)]
struct SemanticAnalyzer {
    synthetic_counter: usize,
}

impl SemanticAnalyzer {
    fn analyze_query_with_scope(
        &mut self,
        query: &Query,
        scope: &mut SemanticScope,
    ) -> CypherPlanResult<Vec<SemanticOutput>> {
        let initial_scope = scope.clone();
        let root_outputs = self.analyze_query_body(query, scope)?;
        let union_mode = query.unions.first().map(|branch| branch.all);
        for branch in &query.unions {
            if union_mode.is_some_and(|all| all != branch.all) {
                return Err(CypherPlanError::Invalid(
                    "Binder exception: Union and union all can not be used together.".to_string(),
                ));
            }
            let mut branch_scope = initial_scope.clone();
            let branch_outputs = self.analyze_query_with_scope(&branch.query, &mut branch_scope)?;
            validate_union_outputs(&root_outputs, &branch_outputs)?;
        }
        Ok(root_outputs)
    }

    fn analyze_query_body(
        &mut self,
        query: &Query,
        scope: &mut SemanticScope,
    ) -> CypherPlanResult<Vec<SemanticOutput>> {
        let mut result_fields = None;
        for clause in &query.clauses {
            match clause {
                Clause::Merge(clause) => {
                    // MERGE's pattern binds like MATCH — the create arm
                    // introduces exactly the same variables.
                    self.analyze_pattern_part_with_clause(&clause.pattern, scope, None)?;
                    for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                        for expr in merge_set_item_exprs(item) {
                            self.validate_expr_scope(expr, scope, "MERGE SET")?;
                        }
                    }
                }
                Clause::Match(clause) => {
                    let mut clause_relationships = BTreeMap::new();
                    for part in &clause.patterns {
                        self.analyze_pattern_part_with_clause(
                            part,
                            scope,
                            Some(&mut clause_relationships),
                        )?;
                    }
                    if let Some(predicate) = &clause.predicate {
                        self.validate_expr_scope(predicate, scope, "WHERE predicate")?;
                    }
                }
                Clause::Unwind(clause) => {
                    if let Expr::List(items) = &clause.expr {
                        // UNWIND accepts heterogeneous list literals
                        // (bound as ANY[]); validate elements only.
                        for item in items {
                            self.validate_expr_scope(item, scope, "UNWIND expression")?;
                        }
                    } else {
                        self.validate_expr_scope(&clause.expr, scope, "UNWIND expression")?;
                        validate_list_source(&clause.expr, scope)?;
                    }
                    if scope.contains(&clause.alias) {
                        return Err(CypherPlanError::Invalid(format!(
                            "Binder exception: Variable {} already exists.",
                            clause.alias
                        ))
                        .classified(CypherSemanticError::VariableAlreadyBound));
                    }
                    scope.insert(
                        clause.alias.clone(),
                        unwind_element_kind(&clause.expr, scope),
                    );
                }
                Clause::Call(clause) => {
                    let (source_yields, alias_yields) = procedure_yields(clause);
                    validate_unique(
                        &alias_yields,
                        "procedure YIELD contains duplicate output variables",
                    )?;
                    if !clause.standalone
                        && source_yields.is_empty()
                        && matches!(procedure_mode(&clause.name), ProcedureMode::Read)
                    {
                        return Err(CypherPlanError::Invalid(format!(
                            "procedure `{}` declares result fields and requires explicit YIELD inside a larger query",
                            clause.name
                        )));
                    }
                    for arg in &clause.args {
                        self.validate_expr_scope(arg, scope, "procedure argument")?;
                    }
                    let visible = scope.field_set();
                    let rebound = alias_yields
                        .iter()
                        .filter(|yield_name| visible.contains(*yield_name))
                        .cloned()
                        .collect::<Vec<_>>();
                    if !rebound.is_empty() {
                        return Err(CypherPlanError::Invalid(format!(
                            "procedure `{}` tries to rebind variables already in scope: {}",
                            clause.name,
                            rebound.join(", ")
                        )));
                    }
                    for output in &alias_yields {
                        scope.insert(output.clone(), BindingKind::Unknown);
                    }
                    if let Some(predicate) = &clause.predicate {
                        self.validate_expr_scope(predicate, scope, "WHERE predicate")?;
                    }
                }
                Clause::Create(clause) => {
                    for part in &clause.patterns {
                        self.validate_create_node(&part.element.start, scope)?;
                        for chain in &part.element.chains {
                            if let Some(properties) = &chain.relationship.properties {
                                self.validate_expr_scope(
                                    properties,
                                    scope,
                                    "CREATE relationship properties",
                                )?;
                            }
                            if let Some(variable) = &chain.relationship.variable {
                                if scope.contains(variable) {
                                    return Err(CypherPlanError::Invalid(format!(
                                        "Binder exception: Variable {variable} already exists."
                                    )));
                                }
                                scope.insert(variable.clone(), BindingKind::Relationship);
                            }
                            self.validate_create_node(&chain.node, scope)?;
                        }
                    }
                }
                Clause::Set(clause) => {
                    for item in &clause.items {
                        match item {
                            crate::language::cypher::ast::SetItem::Property {
                                target,
                                value,
                                ..
                            } => {
                                self.validate_expr_scope(target, scope, "SET property target")?;
                                self.validate_expr_scope(value, scope, "SET property value")?;
                            }
                            crate::language::cypher::ast::SetItem::Replace { variable, value }
                            | crate::language::cypher::ast::SetItem::Merge { variable, value } => {
                                if !scope.contains(variable) {
                                    return Err(CypherPlanError::Invalid(format!(
                                        "SET references variables that are not in scope: {variable}"
                                    ))
                                    .classified(CypherSemanticError::UndefinedVariable));
                                }
                                self.validate_expr_scope(value, scope, "SET value")?;
                            }
                            crate::language::cypher::ast::SetItem::Labels { variable, .. } => {
                                if !scope.contains(variable) {
                                    return Err(CypherPlanError::Invalid(format!(
                                        "SET references variables that are not in scope: {variable}"
                                    ))
                                    .classified(CypherSemanticError::UndefinedVariable));
                                }
                            }
                        }
                    }
                }
                Clause::Delete(clause) => {
                    for expr in &clause.expressions {
                        self.validate_expr_scope(expr, scope, "DELETE expression")?;
                    }
                }
                Clause::With(clause) => {
                    validate_with_projection_aliases(&clause.projection)?;
                    let outputs = self.analyze_projection_body(&clause.projection, scope)?;
                    if let Some(predicate) = &clause.predicate {
                        self.validate_with_predicate(
                            predicate,
                            &clause.projection,
                            scope,
                            &outputs,
                        )?;
                    }
                    let output_fields = outputs.clone();
                    scope.replace(outputs);
                    result_fields = Some(output_fields);
                }
                Clause::Return(clause) => {
                    let outputs = self.analyze_projection_body(&clause.projection, scope)?;
                    result_fields = Some(outputs);
                }
            }
        }
        Ok(result_fields.unwrap_or_else(|| scope.fields()))
    }

    fn analyze_pattern_part(
        &mut self,
        part: &PatternPart,
        scope: &mut SemanticScope,
    ) -> CypherPlanResult<()> {
        self.analyze_pattern_part_with_clause(part, scope, None)
    }

    fn analyze_pattern_part_with_clause(
        &mut self,
        part: &PatternPart,
        scope: &mut SemanticScope,
        mut clause_relationships: Option<&mut BTreeMap<String, BindingKind>>,
    ) -> CypherPlanResult<()> {
        validate_path_binding(part, scope)?;
        let mut local_kinds = scope.bindings.clone();
        let mut local_relationships = BTreeSet::new();
        let declared = pattern_binding_names(part);
        let mut allowed = scope.field_set();
        allowed.extend(declared.iter().cloned());

        if let Some(properties) = &part.element.start.properties {
            self.validate_expr_refs(properties, &allowed, "pattern property expression")?;
        }
        if let Some(path) = &part.variable {
            if !scope.contains(path) {
                scope.insert(path.clone(), BindingKind::RecursiveRelationship);
                local_kinds.insert(path.clone(), BindingKind::RecursiveRelationship);
                if let Some(bindings) = clause_relationships.as_deref_mut() {
                    bindings.insert(path.clone(), BindingKind::RecursiveRelationship);
                }
            }
        }
        if let Some(node) = &part.element.start.variable {
            validate_node_binding(node, &local_kinds)?;
            if !scope.contains(node) {
                scope.insert(node.clone(), BindingKind::Node);
                local_kinds.insert(node.clone(), BindingKind::Node);
            }
        }

        for chain in &part.element.chains {
            if let Some(node) = &chain.node.variable {
                local_kinds.entry(node.clone()).or_insert(BindingKind::Node);
            }
            let variable_length = is_variable_length(&chain.relationship.range);
            if let Some(rel) = &chain.relationship.variable {
                let expected = if variable_length {
                    BindingKind::RecursiveRelationship
                } else {
                    BindingKind::Relationship
                };
                let repeated_in_part = local_relationships.contains(rel);
                let repeated_in_clause = clause_relationships
                    .as_ref()
                    .and_then(|bindings| bindings.get(rel).copied());
                if repeated_in_part {
                    return Err(CypherPlanError::Invalid(format!(
                        "Binder exception: Bind relationship {rel} to relationship with same name is not supported."
                    )));
                }
                if let Some(previous) = repeated_in_clause {
                    if previous == expected {
                        return Err(CypherPlanError::Invalid(format!(
                            "Binder exception: Bind relationship {rel} to relationship with same name is not supported."
                        )));
                    }
                    return Err(CypherPlanError::Invalid(format!(
                        "Binder exception: {rel} has data type {} but {} was expected.",
                        previous.cypher_type_name(),
                        expected.cypher_type_name()
                    )));
                }
                validate_relationship_binding(rel, expected, &local_kinds)?;
                if !scope.contains(rel) {
                    scope.insert(rel.clone(), expected);
                }
                local_kinds.insert(rel.clone(), expected);
                local_relationships.insert(rel.clone());
                if let Some(bindings) = clause_relationships.as_deref_mut() {
                    bindings.insert(rel.clone(), expected);
                }
            }
            if let Some(properties) = &chain.relationship.properties {
                self.validate_expr_refs(properties, &allowed, "pattern property expression")?;
            }
            if let Some(properties) = &chain.node.properties {
                self.validate_expr_refs(properties, &allowed, "pattern property expression")?;
            }
            if let Some(node) = &chain.node.variable {
                validate_node_binding(node, &local_kinds)?;
                if !scope.contains(node) {
                    scope.insert(node.clone(), BindingKind::Node);
                }
                local_kinds.insert(node.clone(), BindingKind::Node);
            }
        }
        Ok(())
    }

    fn analyze_projection_body(
        &mut self,
        body: &ProjectionBody,
        scope: &SemanticScope,
    ) -> CypherPlanResult<Vec<SemanticOutput>> {
        if body.include_existing && scope.bindings.is_empty() {
            return Err(CypherPlanError::Invalid(
                "RETURN or WITH * is not allowed when there are no variables in scope".to_string(),
            ));
        }
        for item in &body.items {
            self.validate_expr_scope(&item.expr, scope, "projection expression")?;
        }
        let output_fields = self.projection_outputs(body, scope);
        validate_unique(
            &output_fields
                .iter()
                .map(|output| output.name.clone())
                .collect::<Vec<_>>(),
            "projection contains duplicate column names",
        )?;

        let mut order_scope = scope.clone();
        for output in &output_fields {
            order_scope.insert(output.name.clone(), output.kind);
        }
        let mut order_candidates = scope.field_set();
        order_candidates.extend(output_fields.iter().map(|output| output.name.clone()));
        for item in &body.order_by {
            validate_order_by_supported(&item.expr, &order_scope)?;
            self.validate_expr_refs(&item.expr, &order_candidates, "ORDER BY expression")?;
        }
        if let Some(skip) = &body.skip {
            self.validate_expr_refs(skip, &order_candidates, "SKIP expression")?;
        }
        if let Some(limit) = &body.limit {
            self.validate_expr_refs(limit, &order_candidates, "LIMIT expression")?;
        }
        Ok(output_fields)
    }

    fn validate_with_predicate(
        &mut self,
        predicate: &Expr,
        body: &ProjectionBody,
        source_scope: &SemanticScope,
        outputs: &[SemanticOutput],
    ) -> CypherPlanResult<()> {
        let source_fields = source_scope.field_set();
        let projected_fields = outputs
            .iter()
            .map(|output| output.name.clone())
            .collect::<BTreeSet<_>>();
        let has_aggregate = body.items.iter().any(|item| contains_aggregate(&item.expr));
        if has_aggregate {
            return self.validate_expr_refs(predicate, &projected_fields, "WHERE predicate");
        }
        let mut candidates = source_fields;
        candidates.extend(projected_fields);
        self.validate_expr_refs(predicate, &candidates, "WHERE predicate")
    }

    fn projection_outputs(
        &mut self,
        body: &ProjectionBody,
        scope: &SemanticScope,
    ) -> Vec<SemanticOutput> {
        let mut outputs = Vec::new();
        if body.include_existing {
            outputs.extend(scope.bindings.iter().map(|(binding, kind)| SemanticOutput {
                name: binding.clone(),
                kind: *kind,
            }));
        }
        for item in &body.items {
            let name = item
                .alias
                .clone()
                .or_else(|| item.expr.variable_name().map(ToString::to_string))
                .unwrap_or_else(|| self.synthetic("expr"));
            let kind = match (&item.alias, &item.expr) {
                (None, Expr::Variable(binding)) => {
                    scope.kind(binding).unwrap_or(BindingKind::Unknown)
                }
                (Some(alias), Expr::Variable(binding)) if alias == binding => {
                    scope.kind(binding).unwrap_or(BindingKind::Unknown)
                }
                _ => projected_expr_kind(&item.expr, scope),
            };
            outputs.push(SemanticOutput { name, kind });
        }
        outputs
    }

    /// Validate one node pattern inside CREATE. A variable already in scope
    /// re-references that node, which is how `MATCH (a) CREATE (a)-[:R]->(b)`
    /// and self-loops work; restating labels or properties on it is not a
    /// reference but a redefinition, and is rejected.
    fn validate_create_node(
        &mut self,
        pattern: &NodePattern,
        scope: &mut SemanticScope,
    ) -> CypherPlanResult<()> {
        if let Some(variable) = &pattern.variable {
            if scope.contains(variable) {
                if !pattern.labels.is_empty() || pattern.properties.is_some() {
                    return Err(CypherPlanError::Invalid(format!(
                        "Binder exception: Variable {variable} already exists."
                    )));
                }
                return Ok(());
            }
        }
        if let Some(properties) = &pattern.properties {
            self.validate_expr_scope(properties, scope, "CREATE properties")?;
        }
        if let Some(variable) = &pattern.variable {
            scope.insert(variable.clone(), BindingKind::Node);
        }
        Ok(())
    }

    fn validate_expr_scope(
        &mut self,
        expr: &Expr,
        scope: &SemanticScope,
        clause: &str,
    ) -> CypherPlanResult<()> {
        validate_expr_kinds(expr, scope)?;
        self.validate_expr_refs(expr, &scope.field_set(), clause)
    }

    fn validate_expr_refs(
        &mut self,
        expr: &Expr,
        candidates: &BTreeSet<String>,
        clause: &str,
    ) -> CypherPlanResult<()> {
        validate_static_expression_types(expr)?;
        self.validate_nested_semantics(expr, candidates)?;
        let mut refs = BTreeSet::new();
        collect_free_variables(expr, &mut BTreeSet::new(), &mut refs);
        remove_local_exists_bindings(expr, &mut refs);
        let missing = refs
            .into_iter()
            .filter(|name| !candidates.contains(name))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else if missing.len() == 1 {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: Variable {} is not in scope.",
                missing[0]
            ))
            .classified(CypherSemanticError::UndefinedVariable))
        } else {
            Err(CypherPlanError::Invalid(format!(
                "{clause} references variables that are not in scope: {}",
                missing.join(", ")
            ))
            .classified(CypherSemanticError::UndefinedVariable))
        }
    }

    fn validate_nested_semantics(
        &mut self,
        expr: &Expr,
        candidates: &BTreeSet<String>,
    ) -> CypherPlanResult<()> {
        match expr {
            Expr::Exists(exists) => self.validate_exists(exists, candidates),
            Expr::PatternPredicate(patterns) => {
                validate_pattern_predicate_scope(patterns, candidates)
            }
            Expr::PatternComprehension {
                pattern,
                predicate,
                map,
                ..
            } => {
                let mut scope = scope_from_candidates(candidates);
                self.analyze_pattern_part(pattern, &mut scope)?;
                if let Some(predicate) = predicate {
                    self.validate_expr_scope(predicate, &scope, "pattern comprehension predicate")?;
                }
                self.validate_expr_scope(map, &scope, "pattern comprehension projection")
            }
            Expr::ListComprehension {
                variable,
                collection,
                predicate,
                map,
            } => {
                self.validate_expr_refs(collection, candidates, "list comprehension collection")?;
                let mut locals = candidates.clone();
                locals.insert(variable.clone());
                if let Some(predicate) = predicate {
                    validate_iteration_aggregate(predicate)?;
                    self.validate_expr_refs(predicate, &locals, "list comprehension predicate")?;
                }
                validate_iteration_aggregate(map)?;
                self.validate_expr_refs(map, &locals, "list comprehension projection")
            }
            Expr::ListReduce {
                accumulator,
                variable,
                collection,
                map,
            } => {
                self.validate_expr_refs(collection, candidates, "list reduce collection")?;
                let mut locals = candidates.clone();
                locals.insert(accumulator.clone());
                locals.insert(variable.clone());
                validate_iteration_aggregate(map)?;
                self.validate_expr_refs(map, &locals, "list reduce projection")
            }
            Expr::ListTransform {
                variable,
                collection,
                map,
            } => {
                self.validate_expr_refs(collection, candidates, "list transform collection")?;
                let mut locals = candidates.clone();
                locals.insert(variable.clone());
                validate_iteration_aggregate(map)?;
                self.validate_expr_refs(map, &locals, "list transform projection")
            }
            Expr::ListFilter {
                variable,
                collection,
                predicate,
            } => {
                self.validate_expr_refs(collection, candidates, "list filter collection")?;
                let mut locals = candidates.clone();
                locals.insert(variable.clone());
                validate_iteration_aggregate(predicate)?;
                self.validate_expr_refs(predicate, &locals, "list filter predicate")
            }
            Expr::Quantifier {
                variable,
                collection,
                predicate,
                ..
            } => {
                self.validate_expr_refs(collection, candidates, "quantifier collection")?;
                let mut locals = candidates.clone();
                locals.insert(variable.clone());
                validate_iteration_aggregate(predicate)?;
                self.validate_expr_refs(predicate, &locals, "quantifier predicate")
            }
            Expr::Function { name, args, .. }
                if name.eq_ignore_ascii_case("regexp_replace") && args.len() == 4 =>
            {
                validate_regexp_replace_option(&args[3])
            }
            Expr::List(items) => validate_literal_list_types(items),
            _ => Ok(()),
        }
    }

    fn validate_exists(
        &mut self,
        exists: &ExistsSubquery,
        candidates: &BTreeSet<String>,
    ) -> CypherPlanResult<()> {
        let mut scope = scope_from_candidates(candidates);
        if let Some(query) = &exists.query {
            self.analyze_query_with_scope(query, &mut scope)?;
            return Ok(());
        }
        for part in &exists.patterns {
            self.analyze_pattern_part(part, &mut scope)?;
        }
        if let Some(predicate) = &exists.predicate {
            self.validate_expr_scope(predicate, &scope, "WHERE predicate")?;
        }
        Ok(())
    }

    fn synthetic(&mut self, prefix: &str) -> String {
        let id = self.synthetic_counter;
        self.synthetic_counter += 1;
        format!("__semantic_{prefix}_{id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::cypher::parser::parse_query;

    fn analyze_outputs(source: &str) -> CypherPlanResult<Vec<String>> {
        let query = parse_query(source).expect("parse query");
        analyze_query(&query).map(|analyzed| analyzed.output_fields)
    }

    fn analyze_error(source: &str) -> String {
        let query = parse_query(source).expect("parse query");
        analyze_query(&query)
            .expect_err("semantic analysis should fail")
            .to_string()
    }

    #[test]
    fn reports_visible_output_fields() {
        let output_fields = analyze_outputs("MATCH (person) RETURN person").expect("analyze");
        assert_eq!(output_fields, vec!["person".to_string()]);
    }

    #[test]
    fn rejects_projection_references_out_of_scope() {
        let err = analyze_error("MATCH (person) RETURN missing");
        assert!(err.contains("Binder exception: Variable missing is not in scope."));
    }

    #[test]
    fn rejects_unwind_rebinding_visible_name() {
        let err = analyze_error("MATCH (person) UNWIND [1] AS person RETURN person");
        assert!(err.contains("Binder exception: Variable person already exists."));
    }

    #[test]
    fn rejects_node_reused_as_relationship() {
        let err = analyze_error("MATCH (r) MATCH ()-[r]-() RETURN r");
        assert!(err.contains("Binder exception: r has data type NODE but REL was expected."));
    }

    #[test]
    fn rejects_relationship_reused_as_node() {
        let err = analyze_error("MATCH ()-[r]-() MATCH (r) RETURN r");
        assert!(err.contains("Binder exception: Cannot bind r as node pattern."));
    }

    #[test]
    fn rejects_value_alias_reused_as_node() {
        let err = analyze_error("WITH 123 AS n MATCH (n) RETURN n");
        assert!(err.contains("Binder exception: Cannot bind n as node pattern."));
    }

    #[test]
    fn same_pattern_node_reuse_takes_precedence_over_relationship() {
        let err = analyze_error("MATCH ()-[r]-(r) RETURN r");
        assert!(err.contains("Binder exception: r has data type NODE but REL was expected."));
    }

    #[test]
    fn rejects_repeated_relationship_name_in_one_match_clause() {
        let err = analyze_error("MATCH ()-[r]->(), ()-[r]->() RETURN r");
        assert!(err.contains(
            "Binder exception: Bind relationship r to relationship with same name is not supported."
        ));
    }

    #[test]
    fn rejects_unwind_of_non_list_literal() {
        let err = analyze_error("UNWIND 1 AS a RETURN a");
        assert!(err.contains("Binder exception: 1 has data type INT64 but LIST was expected."));
    }

    #[test]
    fn rejects_unwind_of_path_binding() {
        let err = analyze_error("MATCH p = ()-[*1..2]->() UNWIND p AS x RETURN x");
        assert!(
            err.contains("Binder exception: p has data type RECURSIVE_REL but LIST was expected.")
        );
    }

    #[test]
    fn rejects_rebinding_visible_path_variable() {
        let err = analyze_error("MATCH (p) MATCH p = ()-[]-() RETURN p");
        assert!(err.contains("SyntaxError: VariableAlreadyBound"));
    }

    #[test]
    fn accepts_union_branches_with_different_output_names_by_position() {
        let output_fields = analyze_outputs(
            "MATCH (p:person) RETURN p.age UNION ALL MATCH (p1:person) RETURN p1.age",
        )
        .expect("analyze");
        assert_eq!(output_fields, vec!["p.age".to_string()]);
    }

    #[test]
    fn rejects_union_arity_mismatch() {
        let err = analyze_error("RETURN 1 AS left, 2 AS extra UNION RETURN 1 AS right");
        assert!(err.contains(
            "Binder exception: The number of columns to union/union all must be the same."
        ));
    }

    #[test]
    fn rejects_union_property_type_mismatch() {
        let err = analyze_error(
            "MATCH (p:person) RETURN p.fName UNION ALL MATCH (p1:person) RETURN p1.age",
        );
        assert!(
            err.contains("Binder exception: p1.age has data type INT64 but STRING was expected.")
        );
    }

    #[test]
    fn rejects_mixed_union_and_union_all() {
        let err = analyze_error(
            "MATCH (p:person) RETURN p.age UNION ALL MATCH (p1:person) RETURN p1.age UNION MATCH (p2:person) RETURN p2.age",
        );
        assert!(err.contains("Binder exception: Union and union all can not be used together."));
    }

    #[test]
    fn accepts_with_order_by_without_skip_or_limit() {
        assert!(analyze_outputs("MATCH (a:person) WITH a.age AS k ORDER BY k RETURN k").is_ok());
    }

    #[test]
    fn rejects_order_by_node_and_complex_property_types() {
        let err = analyze_error("MATCH (a:person) RETURN a ORDER BY a");
        assert!(
            err.contains("Binder exception: Cannot order by a. Order by NODE is not supported.")
        );

        let err = analyze_error("MATCH (a:person) RETURN a ORDER BY a.workedHours");
        assert!(err.contains(
            "Binder exception: Cannot order by a.workedHours. Order by INT64[] is not supported."
        ));
    }

    #[test]
    fn rejects_known_invalid_arithmetic_type_pairs() {
        let err = analyze_error("MATCH (a:person) RETURN a.age + 'hh'");
        assert!(err.contains(
            "Binder exception: Cannot match a built-in function for given function +(INT64,STRING)."
        ));

        let err = analyze_error("MATCH (a:person) WHERE id(a) + 1 < id(a) RETURN a");
        assert!(err.contains("Binder exception: Function + did not receive correct arguments:"));
    }

    #[test]
    fn rejects_invalid_coalesce_static_calls() {
        let err = analyze_error("RETURN coalesce()");
        assert!(err.contains("Binder exception: COALESCE requires at least one argument"));

        let err = analyze_error("RETURN coalesce(1, 'hello')");
        assert!(err.contains(
            "Binder exception: Expression hello has data type STRING but expected INT64."
        ));
    }
}

/// Every variable a pattern part binds (node and relationship).
fn merge_pattern_variables(pattern: &PatternPart) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(variable) = &pattern.element.start.variable {
        out.push(variable.clone());
    }
    for chain in &pattern.element.chains {
        if let Some(variable) = &chain.relationship.variable {
            out.push(variable.clone());
        }
        if let Some(variable) = &chain.node.variable {
            out.push(variable.clone());
        }
    }
    out
}

/// Every inline property map a pattern part carries.
fn merge_pattern_properties(pattern: &PatternPart) -> Vec<&Expr> {
    let mut out = Vec::new();
    if let Some(properties) = &pattern.element.start.properties {
        out.push(properties);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            out.push(properties);
        }
        if let Some(properties) = &chain.node.properties {
            out.push(properties);
        }
    }
    out
}

/// Every expression a `SET` item evaluates.
fn merge_set_item_exprs(item: &crate::language::cypher::ast::SetItem) -> Vec<&Expr> {
    use crate::language::cypher::ast::SetItem as Item;
    match item {
        Item::Property { target, value, .. } => vec![target, value],
        Item::Replace { value, .. } | Item::Merge { value, .. } => vec![value],
        Item::Labels { .. } => Vec::new(),
    }
}
