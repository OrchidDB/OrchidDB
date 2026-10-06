//! Resolve scoring corpora from mapped vertex provenance, not candidate rows.
use super::*;
use datafusion::common::Column;
use datafusion::logical_expr::Subquery;

fn lineage_error() -> RelError {
    RelError::Unsupported(BM25_CORPUS_ERROR.into())
}

/// Follow only identity projections. Filters, joins and limits do not define
/// the corpus; the original mapped source (including authorization) does.
fn origins(
    plan: &LogicalPlan,
    expr: &Expr,
    found: &mut BTreeSet<(String, String)>,
) -> RelResult<()> {
    let Expr::Column(column) = expr else {
        return Err(lineage_error());
    };
    match plan {
        LogicalPlan::Projection(p) => {
            let index = p.schema.index_of_column(column)?;
            let field = p.schema.field(index).name();
            if let Some((binding, property)) = field.split_once(super::super::PROP_MARKER) {
                if matches!(
                    super::super::has_binding_shape(plan, binding),
                    Some(super::super::BindingShape::Edge)
                ) {
                    return Err(lineage_error());
                }
                if let Ok(label_index) = p
                    .schema
                    .index_of_column(&Column::new_unqualified(label_col(binding)))
                {
                    if let Expr::Literal(ScalarValue::Utf8(Some(label)), _) =
                        p.expr[label_index].clone().unalias()
                    {
                        found.insert((label, property.to_owned()));
                        return Ok(());
                    }
                }
            }
            origins(&p.input, &p.expr[index].clone().unalias(), found)
        }
        LogicalPlan::SubqueryAlias(a) => {
            let index = a.schema.index_of_column(column)?;
            origins(
                &a.input,
                &Expr::Column(a.input.schema().qualified_field(index).into()),
                found,
            )
        }
        LogicalPlan::Union(u) => {
            let index = u.schema.index_of_column(column)?;
            for input in &u.inputs {
                origins(
                    input,
                    &Expr::Column(input.schema().qualified_field(index).into()),
                    found,
                )?;
            }
            Ok(())
        }
        LogicalPlan::Aggregate(a) => {
            let index = a.schema.index_of_column(column)?;
            if let Some(group) = a.group_expr.get(index) {
                return origins(&a.input, &group.clone().unalias(), found);
            }
            let representative = a.aggr_expr[index - a.group_expr.len()].clone().unalias();
            if let Expr::AggregateFunction(call) = representative
                && call.func.name() == "first_value"
                && call.params.args.len() == 1
            {
                return origins(&a.input, &call.params.args[0], found);
            }
            Err(lineage_error())
        }
        LogicalPlan::Distinct(datafusion::logical_expr::Distinct::On(d)) => {
            let index = d.schema.index_of_column(column)?;
            origins(&d.input, &d.select_expr[index].clone().unalias(), found)
        }
        // Only operators retaining input columns can preserve provenance.
        LogicalPlan::Filter(_)
        | LogicalPlan::Sort(_)
        | LogicalPlan::Limit(_)
        | LogicalPlan::Join(_)
        | LogicalPlan::Distinct(_)
        | LogicalPlan::Repartition(_)
        | LogicalPlan::Window(_) => {
            let inputs = plan
                .inputs()
                .into_iter()
                .filter(|input| input.schema().index_of_column(column).is_ok())
                .collect::<Vec<_>>();
            if inputs.len() != 1 {
                return Err(lineage_error());
            }
            origins(inputs[0], expr, found)
        }
        _ => Err(lineage_error()),
    }
}

impl GraphMapping {
    pub(in crate::ir::rel) fn bm25_corpus(
        &self,
        plan: &LogicalPlan,
        document: &Expr,
    ) -> RelResult<Expr> {
        let mut sources = BTreeSet::new();
        origins(plan, document, &mut sources)?;
        let mut branches = Vec::new();
        for (label, property) in sources {
            let node = self.node(&label).ok_or_else(lineage_error)?;
            if !node.properties.contains_key(&property) {
                return Err(lineage_error());
            }
            let source = computed::endpoint(self, node, "corpus")?;
            branches.push(
                LogicalPlanBuilder::from(source)
                    .project(vec![
                        col_exact(computed::prop("corpus", &property)).alias("document"),
                    ])?
                    .build()?,
            );
        }
        if branches.is_empty() {
            return Err(lineage_error());
        }
        let corpus =
            computed::corpus(union_all(branches)?, col_exact("document"), "__bm25_corpus")?;
        Ok(Expr::ScalarSubquery(Subquery {
            subquery: Arc::new(corpus),
            outer_ref_columns: Vec::new(),
            spans: Default::default(),
        }))
    }
}
