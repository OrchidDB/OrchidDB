//! Stable textual Graph IR rendering.

use super::*;

/// Pretty-printer that mirrors the `EXPLAIN` style used in the design doc.
/// Each operator on its own line with two-space indentation per depth.
pub fn explain(plan: &GraphPlan) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "Policy: {:?}", plan.policy).ok();
    writeln!(out).ok();
    write_node(&mut out, &plan.root, 0);
    out
}

fn pad(buf: &mut String, depth: usize) {
    for _ in 0..depth {
        buf.push_str("  ");
    }
}

fn write_node(buf: &mut String, node: &Node, depth: usize) {
    use std::fmt::Write;
    pad(buf, depth);
    match node {
        Node::GraphReturn {
            fields,
            result_form,
            input,
        } => {
            writeln!(
                buf,
                "GraphReturn(fields=[{}], resultForm=[{:?}])",
                fields.join(", "),
                result_form
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphNodeScan {
            graph,
            binding,
            labels,
        } => {
            writeln!(
                buf,
                "GraphNodeScan(graph=[{graph}], bind=[{binding}], labels=[{labels:?}])"
            )
            .ok();
        }
        Node::GraphRelScan {
            graph,
            binding,
            types,
            dir,
        } => {
            writeln!(
                buf,
                "GraphRelScan(graph=[{graph}], bind=[{binding}], types=[{types:?}], dir=[{dir:?}])"
            )
            .ok();
        }
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => {
            writeln!(
                buf,
                "GraphValues(bindings=[{}], rows={}, bulk={})",
                bindings.join(", "),
                rows.len(),
                bulk.is_some()
            )
            .ok();
        }
        Node::GraphOneRow => {
            writeln!(buf, "GraphOneRow()").ok();
        }
        Node::GraphEmpty => {
            writeln!(buf, "GraphEmpty()").ok();
        }
        Node::GraphCorrelate { bindings } => {
            writeln!(buf, "GraphCorrelate(bindings=[{}])", bindings.join(", ")).ok();
        }
        Node::GraphBind {
            bind, kind, input, ..
        } => {
            writeln!(buf, "GraphBind(bind=[{bind}], kind=[{kind:?}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphExpand {
            source,
            target,
            target_mode,
            rel_types,
            dir,
            length,
            history,
            path,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphExpand(source=[{source}], target=[{target}], mode=[{target_mode:?}], types=[{rel_types:?}], dir=[{dir:?}], length=[{}..{}], history=[{}], path=[{}])",
                length.min,
                length.max_display(),
                history.as_deref().unwrap_or("-"),
                path.as_deref().unwrap_or("-")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphPathPattern {
            path,
            selector,
            endpoints,
            parts,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphPathPattern(path=[{path}], selector=[{selector:?}], endpoints=[{}], parts={})",
                endpoints.join(", "),
                parts.len()
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphRepeat {
            times,
            emit,
            seed,
            body,
            ..
        } => {
            let times_str = match times {
                Some(n) => format!("{n}"),
                None => "Unbounded".to_string(),
            };
            writeln!(buf, "GraphRepeat(times=[{times_str}], emit=[{emit:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "seed:").ok();
            write_node(buf, seed, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "body:").ok();
            write_node(buf, body, depth + 2);
        }
        Node::GraphPathFilter { input, scope, .. } => {
            writeln!(buf, "GraphPathFilter(scope=[{scope:?}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCreate {
            graph,
            nodes,
            edges,
            input,
        } => {
            let specs = nodes
                .iter()
                .map(|node| match &node.bind {
                    Some(bind) => format!("{bind}:{}", node.label),
                    None => format!(":{}", node.label),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let edge_specs = edges
                .iter()
                .map(|edge| {
                    let bind = edge.bind.as_deref().unwrap_or("");
                    format!("({})-[{bind}:{}]->({})", edge.src, edge.rel_type, edge.dst)
                })
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                buf,
                "GraphCreate(graph=[{graph}], nodes=[{specs}], edges=[{edge_specs}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphMerge {
            correlation,
            outputs,
            input,
            match_arm,
            create_arm,
        } => {
            writeln!(
                buf,
                "GraphMerge(correlation=[{}], outputs=[{}])",
                correlation.join(", "),
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
            write_node(buf, match_arm, depth + 1);
            write_node(buf, create_arm, depth + 1);
        }
        Node::GraphSetProperty { items, input } => {
            let specs = items
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(buf, "GraphSetProperty(keys=[{specs}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDelete {
            targets,
            detach,
            input,
        } => {
            writeln!(
                buf,
                "GraphDelete(targets=[{}], detach=[{detach}])",
                targets.len()
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphFilter { input, .. } => {
            writeln!(buf, "GraphFilter").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphProject {
            mode, items, input, ..
        } => {
            let names = items
                .iter()
                .map(|item| item.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(buf, "GraphProject(mode=[{mode:?}], fields=[{names}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphJvm { operation, input } => {
            writeln!(buf, "GraphJvm(mode=[{:?}], output=[{}], effect=[graph])", operation.mode, operation.output).ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCurrentProject { input, fields, .. } => {
            writeln!(buf, "GraphCurrentProject(fields=[{}])", fields.join(", ")).ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            let group_names = group
                .iter()
                .map(|item| item.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let agg_names = aggs
                .iter()
                .map(|agg| agg.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                buf,
                "GraphAggregate(group=[{group_names}], aggs=[{agg_names}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphGroupMap { output, input, .. } => {
            writeln!(buf, "GraphGroupMap(output=[{output}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphGroupSideEffect { label, input, .. } => {
            writeln!(buf, "GraphGroupSideEffect(label=[{label}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphGroupCountSideEffect { label, input, .. } => {
            writeln!(buf, "GraphGroupCountSideEffect(label=[{label}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSideEffect { label, reducer, input, .. } => {
            writeln!(buf, "GraphSideEffect(label=[{label}], reducer=[{reducer}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphReadSideEffect { label, input } => {
            writeln!(buf, "GraphReadSideEffect(label=[{label}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCap { labels, input } => {
            writeln!(buf, "GraphCap(labels=[{}])", labels.join(", ")).ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphShortestPath {
            source,
            target,
            direction,
            output,
            all_paths,
            max_distance,
            include_edges,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphShortestPath(source=[{source}], target=[{target:?}], dir=[{direction:?}], output=[{output}], all_paths=[{all_paths}], max_distance=[{max_distance:?}], include_edges=[{include_edges}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDistinct {
            keys,
            mode,
            bulk,
            input,
        } => {
            writeln!(
                buf,
                "GraphDistinct(keys=[{}], mode=[{mode:?}], bulk=[{bulk:?}])",
                keys.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSort { input, .. } => {
            writeln!(buf, "GraphSort").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSample { kind, seed, step_id, weight, input } => {
            writeln!(buf, "GraphSample(kind=[{kind:?}], seed=[{seed:?}], step=[{step_id}], weight=[{weight:?}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSlice { slice, input, .. } => {
            writeln!(
                buf,
                "GraphSlice(offset=[{}], fetch=[{:?}], tail=[{:?}])",
                slice.offset, slice.fetch, slice.tail
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => {
            writeln!(
                buf,
                "GraphSliceExpr(offset=[{}], fetch=[{}])",
                offset
                    .as_ref()
                    .map(|expr| format!("{expr:?}"))
                    .unwrap_or_else(|| "None".to_string()),
                fetch
                    .as_ref()
                    .map(|expr| format!("{expr:?}"))
                    .unwrap_or_else(|| "None".to_string())
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphBarrier {
            partition,
            slice,
            materialize,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphBarrier(partition=[{}], slice=[off={},fetch={:?},tail={:?}], materialize=[{materialize}])",
                partition.join(", "),
                slice.offset,
                slice.fetch,
                slice.tail
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphJoin {
            kind, left, right, ..
        } => {
            writeln!(buf, "GraphJoin(kind=[{kind:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            left,
            right,
            ..
        } => {
            writeln!(
                buf,
                "GraphApply(kind=[{kind:?}], correlation=[{}], outputs=[{}])",
                correlation.join(", "),
                outputs.join(", ")
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphUnion {
            all,
            align,
            left,
            right,
        } => {
            writeln!(buf, "GraphUnion(all=[{all}], align=[{align:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphUnwind {
            bind, outer, input, ..
        } => {
            writeln!(buf, "GraphUnwind(bind=[{bind}], outer=[{outer}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphQuantifier {
            kind,
            output,
            input,
            ..
        } => {
            writeln!(buf, "GraphQuantifier(kind=[{kind:?}], output=[{output}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCollect {
            alias,
            distinct,
            input,
            ..
        } => {
            writeln!(buf, "GraphCollect(alias=[{alias}], distinct=[{distinct}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCoalesce {
            output,
            arms,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphCoalesce(output=[{output}], arms=[{}])",
                arms.len()
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "input:").ok();
            write_node(buf, input, depth + 2);
            for (idx, arm) in arms.iter().enumerate() {
                pad(buf, depth + 1);
                writeln!(buf, "arm{idx}:").ok();
                write_node(buf, arm, depth + 2);
            }
        }
        Node::GraphChoose {
            output,
            input,
            arms,
            default,
            unmatched,
            ..
        } => {
            writeln!(
                buf,
                "GraphChoose(output=[{output}], arms=[{}], unmatched=[{unmatched:?}])",
                arms.len()
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "input:").ok();
            write_node(buf, input, depth + 2);
            for (idx, arm) in arms.iter().enumerate() {
                pad(buf, depth + 1);
                writeln!(buf, "arm{idx}:").ok();
                write_node(buf, &arm.body, depth + 2);
            }
            if let Some(default) = default {
                pad(buf, depth + 1);
                writeln!(buf, "default:").ok();
                write_node(buf, default, depth + 2);
            }
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            writeln!(
                buf,
                "GraphSelect(labels=[{}], output=[{}])",
                labels.join(", "),
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphProcedureCall {
            name,
            yields,
            mode,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphProcedureCall(name=[{name}], yields=[{}], mode=[{mode:?}])",
                yields.join(", ")
            )
            .ok();
            if let Some(input) = input {
                write_node(buf, input, depth + 1);
            }
        }
        Node::GraphExtension { name, inputs, .. } => {
            writeln!(
                buf,
                "GraphExtension(name=[{name}], inputs=[{}])",
                inputs.len()
            )
            .ok();
            for input in inputs {
                write_node(buf, input, depth + 1);
            }
        }
        Node::GraphSparqlTriplePattern {
            dataset,
            graph_scope,
            subject,
            predicate,
            object,
            outputs,
        } => {
            writeln!(
                buf,
                "GraphSparqlTriplePattern(dataset=[{dataset}], graphScope=[{graph_scope:?}], subject=[{subject:?}], predicate=[{predicate:?}], object=[{object:?}], outputs=[{}])",
                outputs.join(", ")
            )
            .ok();
        }
        Node::GraphRdfPropertyPath {
            dataset,
            graph_scope,
            subject,
            object,
            path,
            path_materialization,
            zero_length,
        } => {
            writeln!(
                buf,
                "GraphRdfPropertyPath(dataset=[{dataset}], graphScope=[{graph_scope:?}], subject=[{subject:?}], object=[{object:?}], path=[{path:?}], pathMaterialization=[{path_materialization:?}], zeroLength=[{zero_length:?}])"
            )
            .ok();
        }
        Node::GraphSparqlMinus {
            compatible,
            shared,
            left,
            right,
        } => {
            writeln!(
                buf,
                "GraphSparqlMinus(compatible=[{compatible:?}], shared=[{}])",
                shared.join(", ")
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphService {
            endpoint,
            silent,
            outputs,
            input,
        } => {
            writeln!(
                buf,
                "GraphService(endpoint=[{endpoint:?}], silent=[{silent}], outputs=[{}])",
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphConstructTriples { template, input } => {
            writeln!(
                buf,
                "GraphConstructTriples(template=[{} triple{}])",
                template.len(),
                if template.len() == 1 { "" } else { "s" }
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDescribe { terms, input } => {
            writeln!(
                buf,
                "GraphDescribe(terms=[{} term{}])",
                terms.len(),
                if terms.len() == 1 { "" } else { "s" }
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphAsk { field, input } => {
            writeln!(buf, "GraphAsk(field=[{field}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphListComprehension {
            item, alias, input, ..
        } => {
            writeln!(
                buf,
                "GraphListComprehension(item=[{item}], alias=[{alias}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
    }
}
