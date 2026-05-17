//! Cypher semantic defaults and AST analysis used before Graph IR lowering.

use std::collections::{BTreeMap, BTreeSet};

use crate::language::cypher::ast::{
    BinaryOp, Clause, ExistsSubquery, Expr, Literal, PatternElement, PatternPart, ProjectionBody,
    Query, UnaryOp,
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
    ListInt,
    StructInt,
    StructListInt,
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
            BindingKind::ListInt => "INT64[]",
            BindingKind::StructInt => "STRUCT(x INT64)",
            BindingKind::StructListInt => "STRUCT(x INT64[])",
            BindingKind::Value => "ANY",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzedQuery<'a> {
    pub query: &'a Query,
    pub output_fields: Vec<String>,
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

    fn fields(&self) -> Vec<String> {
        self.bindings.keys().cloned().collect()
    }

    fn field_set(&self) -> BTreeSet<String> {
        self.bindings.keys().cloned().collect()
    }

    fn replace(&mut self, outputs: Vec<(String, BindingKind)>) {
        self.bindings = outputs.into_iter().collect();
    }
}

pub fn analyze_query(query: &Query) -> CypherPlanResult<AnalyzedQuery<'_>> {
    let mut analyzer = SemanticAnalyzer::default();
    let mut scope = SemanticScope::default();
    let output_fields = analyzer.analyze_query_with_scope(query, &mut scope)?;
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
    ) -> CypherPlanResult<Vec<String>> {
        let initial_scope = scope.clone();
        let root_outputs = self.analyze_query_body(query, scope)?;
        for branch in &query.unions {
            let mut branch_scope = initial_scope.clone();
            let branch_outputs = self.analyze_query_with_scope(&branch.query, &mut branch_scope)?;
            if branch_outputs != root_outputs {
                return Err(CypherPlanError::Invalid(format!(
                    "UNION branches must project the same columns: left [{}], right [{}]",
                    root_outputs.join(", "),
                    branch_outputs.join(", ")
                )));
            }
        }
        Ok(root_outputs)
    }

    fn analyze_query_body(
        &mut self,
        query: &Query,
        scope: &mut SemanticScope,
    ) -> CypherPlanResult<Vec<String>> {
        let mut result_fields = None;
        for clause in &query.clauses {
            match clause {
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
                    self.validate_expr_scope(&clause.expr, scope, "UNWIND expression")?;
                    validate_list_source(&clause.expr, scope)?;
                    if scope.contains(&clause.alias) {
                        return Err(CypherPlanError::Invalid(format!(
                            "UNWIND alias `{}` is already in scope",
                            clause.alias
                        )));
                    }
                    scope.insert(clause.alias.clone(), BindingKind::Value);
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
                        if let Some(properties) = &part.element.start.properties {
                            self.validate_expr_scope(properties, scope, "CREATE properties")?;
                        }
                        if let Some(variable) = &part.element.start.variable {
                            if scope.contains(variable) {
                                return Err(CypherPlanError::Invalid(format!(
                                    "CREATE variable `{variable}` is already in scope"
                                )));
                            }
                            scope.insert(variable.clone(), BindingKind::Node);
                        }
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
                                        "CREATE variable `{variable}` is already in scope"
                                    )));
                                }
                                scope.insert(variable.clone(), BindingKind::Relationship);
                            }
                            if let Some(properties) = &chain.node.properties {
                                self.validate_expr_scope(properties, scope, "CREATE properties")?;
                            }
                            if let Some(variable) = &chain.node.variable {
                                if scope.contains(variable) {
                                    return Err(CypherPlanError::Invalid(format!(
                                        "CREATE variable `{variable}` is already in scope"
                                    )));
                                }
                                scope.insert(variable.clone(), BindingKind::Node);
                            }
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
                                    )));
                                }
                                self.validate_expr_scope(value, scope, "SET value")?;
                            }
                            crate::language::cypher::ast::SetItem::Labels { variable, .. } => {
                                if !scope.contains(variable) {
                                    return Err(CypherPlanError::Invalid(format!(
                                        "SET references variables that are not in scope: {variable}"
                                    )));
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
                    let output_fields = outputs
                        .iter()
                        .map(|(field, _)| field.clone())
                        .collect::<Vec<_>>();
                    scope.replace(outputs);
                    result_fields = Some(output_fields);
                }
                Clause::Return(clause) => {
                    let outputs = self.analyze_projection_body(&clause.projection, scope)?;
                    result_fields = Some(outputs.into_iter().map(|(field, _)| field).collect());
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
    ) -> CypherPlanResult<Vec<(String, BindingKind)>> {
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
                .map(|(field, _)| field.clone())
                .collect::<Vec<_>>(),
            "projection contains duplicate column names",
        )?;

        let mut order_candidates = scope.field_set();
        order_candidates.extend(output_fields.iter().map(|(field, _)| field.clone()));
        for item in &body.order_by {
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
        outputs: &[(String, BindingKind)],
    ) -> CypherPlanResult<()> {
        let source_fields = source_scope.field_set();
        let projected_fields = outputs
            .iter()
            .map(|(field, _)| field.clone())
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
    ) -> Vec<(String, BindingKind)> {
        let mut outputs = Vec::new();
        if body.include_existing {
            outputs.extend(
                scope
                    .bindings
                    .iter()
                    .map(|(binding, kind)| (binding.clone(), *kind)),
            );
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
                _ => projected_value_kind(&item.expr),
            };
            outputs.push((name, kind));
        }
        outputs
    }

    fn validate_expr_scope(
        &mut self,
        expr: &Expr,
        scope: &SemanticScope,
        clause: &str,
    ) -> CypherPlanResult<()> {
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
        } else {
            Err(CypherPlanError::Invalid(format!(
                "{clause} references variables that are not in scope: {}",
                missing.join(", ")
            )))
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
                    self.validate_expr_refs(predicate, &locals, "list comprehension predicate")?;
                }
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

