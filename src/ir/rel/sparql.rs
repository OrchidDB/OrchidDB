//! Typed SPARQL algebra lowering over RDF quad datasets.
//!
//! Every SPARQL variable `?v` is carried as four columns: the lexical value
//! (`?v`) and the kind, datatype, and language companions named by
//! [`binding_identity_columns`]. A NULL kind means the variable is unbound
//! (or its expression raised an error). Joins use SPARQL solution
//! compatibility rather than SQL equality, so possibly-unbound variables join
//! correctly; expressions evaluate over RDF terms with SPARQL typing and
//! error rules.
//!
//! Result contract: a lowered SELECT returns the visible field values first,
//! followed by the three identity columns of every field in field order.
//! `LoweredPlan::fields` lists only the visible fields.

mod joins;
mod solution_ops;
mod aggregate;
mod expressions;
mod term_ops;
mod service;
use term_ops::*;

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Not;
use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::common::ScalarValue;
use datafusion::error::DataFusionError;
use datafusion::functions_aggregate::count::count_all;
use datafusion::functions_aggregate::expr_fn::{
    array_agg, count as df_count, max as df_max, min as df_min, sum as df_sum,
};
use datafusion::functions_window::expr_fn as df_window;
use datafusion::logical_expr::expr::Case;
use datafusion::logical_expr::{
    ColumnarValue, Expr, ExprFunctionExt, JoinType, LogicalPlan, LogicalPlanBuilder,
    ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, SortExpr, TryCast, Volatility,
};
use datafusion::prelude::lit;

use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit};
use crate::ir::plan::{
    ApplyKind, ConstructTriple, GraphPlan, JoinKind, Node, ProjectMode, ProjectionItem,
    RdfGraphScope, RdfTerm, Slice, SortDir, SortKey,
};
use crate::ir::policy::{Language, ResultForm};
use crate::ir::value::Value;

use super::rdf::{QuadSource, binding_identity_columns, quad_source};
use super::{IslandReport, LoweredPlan, LoweringContext, RelError, RelResult, col_exact};

#[path = "rdf_paths.rs"]
mod rdf_paths;

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const KIND_IRI: &str = "IRI";
const KIND_BLANK: &str = "BLANK";
const KIND_LITERAL: &str = "LITERAL";
const BNODE_SCOPE: &str = "__sq_bnode_scope";
const INTEGER_TYPES: &[&str] = &[
    "integer",
    "nonPositiveInteger",
    "negativeInteger",
    "long",
    "int",
    "short",
    "byte",
    "nonNegativeInteger",
    "unsignedLong",
    "unsignedInt",
    "unsignedShort",
    "unsignedByte",
    "positiveInteger",
];

fn xsd(local: &str) -> String {
    format!("{XSD}{local}")
}

/// Whether the typed SPARQL lowering owns this plan. Plans produced with an
/// ontology mapping contain property-graph scans and keep the generic path.
pub(super) fn handles(plan: &GraphPlan) -> bool {
    fn property_graph(node: &Node) -> bool {
        matches!(
            node,
            Node::GraphNodeScan { .. } | Node::GraphRelScan { .. } | Node::GraphExpand { .. }
        ) || super::node_children(node).into_iter().any(property_graph)
    }
    plan.policy.language == Language::Sparql && !property_graph(&plan.root)
}

pub(super) fn lower_plan(
    ctx: &mut LoweringContext<'_>,
    plan: &GraphPlan,
) -> RelResult<LoweredPlan> {
    let mut lowerer = Lowerer {
        ctx,
        seeds: Vec::new(),
        graph_domains: BTreeMap::new(),
        next: 0,
    };
    let (plan, fields, result_form) = lowerer.lower_root(&plan.root, plan.policy.result_form)?;
    Ok(LoweredPlan {
        plan,
        fields,
        result_form,
        islands: IslandReport {
            lowerable_nodes: 1,
            unsupported: Vec::new(),
        },
    })
}

fn unsupported<T>(message: impl Into<String>) -> RelResult<T> {
    Err(RelError::Unsupported(format!("SPARQL: {}", message.into())))
}

fn s(value: &str) -> Expr {
    lit(value.to_string())
}

fn null_str() -> Expr {
    lit(ScalarValue::Utf8(None))
}

fn null_bool() -> Expr {
    lit(ScalarValue::Boolean(None))
}

fn case(arms: Vec<(Expr, Expr)>, otherwise: Option<Expr>) -> Expr {
    Expr::Case(Case {
        expr: None,
        when_then_expr: arms
            .into_iter()
            .map(|(when, then)| (Box::new(when), Box::new(then)))
            .collect(),
        else_expr: otherwise.map(Box::new),
    })
}

