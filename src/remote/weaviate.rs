use super::*;

pub(super) fn validate_search(search: &RankedJoin) -> Result<()> {
    let metric = search
        .metric()
        .ok_or("Weaviate requires a vector scoring primitive")?;
    if !matches!(
        metric,
        SearchMetric::Cosine | SearchMetric::Dot | SearchMetric::L2
    ) {
        return Err(
            "Weaviate lowering supports cosine similarity, dot product, and L2 distance".into(),
        );
    }
    if search.exact {
        return Err("Weaviate ranked retrieval requires approximate_allowed; exact ranking must execute on an exact backend".into());
    }
    if search.ascending != (metric == SearchMetric::L2) || search.nulls_first {
        return Err("Weaviate requires nearest-first ranking with nulls last".into());
    }
    if search.index.is_none() {
        return Err("Weaviate retrieval requires an explicit vector index binding".into());
    }
    Ok(())
}

pub(super) fn score_filters(
    filters: &mut Vec<Expr>,
    search: &RankedJoin,
    target: &Scan,
    build: &mut Build,
) -> Result<Vec<Value>> {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    let score = search
        .score
        .clone()
        .transform_up(|expr| {
            if let Expr::Column(column) = &expr {
                if search.target.schema().has_column(column) {
                    return resolve(&expr, target)
                        .map(Transformed::yes)
                        .map_err(datafusion::common::DataFusionError::Plan);
                }
            }
            Ok(Transformed::no(expr))
        })
        .map_err(|error| error.to_string())?
        .data
        .unalias();
    let mut remaining = Vec::new();
    let mut thresholds = Vec::new();
    for filter in std::mem::take(filters) {
        let Expr::BinaryExpr(comparison) = &filter else {
            remaining.push(filter);
            continue;
        };
        let (value, op) = if comparison.left.as_ref().clone().unalias() == score {
            (comparison.right.as_ref(), comparison.op)
        } else if comparison.right.as_ref().clone().unalias() == score {
            (
                comparison.left.as_ref(),
                match comparison.op {
                    Operator::Gt => Operator::Lt,
                    Operator::GtEq => Operator::LtEq,
                    Operator::Lt => Operator::Gt,
                    Operator::LtEq => Operator::GtEq,
                    other => other,
                },
            )
        } else {
            remaining.push(filter);
            continue;
        };
        let op =
            match (search.metric(), op) {
                (Some(SearchMetric::L2), Operator::Lt) => "lt",
                (Some(SearchMetric::L2), Operator::LtEq) => "lte",
                (Some(SearchMetric::Cosine | SearchMetric::Dot), Operator::Gt) => "gt",
                (Some(SearchMetric::Cosine | SearchMetric::Dot), Operator::GtEq) => "gte",
                _ => return Err(
                    "Weaviate cannot push this score predicate before nearest-neighbor selection"
                        .into(),
                ),
            };
        let value = build.value(
            value,
            format!("/body/score_filters/{}/value", thresholds.len()),
        )?;
        thresholds.push(json!({"op":op,"value":value}));
    }
    *filters = remaining;
    Ok(thresholds)
}