fn validate_static_expression_types(expr: &Expr) -> CypherPlanResult<()> {
    match expr {
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => {
            validate_static_expression_types(expr)?;
            validate_bool_operand(expr)
        }
        Expr::Unary { expr, .. } => validate_static_expression_types(expr),
        Expr::Binary { op, lhs, rhs } if matches!(op, BinaryOp::And | BinaryOp::Or) => {
            validate_static_expression_types(lhs)?;
            validate_static_expression_types(rhs)?;
            validate_bool_operand(lhs)?;
            validate_bool_operand(rhs)
        }
        Expr::Binary { lhs, rhs, .. } => {
            validate_static_expression_types(lhs)?;
            validate_static_expression_types(rhs)
        }
        Expr::Function { name, args, .. } if name.eq_ignore_ascii_case("xor") => {
            for arg in args {
                validate_static_expression_types(arg)?;
                validate_bool_operand(arg)?;
            }
            Ok(())
        }
        Expr::Function { args, .. } => {
            for arg in args {
                validate_static_expression_types(arg)?;
            }
            Ok(())
        }
        Expr::Property { target, .. } | Expr::IsNull(target) | Expr::IsNotNull(target) => {
            validate_static_expression_types(target)
        }
        Expr::LabelPredicate { target, .. } => validate_static_expression_types(target),
        Expr::StringPredicate {
            target, pattern, ..
        } => {
            validate_static_expression_types(target)?;
            validate_static_expression_types(pattern)
        }
        Expr::List(items) => {
            for item in items {
                validate_static_expression_types(item)?;
            }
            Ok(())
        }
        Expr::Map(items) => {
            for (_, value) in items {
                validate_static_expression_types(value)?;
            }
            Ok(())
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                validate_static_expression_types(case)?;
            }
            for (when, then) in arms {
                validate_static_expression_types(when)?;
                validate_static_expression_types(then)?;
            }
            if let Some(otherwise) = otherwise {
                validate_static_expression_types(otherwise)?;
            }
            Ok(())
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            validate_static_expression_types(collection)?;
            if let Some(predicate) = predicate {
                validate_static_expression_types(predicate)?;
            }
            validate_static_expression_types(map)
        }
        Expr::ListReduce {
            collection, map, ..
        }
        | Expr::ListTransform {
            collection, map, ..
        } => {
            validate_static_expression_types(collection)?;
            validate_static_expression_types(map)
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        }
        | Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            validate_static_expression_types(collection)?;
            validate_static_expression_types(predicate)
        }
        Expr::PatternComprehension { predicate, map, .. } => {
            if let Some(predicate) = predicate {
                validate_static_expression_types(predicate)?;
            }
            validate_static_expression_types(map)
        }
        Expr::Exists(exists) => {
            if let Some(predicate) = &exists.predicate {
                validate_static_expression_types(predicate)?;
            }
            if let Some(query) = &exists.query {
                for clause in &query.clauses {
                    validate_clause_static_expression_types(clause)?;
                }
            }
            Ok(())
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::PatternPredicate(_)
        | Expr::CountStar => Ok(()),
    }
}

