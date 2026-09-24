//! Graph binding column layout and projections.

use super::*;

pub(super) fn col_exact(name: impl Into<String>) -> Expr {
    Expr::Column(Column::new_unqualified(name.into()))
}

pub(crate) fn id_col(binding: &str) -> String {
    format!("{binding}{ID_SUFFIX}")
}

pub(super) fn label_col(binding: &str) -> String {
    format!("{binding}{LABEL_SUFFIX}")
}

pub(super) fn src_id_col(binding: &str) -> String {
    format!("{binding}{SRC_ID_SUFFIX}")
}

pub(super) fn src_label_col(binding: &str) -> String {
    format!("{binding}{SRC_LABEL_SUFFIX}")
}

pub(super) fn dst_id_col(binding: &str) -> String {
    format!("{binding}{DST_ID_SUFFIX}")
}

pub(super) fn dst_label_col(binding: &str) -> String {
    format!("{binding}{DST_LABEL_SUFFIX}")
}

pub(super) fn prop_col(binding: &str, property: &str) -> String {
    format!("{binding}{PROP_MARKER}{property}")
}

pub(super) fn union_tag_col(binding: &str, property: &str) -> String {
    format!("{}__w_union_tag", prop_col(binding, property))
}

pub(super) fn struct_field_col(binding: &str, property: &str, field: &str) -> String {
    format!("{}__w_struct__{field}", prop_col(binding, property))
}

pub(super) fn output_fields(plan: &LogicalPlan) -> Vec<String> {
    plan.schema()
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect()
}

pub(super) fn has_exact_col(plan: &LogicalPlan, name: &str) -> bool {
    plan.schema()
        .fields()
        .iter()
        .any(|field| field.name() == name)
}

pub(super) fn resolve_column_name(plan: &LogicalPlan, name: &str) -> Option<String> {
    if has_exact_col(plan, name) {
        return Some(name.to_string());
    }
    let mut matches = plan
        .schema()
        .fields()
        .iter()
        .filter(|field| field.name().eq_ignore_ascii_case(name))
        .map(|field| field.name().to_string());
    let column = matches.next()?;
    matches.next().is_none().then_some(column)
}

pub(super) fn has_binding_shape(plan: &LogicalPlan, binding: &str) -> Option<BindingShape> {
    if has_exact_col(plan, &id_col(binding)) && has_exact_col(plan, &label_col(binding)) {
        if has_exact_col(plan, &src_id_col(binding)) && has_exact_col(plan, &dst_id_col(binding)) {
            Some(BindingShape::Edge)
        } else {
            Some(BindingShape::Node)
        }
    } else {
        None
    }
}

pub(super) fn projection_aliases(items: &[ProjectionItem]) -> BTreeSet<String> {
    let mut aliases = BTreeSet::new();
    for item in items {
        aliases.insert(item.alias.clone());
        aliases.insert(id_col(&item.alias));
        aliases.insert(label_col(&item.alias));
        aliases.insert(src_id_col(&item.alias));
        aliases.insert(src_label_col(&item.alias));
        aliases.insert(dst_id_col(&item.alias));
        aliases.insert(dst_label_col(&item.alias));
    }
    aliases
}

pub(super) fn existing_columns(plan: &LogicalPlan, excluded: &BTreeSet<String>) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .map(|field| col_exact(field.name()))
        .collect()
}

pub(super) fn existing_columns_by_name(plan: &LogicalPlan, excluded: &BTreeSet<String>) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .map(|field| col_exact(field.name()))
        .collect()
}

pub(super) fn apply_correlation_key_columns(plan: &LogicalPlan) -> Vec<String> {
    plan.schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| name.starts_with("__apply_corr_key_"))
        .collect()
}

pub(super) fn existing_columns_excluding_binding(
    plan: &LogicalPlan,
    binding: &str,
    excluded: &BTreeSet<String>,
) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| !excluded.contains(field.name()))
        .filter(|field| !is_binding_column(field.name(), binding))
        .map(|field| col_exact(field.name()))
        .collect()
}

