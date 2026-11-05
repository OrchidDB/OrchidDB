use crate::ir::rel::{
    layout::{LayoutProvider, TableLayout},
    representation::*,
};
use arrow::{
    array::{Int64Array, StringArray},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use datafusion::{
    catalog::TableProvider,
    datasource::{MemTable, provider_as_source},
    logical_expr::{LogicalPlan, LogicalPlanBuilder, TableProviderFilterPushDown, TableType, lit},
};
use std::{collections::BTreeMap, sync::Arc};

fn fixture() -> (LogicalPlan, BTreeMap<String, Arc<dyn TableProvider>>) {
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("text", DataType::Utf8, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![Some(7), None, Some(7)])),
            Arc::new(StringArray::from(vec!["a", "b", "a"])),
        ],
    )
    .unwrap();
    let table: Arc<dyn TableProvider> =
        Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap());
    let plan = LogicalPlanBuilder::scan("physical", provider_as_source(table.clone()), None)
        .unwrap()
        .build()
        .unwrap();
    (plan, [("physical".into(), table)].into())
}
fn definition() -> RepresentationSource {
    RepresentationSource {
        name: "logical".into(),
        generation: "current".into(),
        default_representation: "primary".into(),
        representations: vec![Representation {
            name: "primary".into(),
            source: RepresentationInput::Table {
                name: "physical".into(),
            },
            generation: "current".into(),
            columns: BTreeMap::new(),
            statistics: vec![],
            average_list_length: None,
        }],
    }
}
fn assert_invalid(
    definition: RepresentationSource,
    plans: Vec<LogicalPlan>,
    providers: &BTreeMap<String, Arc<dyn TableProvider>>,
    expected: &str,
) {
    let error = RepresentationProvider::try_new(definition, plans, providers)
        .unwrap_err()
        .to_string();
    assert!(error.contains(expected), "{error}");
}
fn layout(table: &str) -> TableLayout {
    TableLayout {
        table: table.into(),
        generation: "current".into(),
        snapshot: "snapshot".into(),
        specs: vec![],
        partitions: None,
    }
}

#[test]
fn representation_identity_and_generation_validation_precede_selection() {
    let (plan, providers) = fixture();
    for (mode, expected) in [
        (0, "registered default"),
        (1, "nonempty name"),
        (2, "current default generation"),
        (3, "duplicate or empty representation name"),
        (4, "duplicate or empty representation name"),
    ] {
        let mut definition = definition();
        let mut plans = vec![plan.clone()];
        match mode {
            0 => definition.default_representation = "missing".into(),
            1 => definition.name.clear(),
            2 => definition.representations[0].generation = "old".into(),
            3 => {
                definition.default_representation.clear();
                definition.representations[0].name.clear();
            }
            _ => {
                definition
                    .representations
                    .push(definition.representations[0].clone());
                plans.push(plan.clone());
            }
        }
        assert_invalid(definition, plans, &providers, expected);
    }
}

#[test]
fn representation_expansion_estimates_reject_negative_and_nonfinite_values() {
    let (plan, providers) = fixture();
    for average in [-1.0, f64::NEG_INFINITY, f64::INFINITY, f64::NAN] {
        let mut definition = definition();
        definition.representations[0].average_list_length = Some(average);
        assert_invalid(
            definition,
            vec![plan.clone()],
            &providers,
            "finite and nonnegative",
        );
    }
    for average in [0.0, 0.25, 2.5, f64::MAX] {
        let mut definition = definition();
        definition.representations[0].average_list_length = Some(average);
        assert!(
            RepresentationProvider::try_new(definition, vec![plan.clone()], &providers).is_ok()
        );
    }
}

#[test]
fn representation_column_mappings_reject_missing_and_empty_output_names() {
    let (plan, providers) = fixture();
    for (output, source, expected) in [
        ("", "id", "empty representation output column"),
        (
            "canonical",
            "missing",
            "missing representation column missing",
        ),
    ] {
        let mut definition = definition();
        definition.representations[0]
            .columns
            .insert(output.into(), source.into());
        assert_invalid(definition, vec![plan.clone()], &providers, expected);
    }
}