fn try_cast(expr: Expr, data_type: DataType) -> Expr {
    Expr::TryCast(TryCast::new(Box::new(expr), data_type))
}

fn cast(expr: Expr, data_type: DataType) -> Expr {
    Expr::Cast(datafusion::logical_expr::Cast::new(
        Box::new(expr),
        data_type,
    ))
}

/// `left IS NOT DISTINCT FROM right`, spelled so the SQL unparser accepts it.
fn not_distinct(left: Expr, right: Expr) -> Expr {
    let null = |expr: &Expr| matches!(expr, Expr::Literal(value, _) if value.is_null());
    match (null(&left), null(&right)) {
        (true, true) => lit(true),
        (true, false) => right.is_null(),
        (false, true) => left.is_null(),
        (false, false) => left
            .clone()
            .eq(right.clone())
            .or(left.is_null().and(right.is_null())),
    }
}

/// The value of a constant text expression: `Some(None)` for NULL.
fn const_text(expr: &Expr) -> Option<Option<&str>> {
    match expr {
        Expr::Literal(ScalarValue::Utf8(value), _) => Some(value.as_deref()),
        Expr::Literal(ScalarValue::Null, _) => Some(None),
        _ => None,
    }
}

fn and_all(parts: Vec<Expr>) -> Expr {
    parts
        .into_iter()
        .reduce(Expr::and)
        .unwrap_or_else(|| lit(true))
}

fn or_all(parts: Vec<Expr>) -> Expr {
    parts
        .into_iter()
        .reduce(Expr::or)
        .unwrap_or_else(|| lit(false))
}

const INT: DataType = DataType::Decimal128(38, 0);
const DEC: DataType = DataType::Decimal128(38, 18);

// ---------------------------------------------------------------------------
// DuckDB function passthrough
// ---------------------------------------------------------------------------

/// A DuckDB scalar function referenced by name in generated SQL. It carries
/// only a return type for planning; in-process DataFusion execution reports
/// that the function is DuckDB-only rather than guessing its semantics.
#[derive(Debug, PartialEq, Eq, Hash)]
struct DuckDbFunction {
    name: String,
    return_type: DataType,
    signature: Signature,
}

pub(super) fn is_duck_function(function: &ScalarUDF) -> bool {
    function.inner().as_any().is::<DuckDbFunction>()
}

impl ScalarUDFImpl for DuckDbFunction {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(self.return_type.clone())
    }

    fn invoke_with_args(
        &self,
        _args: ScalarFunctionArgs,
    ) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::NotImplemented(format!(
            "`{}` is evaluated by DuckDB; execute this SPARQL plan through the DuckDB SQL backend",
            self.name
        )))
    }
}

fn duck(name: &str, args: Vec<Expr>, return_type: DataType) -> Expr {
    let volatility = if matches!(name, "random" | "uuid") {
        Volatility::Volatile
    } else if name == "now" {
        Volatility::Stable
    } else {
        Volatility::Immutable
    };
    let udf = ScalarUDF::new_from_impl(DuckDbFunction {
        name: name.into(),
        return_type,
        signature: if args.is_empty() { Signature::exact(vec![], volatility) } else { Signature::variadic_any(volatility) },
    });
    Arc::new(udf).call(args)
}

fn duck_str(name: &str, args: Vec<Expr>) -> Expr {
    duck(name, args, DataType::Utf8)
}

// ---------------------------------------------------------------------------
// RDF terms as expressions
// ---------------------------------------------------------------------------

/// One RDF term as four scalar expressions. `kind` is NULL for unbound or
/// error values; the other components are then irrelevant.
#[derive(Clone, Debug)]
struct Term {
    value: Expr,
    kind: Expr,
    dt: Expr,
    lang: Expr,
}

impl Term {
    fn columns(name: &str) -> Self {
        let [kind, dt, lang] = binding_identity_columns(name);
        Self {
            value: col_exact(name),
            kind: col_exact(kind),
            dt: col_exact(dt),
            lang: col_exact(lang),
        }
    }

    fn error() -> Self {
        Self {
            value: null_str(),
            kind: null_str(),
            dt: null_str(),
            lang: null_str(),
        }
    }

    fn iri(value: Expr) -> Self {
        Self {
            value,
            kind: s(KIND_IRI),
            dt: null_str(),
            lang: null_str(),
        }
    }

    fn literal(value: Expr, datatype: &str) -> Self {
        Self {
            value,
            kind: s(KIND_LITERAL),
            dt: s(datatype),
            lang: null_str(),
        }
    }

