use super::*;
use crate::ir::{ElementId, functions::domain};

fn id(value: ScalarValue) -> ElementId {
    ElementId::new(value).unwrap()
}

#[test]
fn constant_entity_expressions_preserve_typed_keys_endpoints_and_optional_nulls() {
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None,
        graph: &graph,
        options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(),
        language: Language::Cypher,
        scan_counter: 0,
        correlate_plan: None,
        rdf_typed_terms_used: false,
        scan_logical_labels: false,
        gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    };
    for edge in [false, true] {
        for missing in [false, true] {
            let label = if edge { "KNOWS" } else { "Person" };
            let mut columns = vec![
                lit(label).alias(label_col("item")),
                lit(ScalarValue::Int64((!missing).then_some(i64::MAX))).alias(id_col("item")),
            ];
            if edge {
                columns.extend([
                    lit("Person").alias(src_label_col("item")),
                    lit("source-1").alias(src_id_col("item")),
                    lit("Company").alias(dst_label_col("item")),
                    lit(-7_i64).alias(dst_id_col("item")),
                ]);
            }
            let bindings = columns
                .iter()
                .map(|expr| {
                    let Expr::Alias(alias) = expr else {
                        panic!("fixture alias missing")
                    };
                    (alias.name.clone(), alias.expr.as_ref().clone())
                })
                .collect::<BTreeMap<_, _>>();
            let input = LogicalPlanBuilder::empty(true)
                .project(columns)
                .unwrap()
                .build()
                .unwrap();
            let value = context.lower_element_value(&input, "item").unwrap();
            // Evaluate the generated immutable expression with the compiler's
            // scalar simplifier. Graph domains retain Arrow envelopes; they do
            // not claim an engine SQL type mapping at this boundary.
            use datafusion::common::{
                DFSchema,
                tree_node::{Transformed, TreeNode},
            };
            use datafusion::logical_expr::simplify::SimplifyContext;
            use datafusion::optimizer::simplify_expressions::ExprSimplifier;
            let value = value
                .transform_up(|expr| match expr {
                    Expr::Column(column) => Ok(Transformed::yes(bindings[&column.name].clone())),
                    other => Ok(Transformed::no(other)),
                })
                .unwrap()
                .data;
            let schema = DFSchema::empty();
            let simplifier = ExprSimplifier::new(
                SimplifyContext::default().with_schema(Arc::new(schema.clone())),
            );
            let Expr::Literal(value, _) = simplifier
                .simplify(simplifier.coerce(value, &schema).unwrap())
                .unwrap()
            else {
                panic!("constant graph expression did not simplify")
            };
            let array = value.to_array_of_size(1).unwrap();
            let expected = if missing {
                Value::Null
            } else if edge {
                Value::Edge {
                    rel_type: label.into(),
                    id: id(ScalarValue::Int64(Some(i64::MAX))),
                    src_label: "Person".into(),
                    src_id: id(ScalarValue::Utf8(Some("source-1".into()))),
                    dst_label: "Company".into(),
                    dst_id: id(ScalarValue::Int64(Some(-7))),
                    projected_properties: None,
                }
            } else {
                Value::Node {
                    label: label.into(),
                    id: id(ScalarValue::Int64(Some(i64::MAX))),
                }
            };
            assert!(is_type(array.data_type()));
            assert_eq!(decode(array.as_ref(), 0), Some(expected));
        }
    }
    let empty = LogicalPlanBuilder::empty(true).build().unwrap();
    assert!(
        context
            .lower_element_value(&empty, "missing")
            .unwrap_err()
            .to_string()
            .contains("expected graph binding")
    );
}

fn carrier(name: &str, values: Vec<(&str, ScalarValue)>) -> arrow::array::ArrayRef {
    let fields = values
        .iter()
        .map(|(name, value)| Field::new(*name, value.data_type(), true))
        .collect::<Vec<_>>();
    let arrays = values
        .into_iter()
        .map(|(_, value)| value.to_array_of_size(1).unwrap())
        .collect();
    let storage = ScalarValue::Struct(Arc::new(StructArray::new(fields.into(), arrays, None)));
    domain::scalar(name, storage)
        .unwrap()
        .to_array_of_size(1)
        .unwrap()
}

#[test]
fn entity_decoder_rejects_unrelated_domains_and_malformed_identity_fields() {
    let plain = ScalarValue::Int64(Some(1)).to_array_of_size(1).unwrap();
    assert_eq!(decode(plain.as_ref(), 0), None);
    let wrong_storage = domain::scalar("graph_node", ScalarValue::Int64(Some(1)))
        .unwrap()
        .to_array_of_size(1)
        .unwrap();
    assert_eq!(decode(wrong_storage.as_ref(), 0), None);
    for (domain_name, fields) in [
        ("json", vec![("id", ScalarValue::Int64(Some(1)))]),
        (
            "graph_node",
            vec![("label", ScalarValue::Utf8(Some("Person".into())))],
        ),
        (
            "graph_node",
            vec![
                ("id", ScalarValue::Int64(Some(1))),
                ("label", ScalarValue::Int64(Some(7))),
            ],
        ),
        (
            "graph_edge",
            vec![
                ("id", ScalarValue::Int64(Some(1))),
                ("label", ScalarValue::Utf8(Some("KNOWS".into()))),
            ],
        ),
    ] {
        assert_eq!(decode(carrier(domain_name, fields).as_ref(), 0), None);
    }
    let null_id = carrier(
        "graph_node",
        vec![
            ("id", ScalarValue::Int64(None)),
            ("label", ScalarValue::Utf8(Some("Person".into()))),
        ],
    );
    assert_eq!(decode(null_id.as_ref(), 0), Some(Value::Null));
}