#[test]
fn representation_alternatives_require_identical_canonical_names_and_types() {
    let (plan, providers) = fixture();
    for (expressions, expected) in [
        (vec![lit(7_i64).alias("id")], "same canonical columns"),
        (
            vec![lit(7_i64).alias("other"), lit("a").alias("text")],
            "missing canonical column id",
        ),
        (
            vec![lit("7").alias("id"), lit("a").alias("text")],
            "type mismatch for id",
        ),
    ] {
        let mut definition = definition();
        let mut other = definition.representations[0].clone();
        other.name = "other".into();
        definition.representations.push(other);
        let other = LogicalPlanBuilder::empty(true)
            .project(expressions)
            .unwrap()
            .build()
            .unwrap();
        assert_invalid(definition, vec![plan.clone(), other], &providers, expected);
    }
}

#[test]
fn representation_statistics_must_belong_to_exactly_one_scanned_source() {
    let (plan, mut providers) = fixture();
    let mut duplicate = definition();
    duplicate.representations[0].statistics = vec![layout("physical"), layout("physical")];
    assert_invalid(
        duplicate,
        vec![plan.clone()],
        &providers,
        "duplicate representation scan statistics",
    );
    providers.insert("unused".into(), providers["physical"].clone());
    let mut unused = definition();
    unused.representations[0].statistics = vec![layout("unused")];
    assert_invalid(
        unused,
        vec![plan],
        &providers,
        "not scanned by its definition",
    );
    let wrapped: Arc<dyn TableProvider> =
        Arc::new(LayoutProvider::for_statistics(layout("physical"), &providers).unwrap());
    let wrapped_plan = LogicalPlanBuilder::scan("physical", provider_as_source(wrapped), None)
        .unwrap()
        .build()
        .unwrap();
    let mut repeated = definition();
    repeated.representations[0].statistics = vec![layout("physical")];
    assert_invalid(
        repeated,
        vec![wrapped_plan],
        &providers,
        "not again on its representation",
    );
}

#[test]
fn representation_definitions_reject_explain_and_analyze_statements() {
    let (plan, providers) = fixture();
    for analyze in [false, true] {
        let statement = LogicalPlanBuilder::from(plan.clone())
            .explain(false, analyze)
            .unwrap()
            .build()
            .unwrap();
        assert_invalid(
            definition(),
            vec![statement],
            &providers,
            "read-only queries",
        );
    }
}