fn validate_clause_static_expression_types(clause: &Clause) -> CypherPlanResult<()> {
    match clause {
        Clause::Match(clause) => {
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        Clause::Unwind(clause) => validate_static_expression_types(&clause.expr),
        Clause::Call(clause) => {
            for arg in &clause.args {
                validate_static_expression_types(arg)?;
            }
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        Clause::Create(clause) => {
            for part in &clause.patterns {
                if let Some(properties) = &part.element.start.properties {
                    validate_static_expression_types(properties)?;
                }
                for chain in &part.element.chains {
                    if let Some(properties) = &chain.relationship.properties {
                        validate_static_expression_types(properties)?;
                    }
                    if let Some(properties) = &chain.node.properties {
                        validate_static_expression_types(properties)?;
                    }
                }
            }
            Ok(())
        }
        Clause::Set(clause) => {
            for item in &clause.items {
                match item {
                    crate::language::cypher::ast::SetItem::Property { target, value, .. } => {
                        validate_static_expression_types(target)?;
                        validate_static_expression_types(value)?;
                    }
                    crate::language::cypher::ast::SetItem::Replace { value, .. }
                    | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                        validate_static_expression_types(value)?;
                    }
                    crate::language::cypher::ast::SetItem::Labels { .. } => {}
                }
            }
            Ok(())
        }
        Clause::Delete(clause) => {
            for expr in &clause.expressions {
                validate_static_expression_types(expr)?;
            }
            Ok(())
        }
        Clause::With(clause) => {
            validate_projection_static_expression_types(&clause.projection)?;
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        Clause::Return(clause) => validate_projection_static_expression_types(&clause.projection),
    }
}

fn validate_projection_static_expression_types(body: &ProjectionBody) -> CypherPlanResult<()> {
    for item in &body.items {
        validate_static_expression_types(&item.expr)?;
    }
    for sort in &body.order_by {
        validate_static_expression_types(&sort.expr)?;
    }
    if let Some(skip) = &body.skip {
        validate_static_expression_types(skip)?;
    }
    if let Some(limit) = &body.limit {
        validate_static_expression_types(limit)?;
    }
    Ok(())
}

fn validate_bool_operand(expr: &Expr) -> CypherPlanResult<()> {
    match static_expr_type_name(expr)? {
        Some(type_name) if type_name != "BOOL" => Err(CypherPlanError::Invalid(format!(
            "Binder exception: Expression {} has data type {type_name} but expected BOOL. Implicit cast is not supported.",
            display_literal_expr(expr)
        ))),
        _ => Ok(()),
    }
}

fn static_expr_type_name(expr: &Expr) -> CypherPlanResult<Option<String>> {
    match expr {
        Expr::Literal(Literal::Null) => Ok(None),
        Expr::Literal(Literal::Bool(_)) => Ok(Some("BOOL".to_string())),
        Expr::Literal(Literal::Integer(_)) => Ok(Some("INT64".to_string())),
        Expr::Literal(Literal::Float(_)) => Ok(Some("DOUBLE".to_string())),
        Expr::Literal(Literal::String(_)) => Ok(Some("STRING".to_string())),
        Expr::List(items) if items.is_empty() => Ok(Some("INT64[]".to_string())),
        Expr::List(items) => Ok(infer_literal_list_type(items)?
            .map(|inner| format!("{}[]", inner.cypher_name()))
            .or(Some("INT64[]".to_string()))),
        Expr::Map(items) => Ok(Some(static_map_type_name(items)?)),
        _ => Ok(None),
    }
}

fn static_map_type_name(items: &[(String, Expr)]) -> CypherPlanResult<String> {
    let fields = items
        .iter()
        .map(|(key, value)| {
            Ok(format!(
                "{key} {}",
                static_expr_type_name(value)?.unwrap_or_else(|| "ANY".to_string())
            ))
        })
        .collect::<CypherPlanResult<Vec<_>>>()?;
    Ok(format!("STRUCT({})", fields.join(", ")))
}

fn validate_regexp_replace_option(option: &Expr) -> CypherPlanResult<()> {
    match option {
        Expr::Literal(Literal::String(flag)) if flag == "g" => Ok(()),
        Expr::Literal(Literal::String(_)) => Err(CypherPlanError::Invalid(
            "Binder exception: regex_replace can only support global replace option: g."
                .to_string(),
        )),
        Expr::Literal(Literal::Integer(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type INT64 but STRING was expected."
        ))),
        Expr::Literal(Literal::Float(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type DOUBLE but STRING was expected."
        ))),
        Expr::Literal(Literal::Bool(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type BOOL but STRING was expected."
        ))),
        other => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {} has type PROPERTY but LITERAL was expected.",
            display_property_expr(other)
        ))),
    }
}

fn display_property_expr(expr: &Expr) -> String {
    match expr {
        Expr::Variable(name) => name.clone(),
        Expr::Property { target, key } => format!("{}.{}", display_property_expr(target), key),
        _ => display_literal_expr(expr),
    }
}

fn validate_literal_list_types(items: &[Expr]) -> CypherPlanResult<()> {
    let _ = infer_literal_list_type(items)?;
    Ok(())
}

fn infer_literal_list_type(items: &[Expr]) -> CypherPlanResult<Option<LiteralListType>> {
    let mut expected = preferred_literal_list_type(items)?;
    for item in items {
        let Some(actual) = literal_list_type(item)? else {
            continue;
        };
        let Some(expected_type) = expected.as_ref() else {
            expected = Some(actual.clone());
            continue;
        };
        if expected_type.compatible_with(&actual) {
            if matches!(expected_type, LiteralListType::EmptyList)
                && matches!(actual, LiteralListType::List(_))
            {
                expected = Some(actual);
            }
            continue;
        }
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: Expression {} has data type {} but expected {}. Implicit cast is not supported.",
            display_literal_expr(item),
            actual.cypher_name(),
            expected_type.cypher_name()
        )));
    }
    Ok(expected)
}

