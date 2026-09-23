//! Cast lowerings whose relational form has to be proven against the
//! interpreter before it is used.
//!
//! List values cross the relational boundary as the interpreter's display
//! text (`[1,9]`), so a list cast is a rewrite of that text rather than a
//! typed SQL cast. A rewrite is only emitted after checking, for every value
//! the property actually stores, that it produces exactly the text the
//! interpreter's cast renders; anything else declines.

use std::collections::BTreeSet;

use arrow::datatypes::DataType;
use datafusion::functions::regex::expr_fn as df_regex;
use datafusion::logical_expr::{Expr, LogicalPlan};
use datafusion::prelude::lit;

use crate::ir::expr::IrExpr;
use crate::ir::interpreter::{Row as InterpreterRow, eval as interpreter_eval};
use crate::ir::value::Value;

use super::{
    LoweringContext, PROP_MARKER, RelError, RelResult, col_exact, element_property_value,
    expr_is_constant, has_exact_col, literal_collection_context, plan_column_type,
    rel_display_value, tagged_value, union_tag_of,
};

/// How a list cast changes the list's display text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ListTextRewrite {
    /// The target element type prints every stored element identically.
    Identity,
    /// Integer elements gain the six-decimal float rendering (`9` becomes
    /// `9.000000`); every other element prints unchanged.
    IntegersToFloat,
}

/// Binding name the verification evaluates the cast against.
const CAST_SUBJECT: &str = "\u{0}rel_list_cast_subject";

impl LoweringContext<'_> {
    /// Lower `CAST(value, "T[]")` (and the function spellings of it).
    pub(super) fn lower_list_cast(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
        target: &str,
    ) -> RelResult<Expr> {
        let element = target
            .trim()
            .trim_matches('"')
            .trim()
            .trim_end_matches("[]")
            .trim()
            .to_ascii_uppercase();
        let rewrite = if matches!(
            element.as_str(),
            "DOUBLE" | "FLOAT" | "FLOAT4" | "FLOAT8" | "FLOAT32" | "FLOAT64" | "REAL"
        ) {
            ListTextRewrite::IntegersToFloat
        } else {
            ListTextRewrite::Identity
        };
        let Some(source) = args.first() else {
            return Err(RelError::Unsupported("list cast arity".into()));
        };
        match source {
            IrExpr::Property { name: property, .. } => {
                self.verify_stored_list_cast(property, name, args, rewrite)?;
            }
            // Only the text of a stored property can be checked ahead of
            // time. A dynamic list that already prints the way its cast does
            // is safe for the identity rewrite; a float rewrite is not.
            _ if rewrite == ListTextRewrite::Identity => {}
            _ => {
                return Err(RelError::Unsupported(format!(
                    "cast of a dynamic list to `{target}`"
                )));
            }
        }
        let value = self.lower_expr(plan, source)?;
        Ok(match rewrite {
            ListTextRewrite::Identity => value,
            ListTextRewrite::IntegersToFloat => integer_items_as_float_expr(value),
        })
    }

    /// Check the rewrite against the interpreter's cast for every distinct
    /// value stored under `property` on any node or relationship table.
    ///
    /// Checking every table that has the key is a superset of what the
    /// binding can reach, so a mismatch anywhere declines — never the other
    /// way round.
    fn verify_stored_list_cast(
        &self,
        property: &str,
        name: &str,
        args: &[IrExpr],
        rewrite: ListTextRewrite,
    ) -> RelResult<()> {
        let mut cast_args = args.to_vec();
        cast_args[0] = IrExpr::Binding(CAST_SUBJECT.to_string());
        let cast = IrExpr::Call {
            name: name.to_string(),
            args: cast_args,
        };
        let context = literal_collection_context(self.language);
        let mut seen = BTreeSet::new();
        let mut check = |value: Value| -> RelResult<()> {
            if matches!(value, Value::Null) || !seen.insert(tagged_value(&value)) {
                return Ok(());
            }
            let row = InterpreterRow::new().with(CAST_SUBJECT, value.clone());
            let casted = interpreter_eval(&cast, &row, self.graph).map_err(|err| {
                RelError::Unsupported(format!("list cast of `{property}`: {err}"))
            })?;
            let stored = rel_display_value(&value, self.language, context);
            let expected = match casted {
                Value::Null => {
                    return Err(RelError::Unsupported(format!(
                        "list cast of `{property}` yields null for a stored value"
                    )));
                }
                other => rel_display_value(&other, self.language, context),
            };
            let produced = match rewrite {
                ListTextRewrite::Identity => stored,
                ListTextRewrite::IntegersToFloat => integer_items_as_float_text(&stored),
            };
            if produced == expected {
                Ok(())
            } else {
                Err(RelError::Unsupported(format!(
                    "list cast of `{property}` does not preserve its display text"
                )))
            }
        };
        for label in self.graph.node_label_order() {
            if !self
                .graph
                .node_property_keys_with_id(label)
                .iter()
                .any(|key| key == property)
            {
                continue;
            }
            for id in self.graph.node_ids(label).unwrap_or_default() {
                check(element_property_value(
                    self.graph, false, label, id, property,
                ))?;
            }
        }
        for rel_type in self.graph.edge_rel_order() {
            if !self
                .graph
                .edge_property_keys(rel_type)
                .iter()
                .any(|key| key == property)
            {
                continue;
            }
            for id in self.graph.edge_ids(rel_type) {
                check(element_property_value(
                    self.graph, true, rel_type, id, property,
                ))?;
            }
        }
        Ok(())
    }
}

