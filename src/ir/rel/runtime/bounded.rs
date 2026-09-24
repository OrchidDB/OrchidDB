use super::*;
use crate::ir::plan::Slice;
impl Compiler<'_> {
    pub(in crate::ir::rel::runtime) fn lower_bounded(
        &self,
        input: &Node,
        slice: &Slice,
    ) -> Result<Option<LogicalPlan>> {
        let Some(fetch) = slice.fetch.filter(|_| slice.tail.is_none()) else {
            return Ok(None);
        };
        let mut source = input.clone();
        let mut stages = Vec::new();
        let mut lazy = false;
        while let Some(upstream) = ops::stream::take_stream_input(&mut source) {
            lazy |= matches!(source, Node::GraphSideEffect { eager: false, .. });
            stages.push(source);
            source = *upstream;
        }
        if !lazy {
            return Ok(None);
        }
        stages.reverse();
        let stages = stages
            .iter()
            .map(|s| Ok((self.subplan(s)?, ops::stream::range_movable(s))))
            .collect::<Result<Vec<_>>>()?;
        let slice = slice.clone();
        Ok(Some(kernel(
            "BoundedConsume",
            vec![self.lower(&source)?],
            move |mut inputs, state| {
                let rows = inputs.remove(0);
                let graph = &state.graph;
                let ctx = &mut state.context;
                // EarlyLimitStrategy moves a range before bound scalar maps and
                // side-effect steps. At a filter/flat-map boundary RangeGlobalStep
                // requests one additional traverser before recognizing its high bound.
                let boundary = stages
                    .iter()
                    .rposition(|stage| !stage.1)
                    .map_or(0, |index| index + 1);
                let plans: Vec<_> = stages.iter().map(|s| s.0.clone()).collect();
                let (prefix, suffix) = plans.split_at(boundary);
                let high = slice.offset.saturating_add(fetch);
                let mut consumer = Consumer {
                    remaining: if fetch == 0 {
                        0
                    } else {
                        high.saturating_add(1)
                    },
                    position: 0,
                    offset: slice.offset,
                    high,
                    suffix,
                    output: Vec::new(),
                };
                for row in rows {
                    for _ in 0..row.bulk {
                        if consumer.remaining == 0 {
                            break;
                        }
                        let mut single = row.clone();
                        single.bulk = 1;
                        consume(single, prefix, &mut consumer, graph, ctx)?;
                    }
                    if consumer.remaining == 0 {
                        break;
                    }
                }
                Ok(consumer.output)
            },
        )))
    }
}
struct Consumer<'a> {
    remaining: u64,
    position: u64,
    offset: u64,
    high: u64,
    suffix: &'a [Subplan],
    output: Vec<Row>,
}

fn consume(
    row: Row,
    stages: &[Subplan],
    consumer: &mut Consumer<'_>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<()> {
    if consumer.remaining == 0 {
        return Ok(());
    }
    let Some((stage, rest)) = stages.split_first() else {
        let mut row = row;
        row.bulk = row.bulk.min(consumer.remaining);
        consumer.remaining -= row.bulk;
        let position = consumer.position;
        consumer.position = consumer.position.saturating_add(row.bulk);
        let mut selected = ops::slice::slice_op(
            &Slice {
                offset: consumer.offset.saturating_sub(position),
                fetch: Some(consumer.high.saturating_sub(position.max(consumer.offset))),
                tail: None,
            },
            vec![row],
        )?;
        for stage in consumer.suffix {
            if selected.is_empty() {
                break;
            }
            selected = run_body_with_frontier(stage, selected, graph, ctx)?;
        }
        consumer.output.extend(selected);
        return Ok(());
    };
    for row in run_body_with_frontier(stage, vec![row], graph, ctx)? {
        consume(row, rest, consumer, graph, ctx)?;
        if consumer.remaining == 0 {
            break;
        }
    }
    Ok(())
}