fn preferred_literal_list_type(items: &[Expr]) -> CypherPlanResult<Option<LiteralListType>> {
    let mut first = None;
    let mut first_numeric = None;
    let mut first_list = None;
    for item in items {
        let Some(actual) = literal_list_type(item)? else {
            continue;
        };
        if first.is_none() {
            first = Some(actual.clone());
        }
        if first_numeric.is_none()
            && matches!(actual, LiteralListType::Int | LiteralListType::Float)
        {
            first_numeric = Some(actual.clone());
        }
        if matches!(
            actual,
            LiteralListType::List(_) | LiteralListType::EmptyList
        ) {
            first_list = Some(actual);
            break;
        }
    }
    Ok(first_list.or(first_numeric).or(first))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiteralListType {
    Bool,
    Int,
    Float,
    String,
    Map,
    EmptyList,
    List(Box<LiteralListType>),
}

impl LiteralListType {
    fn cypher_name(&self) -> String {
        match self {
            LiteralListType::Bool => "BOOL".to_string(),
            LiteralListType::Int => "INT64".to_string(),
            LiteralListType::Float => "DOUBLE".to_string(),
            LiteralListType::String => "STRING".to_string(),
            LiteralListType::Map => "STRUCT".to_string(),
            LiteralListType::EmptyList => "INT64[]".to_string(),
            LiteralListType::List(inner) => format!("{}[]", inner.cypher_name()),
        }
    }

    fn compatible_with(&self, other: &Self) -> bool {
        self == other || self.numeric_compatible(other) || self.list_compatible(other)
    }

    fn numeric_compatible(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (LiteralListType::Int, LiteralListType::Float)
                | (LiteralListType::Float, LiteralListType::Int)
        )
    }

    fn list_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (LiteralListType::EmptyList, LiteralListType::List(_))
            | (LiteralListType::List(_), LiteralListType::EmptyList) => true,
            (LiteralListType::List(left), LiteralListType::List(right)) => {
                left.compatible_with(right)
            }
            _ => false,
        }
    }
}

fn literal_list_type(expr: &Expr) -> CypherPlanResult<Option<LiteralListType>> {
    match expr {
        Expr::Literal(Literal::Null) => Ok(None),
        Expr::Literal(Literal::Bool(_)) => Ok(Some(LiteralListType::Bool)),
        Expr::Literal(Literal::Integer(_)) => Ok(Some(LiteralListType::Int)),
        Expr::Literal(Literal::Float(_)) => Ok(Some(LiteralListType::Float)),
        Expr::Literal(Literal::String(_)) => Ok(Some(LiteralListType::String)),
        Expr::Map(_) => Ok(Some(LiteralListType::Map)),
        Expr::List(items) if items.is_empty() => Ok(Some(LiteralListType::EmptyList)),
        Expr::List(items) => Ok(infer_literal_list_type(items)?
            .map(|inner| LiteralListType::List(Box::new(inner)))
            .or(Some(LiteralListType::EmptyList))),
        _ => Ok(None),
    }
}

fn display_literal_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Null) => "null".to_string(),
        Expr::Literal(Literal::Bool(true)) => "True".to_string(),
        Expr::Literal(Literal::Bool(false)) => "False".to_string(),
        Expr::Literal(Literal::Integer(value)) => value.clone(),
        Expr::Literal(Literal::Float(value)) => format!("{value:.6}"),
        Expr::Literal(Literal::String(value)) => value.clone(),
        Expr::Map(items) => {
            let values = items
                .iter()
                .map(|(_, value)| display_literal_expr(value))
                .collect::<Vec<_>>();
            format!("STRUCT_PACK({})", values.join(", "))
        }
        Expr::List(items) => {
            let values = items
                .iter()
                .filter(|item| !matches!(item, Expr::Literal(Literal::Null)))
                .map(display_literal_expr)
                .collect::<Vec<_>>();
            format!("LIST_CREATION({})", values.join(", "))
        }
        _ => "<expression>".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcedureMode {
    Read,
    Write,
}

fn procedure_mode(name: &str) -> ProcedureMode {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" | "db.relationshiptypes" | "db.propertykeys" => ProcedureMode::Read,
        _ => ProcedureMode::Write,
    }
}

fn procedure_yields(
    clause: &crate::language::cypher::ast::ProcedureCallClause,
) -> (Vec<String>, Vec<String>) {
    if !clause.yields.is_empty() {
        return (
            clause
                .yields
                .iter()
                .map(|item| item.field.clone())
                .collect(),
            clause
                .yields
                .iter()
                .map(|item| item.alias.clone())
                .collect(),
        );
    }
    if clause.yield_all || clause.standalone {
        let yields = default_procedure_yields(&clause.name);
        return (yields.clone(), yields);
    }
    (Vec::new(), Vec::new())
}

fn default_procedure_yields(name: &str) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" => vec!["label".to_string()],
        "db.relationshiptypes" => vec!["relationshipType".to_string()],
        "db.propertykeys" => vec!["propertyKey".to_string()],
        _ => vec!["value".to_string()],
    }
}