/// Column carrying the union tag of a union-valued projection `binding`.
///
/// A union prints as its payload alone, so the display column cannot answer
/// `union_tag(q)`. The tag travels beside it the way struct fields and stored
/// union tags do; the property marker keeps it out of the island's decoded
/// bindings.
pub(super) fn value_union_tag_col(binding: &str) -> String {
    format!("{binding}{PROP_MARKER}__w_value_union_tag")
}

impl LoweringContext<'_> {
    /// The union tag(s) to carry beside a projected value, if it is a union.
    ///
    /// A constant union yields its tag, a constant list of unions a list of
    /// tags (so `union_tag(q[i])` can subscript it), and a binding that
    /// already carries tags passes them on.
    pub(super) fn projected_union_tags(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        if let IrExpr::Binding(binding) = expr {
            let column = value_union_tag_col(binding);
            return Ok(has_exact_col(plan, &column).then(|| col_exact(column)));
        }
        if !matches!(expr, IrExpr::Call { .. } | IrExpr::List(_)) || !expr_is_constant(expr, &[]) {
            return Ok(None);
        }
        let Ok(value) = interpreter_eval(expr, &InterpreterRow::new(), self.graph) else {
            return Ok(None);
        };
        Ok(match &value {
            Value::Map(_) => union_tag_of(&value).map(lit),
            Value::List(items) if !items.is_empty() => items
                .iter()
                .map(|item| match item {
                    Value::Null => Some(lit(datafusion::common::ScalarValue::Utf8(None))),
                    other => union_tag_of(other).map(lit),
                })
                .collect::<Option<Vec<_>>>()
                .map(datafusion::functions_nested::expr_fn::make_array),
            _ => None,
        })
    }

    /// `union_tag(q)` / `union_tag(q[i])` over a projected union binding.
    pub(super) fn lower_value_union_tag(
        &self,
        plan: &LogicalPlan,
        arg: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        match arg {
            IrExpr::Binding(binding) => {
                let column = value_union_tag_col(binding);
                Ok(has_exact_col(plan, &column).then(|| col_exact(column)))
            }
            IrExpr::Call { name, args }
                if matches!(
                    name.as_str(),
                    "cypher_subscript" | "list_extract" | "list_element" | "element_at"
                ) =>
            {
                let [IrExpr::Binding(binding), index] = args.as_slice() else {
                    return Ok(None);
                };
                let column = value_union_tag_col(binding);
                if !matches!(
                    plan_column_type(plan, &column),
                    Some(
                        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                    )
                ) {
                    return Ok(None);
                }
                self.lower_cypher_subscript(plan, &IrExpr::Binding(column), index)
                    .map(Some)
            }
            _ => Ok(None),
        }
    }
}

/// Rewrite every integer list item in display text to six-decimal float
/// text. An item is the text between a `[`/`,` and the next `,`/`]`.
///
/// A regex match consumes the delimiter on both sides of an item, so in one
/// global pass an item directly after a rewritten item is skipped. A second
/// pass reaches exactly those: each skipped item's neighbours were
/// rewritten, so their delimiters are free again. [`integer_items_as_float_text`]
/// is the same function over a Rust string and is what the rewrite is
/// verified with.
fn integer_items_as_float_expr(text: Expr) -> Expr {
    let pass = |value: Expr| {
        df_regex::regexp_replace(
            value,
            lit(r"([\[,])(-?[0-9]+)([,\]])"),
            lit(r"\1\2.000000\3"),
            Some(lit("g")),
        )
    };
    pass(pass(text))
}

fn integer_items_as_float_text(text: &str) -> String {
    fn push_item(out: &mut String, item: &str, before: Option<char>, after: Option<char>) {
        out.push_str(item);
        let digits = item.strip_prefix('-').unwrap_or(item);
        if matches!(before, Some('[' | ','))
            && matches!(after, Some(',' | ']'))
            && !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            out.push_str(".000000");
        }
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut item = String::new();
    let mut before = None;
    for ch in text.chars() {
        if matches!(ch, '[' | ',' | ']') {
            push_item(&mut out, &item, before, Some(ch));
            item.clear();
            out.push(ch);
            before = Some(ch);
        } else {
            item.push(ch);
        }
    }
    push_item(&mut out, &item, before, None);
    out
}

#[cfg(test)]
mod tests {
    use super::integer_items_as_float_text;

    #[test]
    fn rewrites_integer_items_only() {
        assert_eq!(integer_items_as_float_text("[1,9]"), "[1.000000,9.000000]");
        assert_eq!(
            integer_items_as_float_text("[10,11,12]"),
            "[10.000000,11.000000,12.000000]"
        );
        assert_eq!(integer_items_as_float_text("[-3]"), "[-3.000000]");
        assert_eq!(
            integer_items_as_float_text("[1.500000,2]"),
            "[1.500000,2.000000]"
        );
        assert_eq!(integer_items_as_float_text("[]"), "[]");
        assert_eq!(integer_items_as_float_text("[,]"), "[,]");
        assert_eq!(integer_items_as_float_text("[a1,2b]"), "[a1,2b]");
        assert_eq!(
            integer_items_as_float_text("[[1,2],[3]]"),
            "[[1.000000,2.000000],[3.000000]]"
        );
    }
}
