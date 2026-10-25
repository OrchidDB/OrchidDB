use arrow::{
    array::{Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use futures::StreamExt;
use orchiddb::{
    execution::stream::{BatchSource, BatchStream, InputRelation},
    federation::Session,
    ir::rel::sql::DuckDbSession,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[tokio::test]
async fn arrow_input_and_output_are_streamed_and_typed() {
    let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    let shape = schema.clone();
    let source = BatchSource::once(BatchStream::new(
        schema,
        futures::stream::iter(0..10).map(move |n| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(RecordBatch::try_new(
                shape.clone(),
                vec![Arc::new(Int64Array::from(vec![n; 2048]))],
            )
            .unwrap())
        }),
    ));
    let mut session = DuckDbSession::default();
    let mut out = session
        .query_stream(
            "SELECT n + 1 AS n FROM input",
            vec![InputRelation {
                name: "input".into(),
                source,
            }],
        )
        .await
        .unwrap();
    let first = out.next().await.unwrap().unwrap();
    assert!(first.num_rows() <= 2048);
    let mut rows = first.num_rows();
    while let Some(batch) = out.next().await {
        rows += batch.unwrap().num_rows();
    }
    assert_eq!(rows, 20480);
    assert_eq!(polls.load(Ordering::SeqCst), 10);
}
#[tokio::test]
async fn errors_after_first_input_batch_are_not_truncated() {
    let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from(vec![1; 2048]))],
    )
    .unwrap();
    let source = BatchSource::once(BatchStream::new(
        schema,
        futures::stream::iter([Ok(batch), Err("input broke".into())]),
    ));
    let mut session = DuckDbSession::default();
    let result = session
        .query_stream(
            "SELECT * FROM input",
            vec![InputRelation {
                name: "input".into(),
                source,
            }],
        )
        .await;
    let error = match result {
        Err(e) => e,
        Ok(s) => s.collect_batches().await.unwrap_err(),
    };
    assert!(error.contains("input broke"), "{error}");
}

#[tokio::test]
async fn limit_cancels_unconsumed_input_and_session_remains_usable() {
    let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let polls = Arc::new(AtomicUsize::new(0));
    let count = polls.clone();
    let shape = schema.clone();
    let source = BatchSource::once(BatchStream::new(
        schema,
        futures::stream::iter(0..100_000).map(move |n| {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(RecordBatch::try_new(
                shape.clone(),
                vec![Arc::new(Int64Array::from(vec![n; 2048]))],
            )
            .unwrap())
        }),
    ));
    let mut session = DuckDbSession::default();
    let result = session
        .query_stream(
            "SELECT n FROM input LIMIT 1",
            vec![InputRelation {
                name: "input".into(),
                source,
            }],
        )
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(result.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
    assert!(
        polls.load(Ordering::SeqCst) < 100,
        "LIMIT should stop upstream reads"
    );
    let next = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        session.query_stream("SELECT 7 AS n", vec![]),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(next.collect_batches().await.unwrap()[0].num_rows(), 1);
}

#[tokio::test]
async fn dependent_queries_start_only_when_output_is_requested() {
    let calls = Arc::new(AtomicUsize::new(0));
    let requests = calls.clone();
    let queries = Box::pin(futures::stream::iter(0..100).map(move |n| {
        requests.fetch_add(1, Ordering::SeqCst);
        Ok(format!("SELECT {n}::BIGINT AS id"))
    }));
    let mut session = DuckDbSession::default();
    let mut output = session
        .query_requests_stream(
            queries,
            vec![orchiddb::federation::TransferColumn {
                name: "id".into(),
                data_type: "int64".into(),
                nullable: false,
            }],
        )
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(output.next().await.unwrap().unwrap().num_rows(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(output);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn nested_map_union_preserves_members_offsets_and_nulls() {
    use arrow::{
        array::{Array, ArrayRef, MapArray, StringArray, StructArray, UnionArray},
        buffer::{OffsetBuffer, ScalarBuffer},
        datatypes::UnionFields,
    };
    for sparse in [false, true] {
        let fields = UnionFields::try_new(
            [3, 7],
            [
                Field::new("number", DataType::Int64, true),
                Field::new("text", DataType::Utf8, true),
            ],
        )
        .unwrap();
        let children: Vec<ArrayRef> = if sparse {
            vec![
                Arc::new(Int64Array::from(vec![Some(99), Some(42), None, None])),
                Arc::new(StringArray::from(vec![None, None, Some("hello"), None])),
            ]
        } else {
            vec![
                Arc::new(Int64Array::from(vec![Some(99), Some(42), None])),
                Arc::new(StringArray::from(vec!["hello"])),
            ]
        };
        let values = UnionArray::try_new(
            fields,
            ScalarBuffer::from(vec![3i8, 3, 7, 3]),
            (!sparse).then(|| ScalarBuffer::from(vec![0i32, 1, 0, 2])),
            children,
        )
        .unwrap();
        let values = values.slice(1, 3);
        let entries = StructArray::from(vec![
            (
                Arc::new(Field::new("key", DataType::Utf8, false)),
                Arc::new(StringArray::from(vec!["a", "b", "c"])) as ArrayRef,
            ),
            (
                Arc::new(Field::new("value", values.data_type().clone(), true)),
                Arc::new(values) as ArrayRef,
            ),
        ]);
        let map = MapArray::new(
            Arc::new(Field::new("entries", entries.data_type().clone(), false)),
            OffsetBuffer::new(ScalarBuffer::from(vec![0i32, 2, 3])),
            entries,
            None,
            false,
        );
        let schema = Arc::new(Schema::new(vec![Field::new(
            "m",
            map.data_type().clone(),
            false,
        )]));
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(map)]).unwrap();
        let mut session = DuckDbSession::default();
        let batches = session.query_stream("SELECT union_extract(m['a'], 'number') AS n, union_extract(m['b'], 'text') AS s, union_extract(m['c'], 'number') AS nullable FROM input", vec![InputRelation {name:"input".into(),source:BatchSource::once(BatchStream::new(schema, futures::stream::iter([Ok(batch)])))}]).await.unwrap().collect_batches().await.unwrap();
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 2);
        let batch = &batches[0];
        assert_eq!(
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(0),
            42
        );
        assert_eq!(
            batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(0),
            "hello"
        );
        assert!(batch.column(0).is_null(1));
        assert!(batch.column(1).is_null(1));
        assert!(batch.column(2).is_null(1));
    }
}
