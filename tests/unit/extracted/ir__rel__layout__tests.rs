
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
