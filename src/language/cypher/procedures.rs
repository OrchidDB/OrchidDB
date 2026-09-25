//! Resolve procedure signatures before parameter binding and semantic analysis.
use super::ast::{Clause, Expr, Literal, ProcedureYieldItem, Query};
use super::planner::{CypherPlanError, CypherPlanResult, CypherSemanticError as Code};
use crate::ir::procedures::ProcedureCatalog;
use crate::ir::value::Value;
use std::collections::BTreeSet;

pub fn prepare(query: &mut Query, catalog: &ProcedureCatalog) -> CypherPlanResult<()> {
    for clause in &mut query.clauses {
        match &*clause {
            Clause::Match(clause) => {
                for pattern in &clause.patterns {
                    validate_pattern_parameters(pattern)?;
                }
            }
            Clause::Merge(clause) => validate_pattern_parameters(&clause.pattern)?,
            _ => {}
        }
        let Clause::Call(call) = clause else { continue };
        let Some(procedure) = catalog.get(&call.name) else {
            if matches!(
                call.name.to_ascii_lowercase().as_str(),
                "db.labels" | "db.relationshiptypes" | "db.propertykeys"
            ) {
                continue;
            }
            return Err(error(
                Code::ProcedureNotFound,
                format!("Unknown procedure {}", call.name),
            ));
        };
        let signature = &procedure.signature;
        if call.implicit_arguments {
            if !call.standalone {
                return Err(error(
                    Code::InvalidArgumentPassingMode,
                    "Implicit arguments require a standalone call",
                ));
            }
            call.args = signature
                .inputs
                .iter()
                .map(|field| Expr::Parameter(field.name.clone()))
                .collect();
            call.implicit_arguments = false;
        }
        if call.args.len() != signature.inputs.len() {
            return Err(error(
                Code::InvalidNumberOfArguments,
                format!(
                    "{} requires {} arguments",
                    call.name,
                    signature.inputs.len()
                ),
            ));
        }
        for (arg, field) in call.args.iter().zip(&signature.inputs) {
            let value = match arg {
                Expr::Literal(Literal::Bool(v)) => Some(Value::Bool(*v)),
                Expr::Literal(Literal::String(v)) => Some(Value::String(v.clone())),
                Expr::Literal(Literal::Integer(v)) => v.parse().ok().map(Value::Int),
                Expr::Literal(Literal::Float(v)) => Some(Value::Float(*v)),
                Expr::Literal(Literal::Null) => Some(Value::Null),
                _ => None,
            };
            if value.as_ref().is_some_and(|value| !field.accepts(value)) {
                return Err(error(
                    Code::InvalidArgumentType,
                    format!("Invalid type for procedure argument {}", field.name),
                ));
            }
        }
        if call.yield_all && !call.standalone {
            return Err(error(
                Code::UnexpectedSyntax,
                "YIELD * requires a standalone call",
            ));
        }
        if call.yields.is_empty() && (call.standalone || call.yield_all) {
            call.yields = signature
                .outputs
                .iter()
                .map(|field| ProcedureYieldItem {
                    field: field.name.clone(),
                    alias: field.name.clone(),
                })
                .collect();
        }
        let mut aliases = BTreeSet::new();
        for output in &call.yields {
            if !aliases.insert(&output.alias) {
                return Err(error(
                    Code::VariableAlreadyBound,
                    "Duplicate procedure output alias",
                ));
            }
            if !signature
                .outputs
                .iter()
                .any(|field| field.name == output.field)
            {
                return Err(error(
                    Code::UndefinedVariable,
                    format!("Unknown procedure output {}", output.field),
                ));
            }
        }
        call.signature = Some(signature.clone());
    }
    for branch in &mut query.unions {
        prepare(&mut branch.query, catalog)?;
    }
    Ok(())
}

fn error(code: Code, message: impl Into<String>) -> CypherPlanError {
    CypherPlanError::Invalid(message.into()).classified(code)
}

fn validate_pattern_parameters(pattern: &super::ast::PatternPart) -> CypherPlanResult<()> {
    let element = &pattern.element;
    for properties in element
        .start
        .properties
        .iter()
        .chain(element.chains.iter().flat_map(|chain| {
            chain
                .relationship
                .properties
                .iter()
                .chain(chain.node.properties.iter())
        }))
    {
        if matches!(properties, Expr::Parameter(_)) {
            return Err(error(
                Code::InvalidParameterUse,
                "MATCH and MERGE patterns require explicit property keys",
            ));
        }
    }
    Ok(())
}