    fn string(value: Expr) -> Self {
        Self::literal(value, &xsd("string"))
    }

    fn integer(value: Expr) -> Self {
        Self::literal(cast(value, DataType::Utf8), &xsd("integer"))
    }

    /// A boolean literal from a nullable SQL boolean (NULL = error).
    /// Expressions whose natural result is a SQL boolean.
    fn boolean(value: Expr) -> Self {
        Self {
            value: case(
                vec![
                    (value.clone().is_true(), s("true")),
                    (value.clone().is_false(), s("false")),
                ],
                None,
            ),
            kind: case(
                vec![(value.clone().is_null(), null_str())],
                Some(s(KIND_LITERAL)),
            ),
            dt: case(
                vec![(value.is_null(), null_str())],
                Some(s(&xsd("boolean"))),
            ),
            lang: null_str(),
        }
    }

    /// Make the term an error wherever `condition` is not true.
    fn only_if(self, condition: Expr) -> Self {
        let guard = |expr: Expr| case(vec![(condition.clone(), expr)], None);
        Self {
            value: guard(self.value),
            kind: guard(self.kind),
            dt: guard(self.dt),
            lang: guard(self.lang),
        }
    }

    fn aliased(self, name: &str) -> Vec<Expr> {
        let [kind, dt, lang] = binding_identity_columns(name);
        vec![
            self.value.alias(name),
            self.kind.alias(kind),
            self.dt.alias(dt),
            self.lang.alias(lang),
        ]
    }

    fn bound(&self) -> Expr {
        self.kind.clone().is_not_null()
    }

    fn is_literal(&self) -> Expr {
        self.kind.clone().eq(s(KIND_LITERAL))
    }

    fn has_datatype(&self, datatype: &str) -> Expr {
        self.is_literal().and(self.dt.clone().eq(s(datatype)))
    }

    /// Simple literal or `xsd:string`.
    fn is_simple(&self) -> Expr {
        self.has_datatype(&xsd("string"))
    }

    /// A string literal argument: simple, `xsd:string`, or language-tagged.
    fn is_string(&self) -> Expr {
        self.is_literal().and(
            self.dt
                .clone()
                .in_list(vec![s(&xsd("string")), s(RDF_LANG_STRING)], false),
        )
    }

    /// 1 integer, 2 decimal, 3 float, 4 double; NULL for non-numeric terms.
    fn numeric_rank(&self) -> Expr {
        if let (Some(kind), Some(dt)) = (const_text(&self.kind), const_text(&self.dt)) {
            let rank = match (kind, dt.and_then(|dt| dt.strip_prefix(XSD))) {
                (Some(KIND_LITERAL), Some(local)) if INTEGER_TYPES.contains(&local) => Some(1),
                (Some(KIND_LITERAL), Some("decimal")) => Some(2),
                (Some(KIND_LITERAL), Some("float")) => Some(3),
                (Some(KIND_LITERAL), Some("double")) => Some(4),
                _ => None,
            };
            return lit(ScalarValue::Int64(rank));
        }
        case(
            vec![
                (
                    self.is_literal().and(
                        self.dt
                            .clone()
                            .in_list(INTEGER_TYPES.iter().map(|t| s(&xsd(t))).collect(), false),
                    ),
                    lit(1_i64),
                ),
                (self.has_datatype(&xsd("decimal")), lit(2_i64)),
                (self.has_datatype(&xsd("float")), lit(3_i64)),
                (self.has_datatype(&xsd("double")), lit(4_i64)),
            ],
            None,
        )
    }

    fn is_numeric(&self) -> Expr {
        self.numeric_rank().is_not_null()
    }

    fn boolean_value(&self) -> Expr {
        case(
            vec![
                (
                    self.value.clone().in_list(vec![s("true"), s("1")], false),
                    lit(true),
                ),
                (
                    self.value.clone().in_list(vec![s("false"), s("0")], false),
                    lit(false),
                ),
            ],
            None,
        )
    }

    fn same_term(&self, other: &Term) -> Expr {
        and_all(vec![
            self.kind.clone().eq(other.kind.clone()),
            self.value.clone().eq(other.value.clone()),
            not_distinct(self.dt.clone(), other.dt.clone()),
            not_distinct(self.lang.clone(), other.lang.clone()),
        ])
    }
}

