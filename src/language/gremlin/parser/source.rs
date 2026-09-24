//! Traversal source configuration and strategy lowering.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use antlr4rust::tree::ParseTree;

use super::literals::sack_op_from_text;
use super::{GValue, LoweringVisitor, Predicate, Rc, Step};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn visit_traversalSource_recursive<'input>(
        &mut self,
        ctx: &TraversalSourceContext<'input>,
    ) {
        // The grammar is left-recursive: `g.X.Y.Z` is parsed as
        // ((g.X).Y).Z so the innermost method is the deepest. Walk inward
        // first, then handle the outermost self-method on the way back so
        // the resulting Step list mirrors source order.
        if let Some(inner) = ctx.traversalSource() {
            self.visit_traversalSource_recursive(&inner);
        }
        if let Some(method) = ctx.traversalSourceSelfMethod() {
            self.visit_traversalSourceSelfMethod_lower(&method);
        }
    }

    pub(super) fn visit_traversalSourceSelfMethod_lower<'input>(
        &mut self,
        ctx: &TraversalSourceSelfMethodContext<'input>,
    ) {
        // Source-self methods are configuration knobs. We treat any failure
        // in their literal sub-parses as "skip the configuration" rather
        // than propagating the error: the rest of the traversal should
        // still compile (e.g. `withSack(BigInteger.TEN.pow(1000))` carries
        // an integer literal that overflows i64; without this guard the
        // whole traversal fails to parse).
        let errors_before = self.errors.len();
        if let Some(c) = ctx.traversalSourceSelfMethod_withoutStrategies() {
            if c.get_text().split(|c: char| !c.is_alphanumeric()).any(|s| s == "PathRetractionStrategy") {
                self.steps.push(Step::WithoutPathRetraction);
            }
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withBulk() {
            self.steps.push(Step::WithBulk(!c.get_text().contains("false")));
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withSack() {
            let Some(lit) = c.genericLiteral() else {
                return;
            };
            self.visit_genericLiteral(&lit);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.value_stack.clear();
                return;
            }
            let Some(initial) = self.pop_value() else {
                return;
            };
            let op = c.traversalBiFunction().and_then(|b| {
                b.traversalOperator()
                    .and_then(|o| sack_op_from_text(&o.get_text()))
            });
            self.steps.push(Step::WithSack { initial, op });
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withSideEffect() {
            let Some(s) = c.stringLiteral() else { return };
            self.visit_stringLiteral(&s);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.string_stack.clear();
                return;
            }
            let Some(label) = self.pop_string() else {
                return;
            };
            let Some(lit) = c.genericLiteral() else {
                return;
            };
            self.visit_genericLiteral(&lit);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.value_stack.clear();
                return;
            }
            let Some(initial) = self.pop_value() else {
                return;
            };
            let op = c.traversalBiFunction().and_then(|b| {
                b.traversalOperator()
                    .and_then(|o| sack_op_from_text(&o.get_text()))
            });
            self.steps.push(Step::WithSideEffect { label, initial, op });
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withStrategies() {
            // Walk every strategy in the var-args. SubgraphStrategy
            // contributes graph-visibility filters; ProductiveByStrategy
            // changes `by(...)` productivity from drop-row to keep-NULL.
            let mut strategies: Vec<Rc<TraversalStrategyContextAll<'input>>> = Vec::new();
            if let Some(first) = c.traversalStrategy() {
                strategies.push(first);
            }
            if let Some(varargs) = c.traversalStrategyVarargs() {
                if let Some(expr) = varargs.traversalStrategyExpr() {
                    strategies.extend(expr.traversalStrategy_all());
                }
            }
            for strat in strategies {
                let class_name = strat
                    .classType()
                    .map(|ct| ct.get_text())
                    .unwrap_or_default();
                if class_name == "ProductiveByStrategy" {
                    self.steps.push(Step::WithProductiveByStrategy);
                    continue;
                }
                if class_name == "PartitionStrategy" {
                    let mut partition_key = "_partition".to_string();
                    let mut read_partitions: Vec<GValue> = Vec::new();
                    let mut write_partition = None;
                    for cfg in strat.configuration_all() {
                        let key_text = cfg
                            .keyword()
                            .map(|k| k.get_text())
                            .or_else(|| cfg.nakedKey().map(|k| k.get_text()))
                            .unwrap_or_default();
                        let Some(arg) = cfg.genericArgument() else {
                            continue;
                        };
                        let Some(lit) = arg.genericLiteral() else {
                            continue;
                        };
                        match key_text.as_str() {
                            "partitionKey" => {
                                self.visit_genericLiteral(&lit);
                                if self.errors.len() != errors_before {
                                    self.errors.truncate(errors_before);
                                    self.value_stack.clear();
                                    continue;
                                }
                                if let Some(GValue::String(key)) = self.pop_value() {
                                    partition_key = key;
                                }
                            }
                            "writePartition" => {
                                self.visit_genericLiteral(&lit);
                                write_partition = self.pop_value();
                            }
                            "includeMetaProperties" if lit.get_text() == "true" => {
                                self.errors.push(super::GremlinError::Unsupported("PartitionStrategy includeMetaProperties requires meta-property storage".into()));
                                return;
                            }
                            "readPartitions" => {
                                self.visit_genericLiteral(&lit);
                                if self.errors.len() != errors_before {
                                    self.errors.truncate(errors_before);
                                    self.value_stack.clear();
                                    continue;
                                }
                                match self.pop_value() {
                                    Some(GValue::List(values)) => read_partitions.extend(values),
                                    Some(value) => read_partitions.push(value),
                                    None => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    {
                        let filter = vec![Step::Has {
                            key: partition_key.clone(),
                            predicate: Predicate::Within(read_partitions),
                        }];
                        self.steps.push(Step::WithStrategy {
                            vertex_filter: Some(filter.clone()),
                            edge_filter: Some(filter),
                            vertex_property_filter: None,
                            // Partition visibility is checked on each returned element;
                            // an edge may connect to a vertex outside the read partitions.
                            check_adjacent_vertices: false,
                        });
                    }
                    if let Some(value) = write_partition {
                        self.steps.push(Step::WithPartitionWrite { key: partition_key, value });
                    }
                    continue;
                }
                if class_name != "SubgraphStrategy" {
                    continue;
                }
                let mut vertex_filter: Option<Vec<Step>> = None;
                let mut edge_filter: Option<Vec<Step>> = None;
                let mut vertex_property_filter: Option<Vec<Step>> = None;
                let mut check_adjacent_vertices = true;
                for cfg in strat.configuration_all() {
                    let key_text = cfg
                        .keyword()
                        .map(|k| k.get_text())
                        .or_else(|| cfg.nakedKey().map(|k| k.get_text()))
                        .unwrap_or_default();
                    let Some(arg) = cfg.genericArgument() else {
                        continue;
                    };
                    if key_text == "checkAdjacentVertices" {
                        check_adjacent_vertices = arg.get_text() != "false";
                        continue;
                    }
                    let Some(lit) = arg.genericLiteral() else {
                        continue;
                    };
                    let Some(nested) = lit.nestedTraversal() else {
                        continue;
                    };
                    let steps = self.lower_nested_traversal(&nested);
                    if self.errors.len() != errors_before {
                        self.errors.truncate(errors_before);
                        continue;
                    }
                    match key_text.as_str() {
                        "vertices" => vertex_filter = Some(steps),
                        "edges" => edge_filter = Some(steps),
                        "vertexProperties" => vertex_property_filter = Some(steps),
                        _ => {}
                    }
                }
                if vertex_filter.is_some()
                    || edge_filter.is_some()
                    || vertex_property_filter.is_some()
                {
                    self.steps.push(Step::WithStrategy {
                        vertex_filter,
                        edge_filter,
                        vertex_property_filter,
                        check_adjacent_vertices,
                    });
                }
            }
            return;
        }
        // withBulk / withPath / withoutStrategies / with are configuration
        // knobs we don't model — leave them as no-ops.
    }
}