fn validate_unique(fields: &[String], message: &str) -> CypherPlanResult<()> {
    let mut seen = BTreeSet::new();
    let duplicates = fields
        .iter()
        .filter(|field| !seen.insert((*field).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "{message}: {}",
            duplicates.join(", ")
        )))
    }
}

fn validate_with_projection_aliases(body: &ProjectionBody) -> CypherPlanResult<()> {
    let missing = body
        .items
        .iter()
        .filter(|item| !item.explicit_alias && !matches!(item.expr, Expr::Variable(_)))
        .map(|item| {
            item.alias
                .clone()
                .unwrap_or_else(|| "<expression>".to_string())
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "non-variable expressions in WITH must be aliased with AS: {}",
            missing.join(", ")
        )))
    }
}

fn validate_path_binding(part: &PatternPart, scope: &SemanticScope) -> CypherPlanResult<()> {
    let Some(path) = &part.variable else {
        return Ok(());
    };
    if scope.contains(path) || pattern_element_declares(&part.element, path) {
        return Err(CypherPlanError::Invalid(
            "SyntaxError: VariableAlreadyBound".to_string(),
        ));
    }
    Ok(())
}

fn validate_relationship_binding(
    binding: &str,
    expected: BindingKind,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if kind != expected && kind != BindingKind::Unknown => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: {binding} has data type {} but {} was expected.",
                kind.cypher_type_name(),
                expected.cypher_type_name()
            )))
        }
        _ => Ok(()),
    }
}

fn validate_node_binding(
    binding: &str,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if !matches!(kind, BindingKind::Unknown | BindingKind::Node) => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: Cannot bind {binding} as node pattern."
            )))
        }
        _ => Ok(()),
    }
}

fn validate_pattern_predicate_scope(
    patterns: &[PatternPart],
    candidates: &BTreeSet<String>,
) -> CypherPlanResult<()> {
    let mut named = BTreeSet::new();
    for part in patterns {
        named.extend(pattern_binding_names(part));
    }
    let introduced = named
        .into_iter()
        .filter(|name| !candidates.contains(name))
        .collect::<Vec<_>>();
    if introduced.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "pattern predicates may not introduce new variables: {}",
            introduced.join(", ")
        )))
    }
}

fn pattern_element_declares(element: &PatternElement, binding: &str) -> bool {
    element.start.variable.as_deref() == Some(binding)
        || element.chains.iter().any(|chain| {
            chain.node.variable.as_deref() == Some(binding)
                || chain.relationship.variable.as_deref() == Some(binding)
        })
}

fn pattern_binding_names(pattern: &PatternPart) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(variable) = &pattern.variable {
        names.insert(variable.clone());
    }
    if let Some(variable) = &pattern.element.start.variable {
        names.insert(variable.clone());
    }
    for chain in &pattern.element.chains {
        if let Some(variable) = &chain.relationship.variable {
            names.insert(variable.clone());
        }
        if let Some(variable) = &chain.node.variable {
            names.insert(variable.clone());
        }
    }
    names
}

fn is_variable_length(range: &crate::language::cypher::ast::RangeLiteral) -> bool {
    range.min != 1 || range.max != Some(1)
}

fn projected_value_kind(expr: &Expr) -> BindingKind {
    match expr {
        Expr::Variable(_) => BindingKind::Unknown,
        Expr::Literal(Literal::Bool(_)) => BindingKind::Bool,
        Expr::Literal(Literal::Integer(_)) => BindingKind::Int,
        Expr::Literal(Literal::Float(_)) => BindingKind::Float,
        Expr::Literal(Literal::String(_)) => BindingKind::String,
        Expr::List(items)
            if items
                .iter()
                .all(|item| matches!(item, Expr::Literal(Literal::Integer(_)))) =>
        {
            BindingKind::ListInt
        }
        Expr::Map(items) if items.len() == 1 && items[0].0 == "x" => match &items[0].1 {
            Expr::Literal(Literal::Integer(_)) => BindingKind::StructInt,
            Expr::List(values)
                if values
                    .iter()
                    .all(|item| matches!(item, Expr::Literal(Literal::Integer(_)))) =>
            {
                BindingKind::StructListInt
            }
            _ => BindingKind::Value,
        },
        _ => BindingKind::Value,
    }
}

fn validate_list_source(expr: &Expr, scope: &SemanticScope) -> CypherPlanResult<()> {
    if let Expr::Variable(name) = expr {
        if let Some(
            kind @ (BindingKind::Node
            | BindingKind::Relationship
            | BindingKind::RecursiveRelationship),
        ) = scope.kind(name)
        {
            return Err(CypherPlanError::Invalid(format!(
                "Binder exception: {name} has data type {} but LIST was expected.",
                kind.cypher_type_name()
            )));
        }
    }
    if let Some(actual) = static_non_list_type_name(expr) {
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: {} has data type {actual} but LIST was expected.",
            display_literal_expr(expr)
        )));
    }
    Ok(())
}

