
    use super::*;
    #[test]
    fn iceberg_hash_and_transform_vectors() {
        assert_eq!(murmur3(&34_i64.to_le_bytes()), 2017239379);
        assert_eq!(murmur3(b"iceberg"), 1210000089);
        assert_eq!(murmur3(&[0, 1, 2, 3]) as i32, -188683207);
        let day = (chrono::NaiveDate::from_ymd_opt(2017, 11, 16).unwrap()
            - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
        .num_days() as i32;
        assert_eq!(murmur3(&i64::from(day).to_le_bytes()) as i32, -653330422);
        assert_eq!(
            transform(
                &ScalarValue::Int32(Some(34)),
                &PartitionTransform::Bucket { buckets: 16 }
            ),
            Some(ScalarValue::Int32(Some(3)))
        );
        assert_eq!(
            transform(
                &ScalarValue::Int64(Some(-1)),
                &PartitionTransform::Truncate { width: 10 }
            ),
            Some(ScalarValue::Int64(Some(-10)))
        );
        assert_eq!(
            transform(
                &ScalarValue::TimestampMicrosecond(Some(-1), None),
                &PartitionTransform::Day
            ),
            Some(ScalarValue::Int32(Some(-1)))
        );
        assert_eq!(
            transform(&ScalarValue::Date32(Some(day)), &PartitionTransform::Month),
            Some(ScalarValue::Int32(Some(574)))
        );
    }

fn layout_fixture() -> (LogicalSource, BTreeMap<String, Arc<dyn TableProvider>>) {
    use arrow::datatypes::{DataType,Field,Schema};
    let schema=Arc::new(Schema::new(vec![Field::new("x",DataType::Int64,true)]));
    let table=Arc::new(datafusion::datasource::MemTable::try_new(schema,vec![vec![]]).unwrap()) as Arc<dyn TableProvider>;
    let source=serde_json::from_value(serde_json::json!({"name":"logical","generation":"v1","default_table":"physical","layouts":[{"table":"physical","generation":"v1","specs":[{"spec_id":0,"fields":[]}],"partitions":[{"spec_id":0,"bytes":100,"files":1}]}]})).unwrap();
    (source,BTreeMap::from([("physical".into(),table)]))
}

#[test]
fn layout_metadata_rejects_invalid_identity_specs_and_bounds() {
    let (valid,providers)=layout_fixture();
    let mut cases=Vec::new();
    let mut s=valid.clone();s.name.clear();cases.push((s,"requires a name"));
    let mut s=valid.clone();s.layouts.clear();cases.push((s,"requires a name"));
    let mut s=valid.clone();s.default_table="missing".into();cases.push((s,"default_table"));
    let mut s=valid.clone();s.layouts[0].generation="old".into();cases.push((s,"stale generation"));
    let mut s=valid.clone();s.layouts.push(s.layouts[0].clone());cases.push((s,"duplicate or recursive"));
    let mut s=valid.clone();s.name="physical".into();cases.push((s,"duplicate or recursive"));
    let mut s=valid.clone();let spec=s.layouts[0].specs[0].clone();s.layouts[0].specs.push(spec);cases.push((s,"duplicate partition spec"));
    let field=PartitionField{source_column:"x".into(),source_id:1,field_id:10,transform:PartitionTransform::Identity};
    let mut s=valid.clone();s.layouts[0].specs[0].fields=vec![field.clone(),field.clone()];cases.push((s,"duplicate partition field"));
    for transform in [PartitionTransform::Bucket{buckets:0},PartitionTransform::Truncate{width:0}] {
        let mut s=valid.clone();s.layouts[0].specs[0].fields=vec![PartitionField{transform,..field.clone()}];cases.push((s,"argument must be positive"));
    }
    let mut s=valid.clone();s.layouts[0].partitions.as_mut().unwrap()[0].spec_id=999;cases.push((s,"unknown spec ID"));
    let mut s=valid.clone();s.layouts[0].partitions.as_mut().unwrap()[0].bounds.insert("x".into(),ColumnBounds{min:Some("20".into()),max:Some("10".into())});cases.push((s,"invalid partition bounds"));
    for (source,expected) in cases {
        assert!(LayoutProvider::try_new(source,&providers).unwrap_err().to_string().contains(expected),"{expected}");
    }
    let provider=LayoutProvider::try_new(valid,&providers).unwrap();
    assert_eq!(provider.table_type(),TableType::Base);
    assert!(format!("{provider:?}").contains("LayoutProvider"));
}

#[test]
fn partition_transforms_preserve_units_unicode_and_overflow_boundaries() {
    use ScalarValue as S;
    use PartitionTransform as T;
    for v in [S::Int32(Some(34)),S::Int64(Some(34)),S::Date32(Some(34))] {
        assert_eq!(transform(&v,&T::Bucket{buckets:16}),Some(S::Int32(Some(3))));
    }
    for v in [S::Utf8(Some("iceberg".into())),S::LargeUtf8(Some("iceberg".into())),S::Binary(Some(b"iceberg".to_vec())),S::LargeBinary(Some(b"iceberg".to_vec()))] {
        assert_eq!(transform(&v,&T::Bucket{buckets:16}),Some(S::Int32(Some(9))));
    }
    let expected=transform(&S::Int64(Some(1_000_000)),&T::Bucket{buckets:31});
    for v in [S::TimestampSecond(Some(1),None),S::TimestampMillisecond(Some(1000),None),S::TimestampMicrosecond(Some(1_000_000),None),S::TimestampNanosecond(Some(1_000_000_000),None)] {
        assert_eq!(transform(&v,&T::Bucket{buckets:31}),expected);
    }
    assert_eq!(transform(&S::Int32(Some(-1)),&T::Truncate{width:10}),Some(S::Int32(Some(-10))));
    assert_eq!(transform(&S::Utf8(Some("😀é字".into())),&T::Truncate{width:2}),Some(S::Utf8(Some("😀é".into()))));
    assert_eq!(transform(&S::Int32(Some(i32::MIN)),&T::Truncate{width:3}),None);
    assert_eq!(transform(&S::Int64(Some(i64::MIN)),&T::Truncate{width:10}),None);
    for v in [S::TimestampSecond(Some(-1),None),S::TimestampMillisecond(Some(-1),None),S::TimestampMicrosecond(Some(-1),None),S::TimestampNanosecond(Some(-1),None)] {
        for t in [T::Year,T::Month,T::Day,T::Hour] {assert_eq!(transform(&v,&t),Some(S::Int32(Some(-1))),"{v:?} {t:?}");}
    }
    for t in [T::Year,T::Month,T::Day,T::Hour,T::Bucket{buckets:2}] {
        assert_eq!(transform(&S::TimestampSecond(Some(i64::MAX),None),&t),None);
    }
    assert_eq!(transform(&S::TimestampMillisecond(Some(i64::MAX),None),&T::Bucket{buckets:2}),None);
    for t in [T::Identity,T::Bucket{buckets:0},T::Truncate{width:0},T::Void,T::Unknown{name:"new".into()}] {
        assert_eq!(transform(&S::Int64(None),&t),None);
    }
    for t in [T::Bucket{buckets:2},T::Truncate{width:2},T::Day,T::Bucket{buckets:0},T::Truncate{width:0},T::Void,T::Unknown{name:"new".into()}] {
        assert_eq!(transform(&S::Boolean(Some(true)),&t),None);
    }
    assert_eq!(transform(&S::Boolean(Some(true)),&T::Identity),Some(S::Boolean(Some(true))));
}

#[test]
fn partition_bounds_prune_only_proven_impossible_predicates() {
    use datafusion::prelude::{col,lit};
    let (source,providers)=layout_fixture();let schema=providers["physical"].schema();
    let mut partition=source.layouts[0].partitions.as_ref().unwrap()[0].clone();
    partition.bounds.insert("x".into(),ColumnBounds{min:Some("10".into()),max:Some("20".into())});
    let x=col("x");
    for (predicate,excluded) in [
        (x.clone().lt(lit(10_i64)),true),(x.clone().lt_eq(lit(10_i64)),false),
        (x.clone().lt_eq(lit(9_i64)),true),(x.clone().gt(lit(20_i64)),true),
        (x.clone().gt_eq(lit(20_i64)),false),(x.clone().gt_eq(lit(21_i64)),true),
        (lit(10_i64).gt(x.clone()),true),(lit(9_i64).gt_eq(x.clone()),true),
        (lit(20_i64).lt(x.clone()),true),(lit(21_i64).lt_eq(x.clone()),true),
        (lit(0_i64).eq(x.clone()),true),(x.clone().not_eq(lit(0_i64)),false),
        (x.clone().lt(lit(10_i64)).and(x.clone().gt(lit(0_i64))),true),
        (x.clone().lt(lit(10_i64)).or(x.clone().gt(lit(0_i64))),false),
        (Expr::IsTrue(Box::new(x.clone().gt(lit(20_i64)))),true),
        (x.clone().in_list(vec![lit(0_i64),lit(30_i64)],false),true),
        (x.clone().in_list(vec![lit(0_i64),lit(10_i64)],false),false),
        (x.clone().in_list(vec![lit(0_i64)],true),false),
        (x.clone().eq(lit(ScalarValue::Int64(None))),false),
        (x.clone().eq(lit(0_i32)),false),(x.clone().eq(x),false),
        (lit(true),false),
    ] {assert_eq!(impossible(&predicate,&partition,&schema,None),excluded,"{predicate}");}
    partition.bounds.insert("missing".into(),ColumnBounds{min:Some("0".into()),max:Some("1".into())});
    assert!(!impossible(&col("missing").eq(lit(5_i64)),&partition,&schema,None));
    partition.bounds.insert("x".into(),ColumnBounds{min:Some("bad".into()),max:None});
    assert!(!impossible(&col("x").eq(lit(5_i64)),&partition,&schema,None));
}

#[test]
fn transformed_partition_bounds_keep_bucket_and_truncation_collisions() {
    use datafusion::prelude::{col,lit};
    let (source,providers)=layout_fixture();let schema=providers["physical"].schema();
    let mut partition=source.layouts[0].partitions.as_ref().unwrap()[0].clone();
    partition.values.insert(10,Some("10".into()));
    let spec=|transform|PartitionSpec{spec_id:0,fields:vec![PartitionField{source_column:"x".into(),source_id:1,field_id:10,transform}]};
    for (transform,predicate,excluded) in [
        (PartitionTransform::Identity,col("x").lt(lit(10_i64)),true),
        (PartitionTransform::Identity,col("x").gt(lit(10_i64)),true),
        (PartitionTransform::Truncate{width:10},col("x").lt(lit(10_i64)),false),
        (PartitionTransform::Truncate{width:10},col("x").lt_eq(lit(9_i64)),true),
        (PartitionTransform::Truncate{width:10},col("x").gt_eq(lit(20_i64)),true),
        (PartitionTransform::Truncate{width:10},col("x").gt(lit(19_i64)),false),
        (PartitionTransform::Bucket{buckets:16},col("x").lt(lit(1_i64)),false),
        (PartitionTransform::Bucket{buckets:16},col("x").gt(lit(1_i64)),false),
    ] {assert_eq!(impossible(&predicate,&partition,&schema,Some(&spec(transform))),excluded,"{predicate}");}
    partition.values.insert(10,Some("not a number".into()));
    assert!(!impossible(&col("x").eq(lit(1_i64)),&partition,&schema,Some(&spec(PartitionTransform::Identity))));
}

#[test]
fn transformed_partition_pruning_is_sound_for_every_member_of_collision_groups() {
    use datafusion::prelude::{col,lit};
    let (source,providers)=layout_fixture();let schema=providers["physical"].schema();
    for transform_kind in [PartitionTransform::Identity,PartitionTransform::Truncate{width:10},PartitionTransform::Bucket{buckets:4}] {
        let mut groups:BTreeMap<String,Vec<i64>>=BTreeMap::new();
        for value in -40..=40 {groups.entry(transform(&ScalarValue::Int64(Some(value)),&transform_kind).unwrap().to_string()).or_default().push(value);}
        let spec=PartitionSpec{spec_id:0,fields:vec![PartitionField{source_column:"x".into(),source_id:1,field_id:10,transform:transform_kind.clone()}]};
        for (key,members) in groups {
            let mut partition=source.layouts[0].partitions.as_ref().unwrap()[0].clone();
            partition.values.insert(10,Some(key));
            for threshold in (-45_i64..=45).step_by(3) {
                for op in [Operator::Eq,Operator::NotEq,Operator::Lt,Operator::LtEq,Operator::Gt,Operator::GtEq] {
                    let expression=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(col("x")),op,Box::new(lit(threshold))));
                    let any_match=members.iter().any(|v|match op {Operator::Eq=>*v==threshold,Operator::NotEq=>*v!=threshold,Operator::Lt=>*v<threshold,Operator::LtEq=>*v<=threshold,Operator::Gt=>*v>threshold,Operator::GtEq=>*v>=threshold,_=>unreachable!()});
                    if impossible(&expression,&partition,&schema,Some(&spec)) {assert!(!any_match,"{transform_kind:?}: {members:?}, {expression}");}
                }
            }
        }
    }
}