pub(super) fn existing_columns_excluding_bindings(plan: &LogicalPlan, bindings: &[&str]) -> Vec<Expr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| {
            !bindings
                .iter()
                .any(|binding| is_binding_column(field.name(), binding))
        })
        .map(|field| col_exact(field.name()))
        .collect()
}

pub(super) fn path_len_col(binding: &str) -> String {
    format!("{binding}{PATH_LEN_SUFFIX}")
}

/// A variable-length path rendered without its endpoint nodes: the value of
/// the relationship variable `e` in `-[e*]->`, as opposed to a named path.
pub(super) fn path_inner_col(binding: &str) -> String {
    format!("{binding}{PATH_INNER_SUFFIX}")
}

pub(super) fn is_binding_column(name: &str, binding: &str) -> bool {
    name == binding
        || name == id_col(binding)
        || name == label_col(binding)
        || name == src_id_col(binding)
        || name == src_label_col(binding)
        || name == dst_id_col(binding)
        || name == dst_label_col(binding)
        || name == path_len_col(binding)
        || name == path_inner_col(binding)
        || name.starts_with(&format!("{binding}{PROP_MARKER}"))
}

pub(super) fn duplicate_binding_projection(plan: &LogicalPlan, from: &str, to: &str) -> RelResult<Vec<Expr>> {
    let mut projections = existing_columns(plan, &BTreeSet::new());
    projections.extend(duplicate_binding_projection_only(plan, from, to)?);
    Ok(projections)
}

pub(super) fn duplicate_binding_projection_only(
    plan: &LogicalPlan,
    from: &str,
    to: &str,
) -> RelResult<Vec<Expr>> {
    let Some(shape) = has_binding_shape(plan, from) else {
        return Err(RelError::Unsupported(format!(
            "binding `{from}` is not an element binding"
        )));
    };
    let mut projections = vec![
        col_exact(id_col(from)).alias(id_col(to)),
        col_exact(label_col(from)).alias(label_col(to)),
    ];
    if shape == BindingShape::Edge {
        projections.extend([
            col_exact(src_label_col(from)).alias(src_label_col(to)),
            col_exact(src_id_col(from)).alias(src_id_col(to)),
            col_exact(dst_label_col(from)).alias(dst_label_col(to)),
            col_exact(dst_id_col(from)).alias(dst_id_col(to)),
        ]);
    }
    let prefix = format!("{from}{PROP_MARKER}");
    for field in plan.schema().fields() {
        let name = field.name();
        if let Some(property) = name.strip_prefix(&prefix) {
            projections.push(col_exact(name).alias(prop_col(to, property)));
        }
    }
    Ok(projections)
}

pub(super) fn binding_pair_eq(binding: &str, id_column: &str, label_column: &str) -> Expr {
    Expr::and(
        binary(
            col_exact(id_col(binding)),
            BinaryOp::Eq,
            col_exact(id_column),
        ),
        binary(
            col_exact(label_col(binding)),
            BinaryOp::Eq,
            col_exact(label_column),
        ),
    )
}

/// Columns produced by a `x.*` projection expansion for `field`, in plan
/// (schema) order.
pub(super) fn star_expansion_columns(plan: &LogicalPlan, field: &str) -> Option<Vec<Expr>> {
    let prefix = format!("{field}{STAR_SEP}");
    let cols = plan
        .schema()
        .fields()
        .iter()
        .filter(|schema_field| schema_field.name().starts_with(prefix.as_str()))
        .map(|schema_field| col_exact(schema_field.name()).alias(schema_field.name()))
        .collect::<Vec<_>>();
    (!cols.is_empty()).then_some(cols)
}

pub(super) fn plan_column_type(plan: &LogicalPlan, name: &str) -> Option<DataType> {
    plan.schema()
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .map(|field| field.data_type().clone())
}