fn static_non_list_type_name(expr: &Expr) -> Option<&'static str> {
    match expr {
        Expr::Literal(Literal::Bool(_)) => Some("BOOL"),
        Expr::Literal(Literal::Integer(_)) => Some("INT64"),
        Expr::Literal(Literal::Float(_)) => Some("DOUBLE"),
        Expr::Literal(Literal::String(_)) => Some("STRING"),
        Expr::Map(_) => Some("STRUCT"),
        Expr::Literal(Literal::Null) | Expr::List(_) => None,
        _ => None,
    }
}

fn scope_from_candidates(candidates: &BTreeSet<String>) -> SemanticScope {
    SemanticScope {
        bindings: candidates
            .iter()
            .map(|binding| (binding.clone(), BindingKind::Unknown))
            .collect(),
    }
}

fn collect_free_variables(expr: &Expr, bound: &mut BTreeSet<String>, out: &mut BTreeSet<String>) {
    match expr {
        Expr::Variable(name) => {
            if !bound.contains(name) {
                out.insert(name.clone());
            }
        }
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            collect_free_variables(target, bound, out);
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            collect_free_variables(expr, bound, out);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            collect_free_variables(lhs, bound, out);
            collect_free_variables(rhs, bound, out);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                collect_free_variables(arg, bound, out);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                collect_free_variables(value, bound, out);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                collect_free_variables(case, bound, out);
            }
            for (when, then) in arms {
                collect_free_variables(when, bound, out);
                collect_free_variables(then, bound, out);
            }
            if let Some(otherwise) = otherwise {
                collect_free_variables(otherwise, bound, out);
            }
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                if let Some(predicate) = predicate {
                    collect_free_variables(predicate, bound, out);
                }
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound_many(bound, [accumulator, variable], |bound| {
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(map, bound, out);
            });
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(predicate, bound, out);
            });
        }
        Expr::Quantifier {
            variable,
            collection,
            predicate,
            ..
        } => {
            collect_free_variables(collection, bound, out);
            with_bound(bound, variable, |bound| {
                collect_free_variables(predicate, bound, out);
            });
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut names = pattern_binding_names(pattern);
            if let Some(variable) = variable {
                names.insert(variable.clone());
            }
            let inserted = insert_bound_many(bound, names.iter());
            collect_pattern_property_variables(pattern, bound, out);
            if let Some(predicate) = predicate {
                collect_free_variables(predicate, bound, out);
            }
            collect_free_variables(map, bound, out);
            remove_inserted(bound, inserted);
        }
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                collect_query_references(query, bound, out);
            }
            for part in &exists.patterns {
                collect_pattern_property_variables(part, bound, out);
            }
            if let Some(predicate) = &exists.predicate {
                collect_free_variables(predicate, bound, out);
            }
        }
        Expr::PatternPredicate(patterns) => {
            for part in patterns {
                out.extend(pattern_binding_names(part));
                collect_pattern_property_variables(part, bound, out);
            }
        }
        Expr::Star | Expr::Parameter(_) | Expr::Literal(_) | Expr::CountStar => {}
    }
}

fn collect_pattern_property_variables(
    pattern: &PatternPart,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    if let Some(properties) = &pattern.element.start.properties {
        collect_free_variables(properties, bound, out);
    }
    for chain in &pattern.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            collect_free_variables(properties, bound, out);
        }
        if let Some(properties) = &chain.node.properties {
            collect_free_variables(properties, bound, out);
        }
    }
}

fn collect_query_references(
    query: &Query,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    let mut query_bound = bound.clone();
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    collect_pattern_property_variables(part, &mut query_bound, out);
                    query_bound.extend(pattern_binding_names(part));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Unwind(clause) => {
                collect_free_variables(&clause.expr, &mut query_bound, out);
                query_bound.insert(clause.alias.clone());
            }
            Clause::Call(clause) => {
                for arg in &clause.args {
                    collect_free_variables(arg, &mut query_bound, out);
                }
                for item in &clause.yields {
                    query_bound.insert(item.alias.clone());
                }
                if clause.yield_all || clause.standalone {
                    query_bound.extend(default_procedure_yields(&clause.name));
                }
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(properties) = &part.element.start.properties {
                        collect_free_variables(properties, &mut query_bound, out);
                    }
                    if let Some(variable) = &part.element.start.variable {
                        query_bound.insert(variable.clone());
                    }
                    for chain in &part.element.chains {
                        if let Some(properties) = &chain.relationship.properties {
                            collect_free_variables(properties, &mut query_bound, out);
                        }
                        if let Some(variable) = &chain.relationship.variable {
                            query_bound.insert(variable.clone());
                        }
                        if let Some(properties) = &chain.node.properties {
                            collect_free_variables(properties, &mut query_bound, out);
                        }
                        if let Some(variable) = &chain.node.variable {
                            query_bound.insert(variable.clone());
                        }
                    }
                }
            }
            Clause::Set(clause) => {
                for item in &clause.items {
                    match item {
                        crate::language::cypher::ast::SetItem::Property {
                            target, value, ..
                        } => {
                            collect_free_variables(target, &mut query_bound, out);
                            collect_free_variables(value, &mut query_bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Replace { variable, value }
                        | crate::language::cypher::ast::SetItem::Merge { variable, value } => {
                            if !query_bound.contains(variable) {
                                out.insert(variable.clone());
                            }
                            collect_free_variables(value, &mut query_bound, out);
                        }
                        crate::language::cypher::ast::SetItem::Labels { variable, .. } => {
                            if !query_bound.contains(variable) {
                                out.insert(variable.clone());
                            }
                        }
                    }
                }
            }
            Clause::Delete(clause) => {
                for expr in &clause.expressions {
                    collect_free_variables(expr, &mut query_bound, out);
                }
            }
            Clause::With(clause) => {
                collect_projection_references(&clause.projection, &mut query_bound, out);
                query_bound = projection_output_names(&clause.projection, &query_bound);
                if let Some(predicate) = &clause.predicate {
                    collect_free_variables(predicate, &mut query_bound, out);
                }
            }
            Clause::Return(clause) => {
                collect_projection_references(&clause.projection, &mut query_bound, out);
            }
        }
    }
}