#[tokio::test]
async fn representation_selection_preserves_projected_column_order_duplicates_and_nulls() {
    use datafusion::common::ScalarValue;
    let (plan, providers) = fixture();
    let representation: Arc<dyn TableProvider> =
        Arc::new(RepresentationProvider::try_new(definition(), vec![plan], &providers).unwrap());
    assert_eq!(representation.table_type(), TableType::View);
    let predicate = lit(true);
    assert_eq!(
        representation
            .supports_filters_pushdown(&[&predicate])
            .unwrap(),
        vec![TableProviderFilterPushDown::Inexact]
    );
    for projection in [vec![1, 0], vec![0]] {
        let plan = LogicalPlanBuilder::scan(
            "logical",
            provider_as_source(representation.clone()),
            Some(projection.clone()),
        )
        .unwrap()
        .build()
        .unwrap();
        let selected = select(plan).unwrap();
        assert_eq!(selected.representation_selections.len(), 1);
        assert_eq!(
            selected.representation_selections[0].representation,
            "primary"
        );
        let batches = crate::ir::rel::execute_test_plan(selected.plan)
            .await
            .unwrap();
        let mut rows = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows()).map(move |row| {
                    (0..batch.num_columns())
                        .map(|column| {
                            ScalarValue::try_from_array(batch.column(column), row).unwrap()
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut expected = [(Some(7), "a"), (None, "b"), (Some(7), "a")]
            .into_iter()
            .map(|(id, text)| {
                let values = [ScalarValue::Int64(id), ScalarValue::Utf8(Some(text.into()))];
                projection
                    .iter()
                    .map(|index| values[*index].clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.partial_cmp(right).unwrap());
        expected.sort_by(|left, right| left.partial_cmp(right).unwrap());
        assert_eq!(rows, expected);
    }
}

#[test]
fn representation_column_alignment_merges_nullability_without_coercing_types() {
    let make_plan = |name: &str, fields: Vec<Field>| {
        LogicalPlanBuilder::scan(
            name,
            provider_as_source(Arc::new(datafusion::datasource::empty::EmptyTable::new(
                Arc::new(Schema::new(fields)),
            ))),
            None,
        )
        .unwrap()
        .build()
        .unwrap()
    };
    let primary = make_plan(
        "primary_rows",
        vec![
            Field::new("id", DataType::Int64, false),
            Field::new("text", DataType::Utf8, false),
        ],
    );
    let alternate = make_plan(
        "other_rows",
        vec![
            Field::new("raw_text", DataType::Utf8, true),
            Field::new("raw_id", DataType::Int64, true),
        ],
    );
    let mut definition = definition();
    let mut other = definition.representations[0].clone();
    other.name = "other".into();
    other.source = RepresentationInput::Table {
        name: "other_rows".into(),
    };
    other.columns = [
        ("id".into(), "raw_id".into()),
        ("text".into(), "raw_text".into()),
    ]
    .into();
    definition.representations.push(other);
    let representation =
        RepresentationProvider::try_new(definition, vec![primary, alternate], &BTreeMap::new())
            .unwrap();
    assert_eq!(
        representation.schema().as_ref(),
        &Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("text", DataType::Utf8, true)
        ])
    );
    let source: Arc<dyn TableProvider> = Arc::new(representation);
    let scan = LogicalPlanBuilder::scan("logical", provider_as_source(source), None)
        .unwrap()
        .build()
        .unwrap();
    let selected = select(scan).unwrap();
    assert_eq!(
        selected
            .plan
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().as_str())
            .collect::<Vec<_>>(),
        ["id", "text"]
    );
    assert_eq!(
        selected.representation_selections[0].representation,
        "primary"
    );
    assert!(
        selected.representation_selections[0]
            .candidates
            .iter()
            .all(|candidate| candidate.estimate.estimated_cost.is_none())
    );
}

#[tokio::test]
async fn constrained_representation_selection_preserves_predicates_and_projection() {
    use crate::ir::rel::constraints::{Constraint, ConstraintCatalog, Fact};
    use datafusion::{common::ScalarValue, prelude::col};
    let (plan, providers) = fixture();
    let representation: Arc<dyn TableProvider> = Arc::new(
        RepresentationProvider::try_new(definition(), vec![plan], &providers).unwrap());
    let mut catalog = ConstraintCatalog::default();
    catalog.insert("logical", Constraint::enforced("known_text", Fact::NonNull{columns:vec!["text".into()]}));
    let wrapped = crate::ir::rel::constraints::bind("logical",representation,&catalog,None).unwrap();
    assert!(provider(&wrapped).is_some());
    assert!(provider(providers.get("physical").unwrap()).is_none());
    let plan = LogicalPlanBuilder::scan_with_filters("logical",provider_as_source(wrapped),Some(vec![1,0]),
        vec![col("id").is_not_null(),col("text").eq(lit("a"))]).unwrap().build().unwrap();
    let selected = select(plan).unwrap();
    assert_eq!(selected.representation_selections.len(),1);
    assert_eq!(selected.representation_selections[0].predicates.len(),2);
    let batches = crate::ir::rel::execute_test_plan(selected.plan).await.unwrap();
    let rows = batches.iter().flat_map(|batch|(0..batch.num_rows()).map(move |row|{
        (ScalarValue::try_from_array(batch.column(0),row).unwrap(),ScalarValue::try_from_array(batch.column(1),row).unwrap())
    })).collect::<Vec<_>>();
    assert_eq!(rows,vec![(ScalarValue::Utf8(Some("a".into())),ScalarValue::Int64(Some(7)));2]);
}

#[test]
fn stale_representation_candidates_never_replace_a_current_default_without_statistics() {
    let (plan, providers) = fixture();
    let mut definition = definition();
    let mut stale = definition.representations[0].clone();
    stale.name="stale".into(); stale.generation="old".into();
    definition.representations.insert(0,stale);
    let source: Arc<dyn TableProvider> = Arc::new(
        RepresentationProvider::try_new(definition,vec![plan.clone(),plan],&providers).unwrap());
    let input = LogicalPlanBuilder::scan("logical",provider_as_source(source),None).unwrap().build().unwrap();
    let selected = select(input).unwrap();
    let decision = &selected.representation_selections[0];
    assert_eq!(decision.representation,"primary");
    assert_eq!(decision.candidates.len(),2);
    assert_eq!(decision.candidates[0].reason,"stale generation");
    assert!(decision.candidates[0].estimate.estimated_cost.is_none());
    assert_eq!(decision.candidates[1].reason,"selected default; unknown default cost");
}
