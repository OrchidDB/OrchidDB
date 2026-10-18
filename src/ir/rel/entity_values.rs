//! Lossless nested graph references. Display strings are never entity carriers.
use super::*;
use arrow::array::{Array, StructArray};

pub(crate) fn is_type(ty: &DataType) -> bool {
    crate::ir::functions::domain::descriptor(ty).is_some_and(|(name,_)|
        matches!(name,"graph_node" | "graph_edge"))
}
impl LoweringContext<'_> {
    pub(super) fn lower_element_value(&self, plan: &LogicalPlan, binding: &str) -> RelResult<Expr> {
        let shape=has_binding_shape(plan,binding).ok_or_else(||RelError::Unsupported("expected graph binding".into()))?;
        let mut fields=vec![lit("label"),col_exact(label_col(binding)),lit("id"),col_exact(id_col(binding))];
        let domain=if shape==BindingShape::Edge {
            fields.extend([lit("src_label"),col_exact(src_label_col(binding)),lit("src_id"),col_exact(src_id_col(binding)),
                lit("dst_label"),col_exact(dst_label_col(binding)),lit("dst_id"),col_exact(dst_id_col(binding))]);
            "graph_edge"
        } else {"graph_node"};
        let value=df_core::named_struct(vec![lit(format!("__orchiddb_domain_{domain}")),df_core::named_struct(fields)]);
        // A missing optional binding is SQL NULL, not a nonnull struct whose
        // fields happen to be NULL. collect() must omit it.
        Ok(Expr::Case(Case::new(None,vec![(Box::new(col_exact(id_col(binding)).is_not_null()),Box::new(value))],None)))
    }
}
pub(crate) fn decode(array: &dyn Array, row: usize) -> Option<Value> {
    let (kind,_) = crate::ir::functions::domain::descriptor(array.data_type())?;
    if !is_type(array.data_type()) {return None;}
    let outer=array.as_any().downcast_ref::<StructArray>()?;
    if outer.is_null(row) {return Some(Value::Null);}
    let fields=outer.column(0).as_any().downcast_ref::<StructArray>()?;
    if fields.is_null(row) || fields.column_by_name("id")?.is_null(row) {return Some(Value::Null);}
    let text=|name: &str| match crate::ir::catalog::array_value(fields.column_by_name(name)?.as_ref(),row,None) {
        Value::String(s)=>Some(s), _=>None,
    };
    let key=|name: &str| crate::ir::ElementId::new(ScalarValue::try_from_array(fields.column_by_name(name)?,row).ok()?).ok();
    Some(if kind=="graph_edge" {
        Value::Edge {rel_type:text("label")?,id:key("id")?,src_label:text("src_label")?,src_id:key("src_id")?,
            dst_label:text("dst_label")?,dst_id:key("dst_id")?,projected_properties:None}
    } else {Value::Node {label:text("label")?,id:key("id")?}})
}