/// Canonical `xsd:decimal` lexical form of a SQL decimal expression.
fn decimal_lexical(value: Expr) -> Expr {
    let text = cast(cast(value, DEC), DataType::Utf8);
    let trimmed = duck_str("rtrim", vec![text, s("0")]);
    case(
        vec![(
            trimmed.clone().like(s("%.")),
            duck_str("concat", vec![trimmed.clone(), s("0")]),
        )],
        Some(trimmed),
    )
}

fn double_lexical(value: Expr) -> Expr {
    cast(value, DataType::Utf8)
}

fn integer_lexical(value: Expr) -> Expr {
    cast(value, DataType::Utf8)
}

// ---------------------------------------------------------------------------
// Solution relations
// ---------------------------------------------------------------------------

/// A relation of SPARQL solutions.
#[derive(Clone, Debug)]
struct Sol {
    plan: LogicalPlan,
    /// Variables present as columns; the flag is true when always bound.
    vars: BTreeMap<String, bool>,
    /// Hidden per-input-row identity columns used for correlation.
    keys: BTreeSet<String>,
    /// Hidden ordinal established by ORDER BY.
    ord: Option<String>,
}

impl Sol {
    fn columns(&self) -> Vec<Expr> {
        let mut out = Vec::new();
        for var in self.vars.keys() {
            out.extend(var_columns(var).into_iter().map(col_exact));
        }
        out.extend(self.keys.iter().map(col_exact));
        if let Some(ord) = &self.ord {
            out.push(col_exact(ord));
        }
        if self.plan.schema().has_column_with_unqualified_name(BNODE_SCOPE) {
            out.push(col_exact(BNODE_SCOPE));
        }
        out
    }

    fn column_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        for var in self.vars.keys() {
            out.extend(var_columns(var));
        }
        out.extend(self.keys.iter().cloned());
        out.extend(self.ord.iter().cloned());
        out
    }

    fn term(&self, var: &str) -> Option<Term> {
        self.vars.contains_key(var).then(|| Term::columns(var))
    }
}

fn var_columns(var: &str) -> Vec<String> {
    let mut out = vec![var.to_string()];
    out.extend(binding_identity_columns(var));
    out
}

/// Expression evaluation state: a plan that may be extended with
/// materialized intermediate terms, and the variables in scope.
struct Env {
    plan: LogicalPlan,
    vars: BTreeMap<String, bool>,
    temps: Vec<String>,
}

impl Env {
    fn term(&self, var: &str) -> Term {
        if self.vars.contains_key(var) {
            Term::columns(var)
        } else {
            Term::error()
        }
    }
}

struct Lowerer<'c, 'a> {
    ctx: &'c mut LoweringContext<'a>,
    /// Correlation seeds for EXISTS: the outer relation, with a row key.
    seeds: Vec<Sol>,
    graph_domains: BTreeMap<String, Sol>,
    next: usize,
}

impl Lowerer<'_, '_> {
    fn filter_plan(&self, plan: LogicalPlan, predicate: Expr) -> RelResult<LogicalPlan> {
        Ok(LogicalPlanBuilder::from(plan)
            .filter(fold(predicate)?)?
            .build()?)
    }

    /// Project over a window node. The SQL unparser drops window results
    /// unless a non-identity projection sits directly above the window, so a
    /// private guard column keeps this projection in place.
    fn window_projection(
        &mut self,
        plan: LogicalPlan,
        mut columns: Vec<Expr>,
    ) -> RelResult<LogicalPlan> {
        columns.push(lit(1_i64).alias(self.fresh("window_guard")));
        self.project(plan, columns)
    }

    // -- root ---------------------------------------------------------------

    fn lower_root(
        &mut self,
        node: &Node,
        policy_form: ResultForm,
    ) -> RelResult<(LogicalPlan, Vec<String>, ResultForm)> {
        match node {
            Node::GraphReturn {
                fields,
                result_form,
                input,
            } => {
                let sol = self.lower(input)?;
                Ok((self.result_rows(sol, fields)?, fields.clone(), *result_form))
            }
            Node::GraphAsk { field, input } => {
                let sol = self.lower(input)?;
                let limited = LogicalPlanBuilder::from(sol.plan)
                    .limit(0, Some(1))?
                    .build()?;
                let limited = self.cte(limited)?;
                let counted = LogicalPlanBuilder::from(limited)
                    .aggregate(
                        Vec::<Expr>::new(),
                        vec![count_all().alias("__sq_ask_count")],
                    )?
                    .build()?;
                let plan = self.project(
                    counted,
                    vec![col_exact("__sq_ask_count").gt(lit(0_i64)).alias(field)],
                )?;
                Ok((plan, vec![field.clone()], ResultForm::Boolean))
            }
            Node::GraphConstructTriples { template, input } => {
                let sol = self.lower(input)?;
                let plan = self.construct(sol, template)?;
                Ok((
                    plan,
                    vec!["subject".into(), "predicate".into(), "object".into()],
                    ResultForm::RdfGraph,
                ))
            }
            Node::GraphDescribe { .. } => {
                unsupported("DESCRIBE has no relational result policy yet")
            }
            other => {
                let sol = self.lower(other)?;
                let fields: Vec<_> = sol.vars.keys().cloned().collect();
                Ok((self.result_rows(sol, &fields)?, fields, policy_form))
            }
        }
    }