fn collect_projection_references(
    body: &ProjectionBody,
    bound: &mut BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    for item in &body.items {
        collect_free_variables(&item.expr, bound, out);
    }
    let mut projection_bound = bound.clone();
    projection_bound.extend(projection_output_names(body, bound));
    for item in &body.order_by {
        collect_free_variables(&item.expr, &mut projection_bound, out);
    }
    if let Some(skip) = &body.skip {
        collect_free_variables(skip, &mut projection_bound, out);
    }
    if let Some(limit) = &body.limit {
        collect_free_variables(limit, &mut projection_bound, out);
    }
}

fn projection_output_names(body: &ProjectionBody, visible: &BTreeSet<String>) -> BTreeSet<String> {
    let mut outputs = if body.include_existing {
        visible.clone()
    } else {
        BTreeSet::new()
    };
    for item in &body.items {
        if let Some(alias) = item
            .alias
            .clone()
            .or_else(|| item.expr.variable_name().map(ToString::to_string))
        {
            outputs.insert(alias);
        }
    }
    outputs
}

fn remove_local_exists_bindings(expr: &Expr, refs: &mut BTreeSet<String>) {
    match expr {
        Expr::Exists(exists) => {
            if let Some(query) = &exists.query {
                remove_query_outputs(query, refs);
            }
            for part in &exists.patterns {
                for name in pattern_binding_names(part) {
                    refs.remove(&name);
                }
            }
        }
        Expr::PatternComprehension { pattern, .. } => {
            for name in pattern_binding_names(pattern) {
                refs.remove(&name);
            }
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            remove_local_exists_bindings(expr, refs);
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => {
            remove_local_exists_bindings(lhs, refs);
            remove_local_exists_bindings(rhs, refs);
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            for arg in args {
                remove_local_exists_bindings(arg, refs);
            }
        }
        Expr::Map(items) => {
            for (_, value) in items {
                remove_local_exists_bindings(value, refs);
            }
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                remove_local_exists_bindings(case, refs);
            }
            for (when, then) in arms {
                remove_local_exists_bindings(when, refs);
                remove_local_exists_bindings(then, refs);
            }
            if let Some(otherwise) = otherwise {
                remove_local_exists_bindings(otherwise, refs);
            }
        }
        _ => {}
    }
}

fn remove_query_outputs(query: &Query, refs: &mut BTreeSet<String>) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(clause) => {
                for part in &clause.patterns {
                    for name in pattern_binding_names(part) {
                        refs.remove(&name);
                    }
                }
            }
            Clause::Unwind(clause) => {
                refs.remove(&clause.alias);
            }
            Clause::Call(clause) => {
                for item in &clause.yields {
                    refs.remove(&item.alias);
                }
                if clause.yield_all || clause.standalone {
                    for name in default_procedure_yields(&clause.name) {
                        refs.remove(&name);
                    }
                }
            }
            Clause::Create(clause) => {
                for part in &clause.patterns {
                    if let Some(variable) = &part.element.start.variable {
                        refs.remove(variable);
                    }
                    for chain in &part.element.chains {
                        if let Some(variable) = &chain.relationship.variable {
                            refs.remove(variable);
                        }
                        if let Some(variable) = &chain.node.variable {
                            refs.remove(variable);
                        }
                    }
                }
            }
            Clause::Set(_) | Clause::Delete(_) => {}
            Clause::With(clause) => {
                for name in projection_output_names(&clause.projection, &BTreeSet::new()) {
                    refs.remove(&name);
                }
            }
            Clause::Return(clause) => {
                for name in projection_output_names(&clause.projection, &BTreeSet::new()) {
                    refs.remove(&name);
                }
            }
        }
    }
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::CountStar => true,
        Expr::Function { name, args, .. } => {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "count" | "sum" | "avg" | "min" | "max" | "collect"
            ) || args.iter().any(contains_aggregate)
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            contains_aggregate(expr)
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => contains_aggregate(lhs) || contains_aggregate(rhs),
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            contains_aggregate(target)
        }
        Expr::List(items) => items.iter().any(contains_aggregate),
        Expr::Map(items) => items.iter().any(|(_, value)| contains_aggregate(value)),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            case.as_deref().is_some_and(contains_aggregate)
                || arms
                    .iter()
                    .any(|(when, then)| contains_aggregate(when) || contains_aggregate(then))
                || otherwise.as_deref().is_some_and(contains_aggregate)
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            contains_aggregate(collection)
                || predicate.as_deref().is_some_and(contains_aggregate)
                || contains_aggregate(map)
        }
        Expr::ListReduce {
            collection, map, ..
        }
        | Expr::ListTransform {
            collection, map, ..
        } => contains_aggregate(collection) || contains_aggregate(map),
        Expr::ListFilter {
            collection,
            predicate,
            ..
        }
        | Expr::Quantifier {
            collection,
            predicate,
            ..
        } => contains_aggregate(collection) || contains_aggregate(predicate),
        Expr::PatternComprehension { predicate, map, .. } => {
            predicate.as_deref().is_some_and(contains_aggregate) || contains_aggregate(map)
        }
        Expr::Exists(exists) => {
            exists.predicate.as_deref().is_some_and(contains_aggregate)
                || exists
                    .query
                    .as_deref()
                    .is_some_and(query_contains_aggregate)
        }
        Expr::PatternPredicate(_) => false,
        Expr::Star | Expr::Variable(_) | Expr::Parameter(_) | Expr::Literal(_) => false,
    }
}

