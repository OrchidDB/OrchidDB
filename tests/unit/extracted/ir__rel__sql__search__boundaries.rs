use super::*;
use crate::ir::rel::source_metadata::{IndexMetadata, SourceMetadata};
use datafusion::logical_expr::{col, lit, LogicalPlanBuilder};
use std::{collections::BTreeMap, sync::Arc};

fn candidate() -> RankedJoin {
    let source=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("source_value")]).unwrap().build().unwrap();
    let target=LogicalPlanBuilder::empty(true).project(vec![lit(2_i64).alias("target_value")]).unwrap().build().unwrap();
    RankedJoin {
        schema:target.schema().clone(),source:Arc::new(source),target:Arc::new(target),
        score:lit(0_f64),predicate:None,source_keys:vec![],target_keys:vec![],
        ascending:true,nulls_first:false,limit:0,exact:true,
        index:Some(IndexMetadata {column:"target_value".into(),metric:SearchMetric::L2,options:BTreeMap::new()}),
        source_metadata:Some(SourceMetadata {table:"documents".into(),format:Some("lance".into()),
            options:BTreeMap::from([("uri".into(),serde_json::json!("memory://documents"))]),indexes:vec![]}),
    }
}

#[test]
fn candidate_filters_preserve_prefilter_boundary() {
    let mut search=candidate();
    validate_lance_predicate(&search).unwrap();
    let target=col("target_value");let source=col("source_value");
    let comparisons=vec![target.clone().eq(source.clone()),target.clone().not_eq(source.clone()),
        target.clone().lt(source.clone()),target.clone().lt_eq(source.clone()),
        target.clone().gt(source.clone()),target.clone().gt_eq(source.clone()),
        source.clone().lt(target.clone()),lit(1_i64).eq(target.clone()),
        source.clone().eq(lit(1_i64)),target.clone().is_null(),target.clone().is_not_null(),
        source.clone().is_null(),source.clone().is_not_null(),lit(true)];
    for expression in comparisons {
        for predicate in [expression.clone(),expression.and(target.clone().is_not_null())] {
            search.predicate=Some(predicate.clone());
            assert!(validate_lance_predicate(&search).is_ok(),"{predicate}");
        }
    }
    for predicate in [target.clone().eq(lit(1_i64)).or(target.clone().eq(lit(2_i64))),
        (target.clone()+lit(1_i64)).gt(source.clone()),
        target.clone().eq(col("unbound")),col("unbound").is_null(),
        target.clone().eq(lit(1_i64)).and(col("unbound").is_not_null())] {
        search.predicate=Some(predicate.clone());
        let error=validate_lance_predicate(&search).unwrap_err().to_string();
        assert!(error.contains("final relationship stage"),"{predicate}: {error}");
    }
}

#[test]
fn zero_candidate_limit_keeps_schema_and_validates_search_controls() {
    let mut search=candidate();
    let template=lance_template_inner(&search).unwrap();
    assert_eq!(template.parameters,1);assert_eq!(template.dialect,"duckdb");
    let connection=duckdb::Connection::open_in_memory().unwrap();
    let mut statement=connection.prepare(&template.sql).unwrap();
    assert!(statement.query([]).unwrap().next().unwrap().is_none());
    assert_eq!(statement.column_names(),vec!["target_value"]);
    for option in ["nprobes","refine_factor"] {
        for metadata in [false,true] {
            for value in [serde_json::json!(0),serde_json::json!(-1),serde_json::json!(4294967296_u64),serde_json::json!(1.5),serde_json::json!("1"),serde_json::Value::Null] {
                let mut invalid=search.clone();
                let options=if metadata {&mut invalid.source_metadata.as_mut().unwrap().options}else{&mut invalid.index.as_mut().unwrap().options};
                options.insert(option.into(),value.clone());
                assert!(lance_template_inner(&invalid).err().expect("invalid control must fail").to_string().contains(&format!("{option} must be a positive u32")),"{option}={value}");
            }
            for value in [1_u64,u32::MAX as u64] {
                let mut valid=search.clone();
                let options=if metadata {&mut valid.source_metadata.as_mut().unwrap().options}else{&mut valid.index.as_mut().unwrap().options};
                options.insert(option.into(),serde_json::json!(value));
                lance_template_inner(&valid).unwrap();
            }
        }
        search.source_metadata.as_mut().unwrap().options.insert(option.into(),serde_json::json!(0));
        search.index.as_mut().unwrap().options.insert(option.into(),serde_json::json!(1));
        lance_template_inner(&search).unwrap();
    }
}

#[test]
fn search_templates_reject_incomplete_bindings_and_metric_changes() {
    let base=candidate();
    let mut no_index=base.clone();no_index.index=None;
    assert!(lance_template_inner(&no_index).err().unwrap().to_string().contains("missing Lance index binding"));
    let mut no_metadata=base.clone();no_metadata.source_metadata=None;
    assert!(lance_template_inner(&no_metadata).err().unwrap().to_string().contains("missing Lance source metadata"));
    for uri in [None,Some(serde_json::Value::Null),Some(serde_json::json!("")),Some(serde_json::json!(42))] {
        let mut invalid=base.clone();
        let options=&mut invalid.source_metadata.as_mut().unwrap().options;
        options.remove("uri");if let Some(uri)=uri {options.insert("uri".into(),uri);}
        assert!(lance_template_inner(&invalid).err().unwrap().to_string().contains("requires a dataset uri"));
    }
    for metric in [SearchMetric::Cosine,SearchMetric::Dot] {
        let mut search=base.clone();search.limit=5;
        search.index.as_mut().unwrap().metric=metric;
        assert!(lance_template_inner(&search).err().unwrap().to_string().contains("only L2"));
        // A zero-row search has no metric-dependent retrieval operation.
        search.limit=0;lance_template_inner(&search).unwrap();
    }
}