    fn result_rows(&mut self, sol: Sol, fields: &[String]) -> RelResult<LogicalPlan> {
        let mut values = Vec::new();
        let mut identity = Vec::new();
        for field in fields {
            let term = sol.term(field).unwrap_or_else(Term::error);
            let [kind, dt, lang] = binding_identity_columns(field);
            values.push(term.value.alias(field));
            identity.extend([
                term.kind.alias(kind),
                term.dt.alias(dt),
                term.lang.alias(lang),
            ]);
        }
        values.extend(identity);
        let mut plan = sol.plan;
        if let Some(ord) = &sol.ord {
            // Carry the ordinal through the final projection, sort, then drop.
            let mut with_ord = values.clone();
            with_ord.push(col_exact(ord));
            plan = self.project(plan, with_ord)?;
            plan = LogicalPlanBuilder::from(plan)
                .sort(vec![col_exact(ord).sort(true, false)])?
                .build()?;
            let names: Vec<_> = plan
                .schema()
                .fields()
                .iter()
                .map(|field| field.name().clone())
                .filter(|name| name != ord)
                .collect();
            return self.project(plan, names.into_iter().map(col_exact).collect());
        }
        self.project(plan, values)
    }

    // -- algebra ------------------------------------------------------------

    fn lower(&mut self, node: &Node) -> RelResult<Sol> {
        match node {
            Node::GraphOneRow => match self.seeds.last() {
                Some(seed) => Ok(seed.clone()),
                None => self.one_row(),
            },
            Node::GraphEmpty => {
                let row = self.one_row()?;
                let plan = LogicalPlanBuilder::from(row.plan)
                    .filter(lit(false))?
                    .build()?;
                Ok(Sol { plan, ..row })
            }
            Node::GraphSparqlGraphNames { dataset, graph_scope } => {
                let (plan, column) = super::rdf::named_graphs(self.ctx, dataset, graph_scope)?;
                let mut vars = BTreeMap::new();
                let columns = match graph_scope {
                    RdfGraphScope::NamedGraphVariable(variable)
                    | RdfGraphScope::DatasetNamedGraphVariable { variable, .. } => {
                        vars.insert(variable.clone(), true);
                        Term::iri(col_exact(column)).aliased(variable)
                    }
                    _ => vec![lit(1_i64).alias(self.fresh("graph"))],
                };
                let sol = Sol { plan: self.project(plan, columns)?, vars,
                    keys: BTreeSet::new(), ord: None };
                for name in sol.vars.keys().filter(|name| name.starts_with("__sq_graph_scope_")) {
                    self.graph_domains.insert(name.clone(), sol.clone());
                }
                Ok(sol)
            }
            Node::GraphSparqlTriplePattern {
                dataset,
                graph_scope,
                subject,
                predicate,
                object,
                ..
            } => self.triple_pattern(dataset, graph_scope, subject, predicate, object),
            Node::GraphJoin {
                kind,
                left,
                right,
                condition,
            } => {
                let left = self.lower(left)?;
                let right = self.lower(right)?;
                match (kind, condition) {
                    (JoinKind::Inner, None) => self.join(left, right, false),
                    (JoinKind::LeftOuter, None) => self.join(left, right, true),
                    (JoinKind::LeftOuter, Some(condition)) => {
                        self.left_join_filtered(left, right, condition)
                    }
                    (JoinKind::Inner, Some(condition)) => {
                        let joined = self.join(left, right, false)?;
                        self.filter(joined, condition)
                    }
                    (other, _) => unsupported(format!("{other:?} join")),
                }
            }
            Node::GraphFilter { condition, input } => {
                let input = self.lower(input)?;
                let input = self.ensure_seeded(input)?;
                self.filter(input, condition)
            }
            Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items,
                input,
                ..
            } => {
                let input = self.lower(input)?;
                let input = self.ensure_seeded(input)?;
                self.extend(input, items)
            }
            Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items,
                input,
                ..
            } => {
                // A sub-select is evaluated bottom-up, without the bindings
                // of an enclosing EXISTS.
                let seeds = std::mem::take(&mut self.seeds);
                let input = self.lower(input);
                self.seeds = seeds;
                self.select(input?, items)
            }
            Node::GraphDistinct { input, .. } => {
                let input = self.lower(input)?;
                self.distinct(input)
            }
            Node::GraphSlice { slice, input } => {
                let input = self.lower(input)?;
                self.slice(input, slice)
            }
            Node::GraphSort { keys, input } => {
                let input = self.lower(input)?;
                self.sort(input, keys)
            }
            Node::GraphUnion { left, right, .. } => {
                let left = self.lower(left)?;
                let left = self.ensure_seeded(left)?;
                let right = self.lower(right)?;
                let right = self.ensure_seeded(right)?;
                self.union(left, right)
            }
            Node::GraphSparqlMinus { left, right, shared, .. } => {
                let left = self.lower(left)?;
                let seeds = std::mem::take(&mut self.seeds);
                let right = self.lower(right);
                self.seeds = seeds;
                self.minus(left, right?, shared)
            }
            Node::GraphValues { bindings, rows, .. } => self.values(bindings, rows),
            Node::GraphApply {
                kind: kind @ (ApplyKind::Semi | ApplyKind::Anti),
                left,
                right,
                ..
            } => {
                let left = self.lower(left)?;
                self.exists(left, right, *kind == ApplyKind::Anti)
            }
            Node::GraphAggregate {
                group, aggs, input, ..
            } => {
                let input = self.lower(input)?;
                self.aggregate(input, group, aggs)
            }
            Node::GraphRdfPropertyPath { dataset, graph_scope, subject, object, path, .. } => {
                self.property_path(dataset, graph_scope, subject, object, path)
            }
            Node::GraphService { endpoint, query, outputs, silent, .. } => self.service(endpoint, query, outputs, *silent),
            Node::GraphApply {
                kind: ApplyKind::Scalar,
                outputs,
                left,
                right,
                ..
            } => {
                // `EXISTS` inside an expression: COUNT(*) over the pattern
                // limited to one row.
                let (
                    [mark],
                    Node::GraphAggregate {
                        group, aggs, input, ..
                    },
                ) = (outputs.as_slice(), right.as_ref())
                else {
                    return unsupported("scalar apply other than an EXISTS mark");
                };
                let pattern = match (group.as_slice(), aggs.as_slice(), input.as_ref()) {
                    (
                        [],
                        [
                            AggCall {
                                kind: AggKind::CountRows,
                                arg: None,
                                alias,
                                ..
                            },
                        ],
                        Node::GraphSlice {
                            slice:
                                Slice {
                                    offset: 0,
                                    fetch: Some(1),
                                    tail: None,
                                },
                            input,
                        },
                    ) if alias == mark => input,
                    _ => return unsupported("scalar apply other than an EXISTS mark"),
                };
                let left = self.lower(left)?;
                self.exists_mark(left, pattern, mark)
            }
            Node::GraphApply { kind, .. } => {
                unsupported(format!("{kind:?} apply in SPARQL algebra"))
            }
            other => unsupported(format!(
                "{} is not part of the typed SPARQL algebra",
                super::unsupported_node_name(other)
            )),
        }
    }

    fn one_row(&mut self) -> RelResult<Sol> {
        let lowered = self.ctx.lower_node(&Node::GraphOneRow)?;
        Ok(Sol {
            plan: lowered.plan,
            vars: BTreeMap::new(),
            keys: BTreeSet::new(),
            ord: None,
        })
    }

    fn triple_pattern(
        &mut self,
        dataset: &str,
        graph_scope: &RdfGraphScope,
        subject: &RdfTerm,
        predicate: &RdfTerm,
        object: &RdfTerm,
    ) -> RelResult<Sol> {
        let QuadSource {
            plan,
            names,
            identity,
            typed,
        } = quad_source(self.ctx, dataset, graph_scope)?;
        if !typed
            && [subject, predicate, object]
                .iter()
                .any(|term| !matches!(term, RdfTerm::Variable(_) | RdfTerm::Iri(_)))
        {
            return Err(RelError::Unsupported(
                "IRI-only RDF quad mapping cannot match literal or blank-node terms; a typed RDF term source is required".into(),
            ));
        }
        let role_term = |role: usize| Term {
            value: col_exact(&names[role]),
            kind: col_exact(&identity[role][0]),
            dt: col_exact(&identity[role][1]),
            lang: col_exact(&identity[role][2]),
        };
        let mut conditions = vec![
            col_exact(&names[1]).is_not_null(),
            col_exact(&names[2]).is_not_null(),
            col_exact(&names[3]).is_not_null(),
        ];
        let mut first_role = BTreeMap::<String, usize>::new();
        let graph_variable = match graph_scope {
            RdfGraphScope::NamedGraphVariable(variable)
            | RdfGraphScope::DatasetNamedGraphVariable { variable, .. } => Some(variable),
            _ => None,
        };
        let roles = [(1, subject), (2, predicate), (3, object)];
        let mut bindings: Vec<(usize, &str)> = roles
            .iter()
            .filter_map(|(role, term)| match term {
                RdfTerm::Variable(variable) => Some((*role, variable.as_str())),
                _ => None,
            })
            .collect();
        if let Some(variable) = graph_variable {
            bindings.push((0, variable.as_str()));
        }
        for (role, term) in roles {
            let actual = role_term(role);
            let constant = match term {
                RdfTerm::Variable(_) => None,
                RdfTerm::Iri(iri) => Some(Term::iri(s(iri))),
                RdfTerm::BlankNode(label) => Some(Term {
                    value: s(label),
                    kind: s(KIND_BLANK),
                    dt: null_str(),
                    lang: null_str(),
                }),
                RdfTerm::LanguageTagged { value, lang } => Some(Term {
                    value: s(value),
                    kind: s(KIND_LITERAL),
                    dt: s(RDF_LANG_STRING),
                    lang: s(&lang.to_ascii_lowercase()),
                }),
                RdfTerm::Typed { lexical, datatype } => Some(Term::literal(s(lexical), datatype)),
                RdfTerm::Literal(value) => {
                    let (lexical, datatype) = literal_identity(value)?;
                    Some(Term::literal(s(&lexical), &datatype))
                }
            };
            if let Some(constant) = constant {
                conditions.push(actual.same_term(&constant));
            }
        }
        for (role, variable) in &bindings {
            match first_role.get(*variable) {
                Some(previous) => {
                    conditions.push(role_term(*previous).same_term(&role_term(*role)))
                }
                None => {
                    first_role.insert(variable.to_string(), *role);
                }
            }
        }
        let plan = self.filter_plan(plan, and_all(conditions))?;
        let mut projections = Vec::new();
        let mut vars = BTreeMap::new();
        for (variable, role) in &first_role {
            projections.extend(role_term(*role).aliased(variable));
            vars.insert(variable.clone(), true);
        }
        if projections.is_empty() {
            projections.push(lit(1_i64).alias(self.fresh("match")));
        }
        let plan = self.project(plan, projections)?;
        Ok(Sol {
            plan,
            vars,
            keys: BTreeSet::new(),
            ord: None,
        })
    }


    fn fresh(&mut self, base: &str) -> String {
        let id = self.next;
        self.next += 1;
        format!("__sq{id}_{base}")
    }

    /// Give a relation a SQL CTE boundary.
    fn cte(&mut self, plan: LogicalPlan) -> RelResult<LogicalPlan> {
        let name = format!("__w_sql_cte_sparql_{}", self.next);
        self.next += 1;
        Ok(LogicalPlanBuilder::from(plan).alias(name)?.build()?)
    }

    fn project(&self, plan: LogicalPlan, exprs: Vec<Expr>) -> RelResult<LogicalPlan> {
        let exprs = exprs.into_iter().map(fold).collect::<RelResult<Vec<_>>>()?;
        Ok(LogicalPlanBuilder::from(plan).project(exprs)?.build()?)
    }

    /// SQL has no zero-column SELECT; give variable-free solutions a
    /// private constant column.
    fn nonempty(&mut self, mut columns: Vec<Expr>) -> Vec<Expr> {
        if columns.is_empty() {
            columns.push(lit(1_i64).alias(self.fresh("row")));
        }
        columns
    }

    fn construct(&mut self, sol: Sol, template: &[ConstructTriple]) -> RelResult<LogicalPlan> {
        // A materialized UUID namespace per solution prevents template blank
        // nodes from colliding with source labels or another result graph.
        let (mut sol, key) = self.with_row_key(sol)?;
        let blank_key = self.fresh("template_blank");
        if template.iter().any(|triple| [&triple.subject, &triple.predicate, &triple.object]
            .iter().any(|term| matches!(term, RdfTerm::BlankNode(_)))) {
            let mut columns = sol.columns();
            columns.push(cast(duck_str("uuid", vec![]), DataType::Utf8).alias(&blank_key));
            sol.plan = self.project(sol.plan, columns)?;
            sol.plan = self.cte(sol.plan)?;
        }
        let mut branches = Vec::new();
        for triple in template {
            let term = |term: &RdfTerm| -> RelResult<Term> {
                Ok(match term {
                    RdfTerm::Variable(variable) => sol.term(variable).unwrap_or_else(Term::error),
                    RdfTerm::Iri(iri) => Term::iri(s(iri)),
                    RdfTerm::BlankNode(label) => Term {
                        value: duck_str(
                            "concat",
                            vec![s(label), s("_"), col_exact(&blank_key), s("_"), cast(col_exact(&key), DataType::Utf8)],
                        ),
                        kind: s(KIND_BLANK),
                        dt: null_str(),
                        lang: null_str(),
                    },
                    RdfTerm::LanguageTagged { value, lang } => Term {
                        value: s(value),
                        kind: s(KIND_LITERAL),
                        dt: s(RDF_LANG_STRING),
                        lang: s(&lang.to_ascii_lowercase()),
                    },
                    RdfTerm::Typed { lexical, datatype } => Term::literal(s(lexical), datatype),
                    RdfTerm::Literal(value) => {
                        let (lexical, datatype) = literal_identity(value)?;
                        Term::literal(s(&lexical), &datatype)
                    }
                })
            };
            let (subject, predicate, object) = (
                term(&triple.subject)?,
                term(&triple.predicate)?,
                term(&triple.object)?,
            );
            // Invalid triples (unbound positions, literal subjects,
            // non-IRI predicates) are omitted from the result graph.
            let valid = and_all(vec![
                subject
                    .kind
                    .clone()
                    .in_list(vec![s(KIND_IRI), s(KIND_BLANK)], false),
                predicate.kind.clone().eq(s(KIND_IRI)),
                object.bound(),
            ]);
            let filtered = self.filter_plan(sol.plan.clone(), valid)?;
            let mut columns = vec![
                subject.value.clone().alias("subject"),
                predicate.value.clone().alias("predicate"),
                object.value.clone().alias("object"),
            ];
            for (name, term) in [
                ("subject", subject),
                ("predicate", predicate),
                ("object", object),
            ] {
                let [kind, dt, lang] = binding_identity_columns(name);
                columns.extend([
                    term.kind.alias(kind),
                    term.dt.alias(dt),
                    term.lang.alias(lang),
                ]);
            }
            branches.push(self.project(filtered, columns)?);
        }
        let mut branches = branches.into_iter();
        let Some(mut plan) = branches.next() else {
            let empty = self.lower(&Node::GraphEmpty)?;
            let mut columns = Vec::new();
            for name in ["subject", "predicate", "object"] {
                columns.extend(Term::error().aliased(name));
            }
            return self.project(empty.plan, columns);
        };
        for branch in branches {
            plan = LogicalPlanBuilder::from(plan).union(branch)?.build()?;
        }
        // An RDF graph is a set of triples.
        Ok(LogicalPlanBuilder::from(plan).distinct()?.build()?)
    }


}