fn query_contains_aggregate(query: &Query) -> bool {
    query.clauses.iter().any(|clause| match clause {
        Clause::With(clause) => projection_contains_aggregate(&clause.projection),
        Clause::Return(clause) => projection_contains_aggregate(&clause.projection),
        Clause::Match(clause) => clause.predicate.as_ref().is_some_and(contains_aggregate),
        Clause::Unwind(clause) => contains_aggregate(&clause.expr),
        Clause::Call(clause) => {
            clause.args.iter().any(contains_aggregate)
                || clause.predicate.as_ref().is_some_and(contains_aggregate)
        }
        Clause::Create(clause) => clause.patterns.iter().any(|part| {
            part.element
                .start
                .properties
                .as_ref()
                .is_some_and(contains_aggregate)
                || part.element.chains.iter().any(|chain| {
                    chain
                        .relationship
                        .properties
                        .as_ref()
                        .is_some_and(contains_aggregate)
                        || chain
                            .node
                            .properties
                            .as_ref()
                            .is_some_and(contains_aggregate)
                })
        }),
        Clause::Set(clause) => clause.items.iter().any(|item| match item {
            crate::language::cypher::ast::SetItem::Property { target, value, .. } => {
                contains_aggregate(target) || contains_aggregate(value)
            }
            crate::language::cypher::ast::SetItem::Replace { value, .. }
            | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                contains_aggregate(value)
            }
            crate::language::cypher::ast::SetItem::Labels { .. } => false,
        }),
        Clause::Delete(clause) => clause.expressions.iter().any(contains_aggregate),
    })
}

fn projection_contains_aggregate(body: &ProjectionBody) -> bool {
    body.items.iter().any(|item| contains_aggregate(&item.expr))
        || body
            .order_by
            .iter()
            .any(|item| contains_aggregate(&item.expr))
}

fn with_bound<F>(bound: &mut BTreeSet<String>, name: &str, f: F)
where
    F: FnOnce(&mut BTreeSet<String>),
{
    let inserted = bound.insert(name.to_string());
    f(bound);
    if inserted {
        bound.remove(name);
    }
}

fn with_bound_many<'a, I, F>(bound: &mut BTreeSet<String>, names: I, f: F)
where
    I: IntoIterator<Item = &'a String>,
    F: FnOnce(&mut BTreeSet<String>),
{
    let inserted = insert_bound_many(bound, names);
    f(bound);
    remove_inserted(bound, inserted);
}

fn insert_bound_many<'a, I>(bound: &mut BTreeSet<String>, names: I) -> Vec<String>
where
    I: IntoIterator<Item = &'a String>,
{
    names
        .into_iter()
        .filter_map(|name| bound.insert(name.clone()).then(|| name.clone()))
        .collect()
}

fn remove_inserted(bound: &mut BTreeSet<String>, inserted: Vec<String>) {
    for name in inserted {
        bound.remove(&name);
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
        assert!(
            err.contains(
                "projection expression references variables that are not in scope: missing"
            )
        );
    }

    #[test]
    fn rejects_unwind_rebinding_visible_name() {
        let err = analyze_error("MATCH (person) UNWIND [1] AS person RETURN person");
        assert!(err.contains("UNWIND alias `person` is already in scope"));
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
    fn rejects_union_column_mismatch() {
        let err = analyze_error("RETURN 1 AS left UNION RETURN 1 AS right");
        assert!(err.contains("UNION branches must project the same columns"));
    }
}