/// Variables read by expressions in a pattern, excluding sub-selects (which
/// never see enclosing bindings).
fn expression_variables(node: &Node, out: &mut BTreeSet<String>) {
    fn expr_vars(expr: &IrExpr, out: &mut BTreeSet<String>) {
        match expr {
            IrExpr::Binding(name) | IrExpr::IsBound(name) => {
                out.insert(name.clone());
            }
            IrExpr::Binary { lhs, rhs, .. } => {
                expr_vars(lhs, out);
                expr_vars(rhs, out);
            }
            IrExpr::Not(inner) | IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) => {
                expr_vars(inner, out)
            }
            IrExpr::Call { args, .. } | IrExpr::List(args) => {
                args.iter().for_each(|arg| expr_vars(arg, out))
            }
            IrExpr::Case { arms, otherwise } => {
                for (when, then) in arms {
                    expr_vars(when, out);
                    expr_vars(then, out);
                }
                if let Some(otherwise) = otherwise {
                    expr_vars(otherwise, out);
                }
            }
            _ => {}
        }
    }
    match node {
        Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            ..
        } => return,
        Node::GraphFilter { condition, .. } => expr_vars(condition, out),
        Node::GraphProject { items, .. } => {
            items.iter().for_each(|item| expr_vars(&item.expr, out))
        }
        Node::GraphJoin {
            condition: Some(condition),
            ..
        } => expr_vars(condition, out),
        Node::GraphSort { keys, .. } => keys.iter().for_each(|key| expr_vars(&key.expr, out)),
        _ => {}
    }
    for child in super::node_children(node) {
        expression_variables(child, out);
    }
}
