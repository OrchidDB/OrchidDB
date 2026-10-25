//! Engine-neutral, owned Arrow streams. No query engine or physical operators.
use arrow::{datatypes::SchemaRef, record_batch::RecordBatch};
use futures::{Stream, StreamExt};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

pub type BatchResult = Result<RecordBatch, String>;

/// A schema-known stream. Dropping it drops its producer, including any driver's
/// cancellation guard. Producers must bound prefetch independently of row count.
pub struct BatchStream {
    schema: SchemaRef,
    inner: Pin<Box<dyn Stream<Item = BatchResult> + Send + 'static>>,
    failed: bool,
}
impl std::fmt::Debug for BatchStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchStream")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}
impl BatchStream {
    pub fn new(
        schema: SchemaRef,
        stream: impl Stream<Item = BatchResult> + Send + 'static,
    ) -> Self {
        Self {
            schema,
            inner: Box::pin(stream),
            failed: false,
        }
    }
    pub fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    /// Terminal collection for APIs explicitly returning all results. Never use
    /// this to connect two execution stages.
    pub async fn collect_batches(mut self) -> Result<Vec<RecordBatch>, String> {
        let mut batches = Vec::new();
        while let Some(batch) = self.next().await {
            batches.push(batch?);
        }
        Ok(batches)
    }
}
impl Stream for BatchStream {
    type Item = BatchResult;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.failed {
            return Poll::Ready(None);
        }
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(batch))) => {
                let error = if batch.schema() != self.schema {
                    Some("engine stream schema changed".to_owned())
                } else if batch
                    .columns()
                    .iter()
                    .zip(self.schema.fields())
                    .any(|(array, field)| !field.is_nullable() && array.null_count() != 0)
                {
                    Some("NULL in non-nullable engine stream column".to_owned())
                } else {
                    None
                };
                if let Some(error) = error {
                    self.failed = true;
                    self.inner = Box::pin(futures::stream::empty());
                    Poll::Ready(Some(Err(error)))
                } else {
                    Poll::Ready(Some(Ok(batch)))
                }
            }
            Poll::Ready(Some(Err(error))) => {
                self.failed = true;
                self.inner = Box::pin(futures::stream::empty());
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.failed = true;
                self.inner = Box::pin(futures::stream::empty());
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

type Factory = dyn Fn() -> Result<BatchStream, String> + Send + Sync + 'static;
enum SourceKind {
    Once(Mutex<Option<BatchStream>>),
    Replayable(Box<Factory>),
}
struct Source {
    schema: SchemaRef,
    kind: SourceKind,
}
/// Clones share ownership. A one-shot input is not accidentally rerun by a join,
/// a second scan, or a distributed retry. Replay requires an explicit factory.
#[derive(Clone)]
pub struct BatchSource(Arc<Source>, Option<u64>);
impl std::fmt::Debug for BatchSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BatchSource")
            .field("schema", &self.0.schema)
            .field("replayable", &self.is_replayable())
            .finish()
    }
}
impl BatchSource {
    pub fn once(stream: BatchStream) -> Self {
        Self(
            Arc::new(Source {
                schema: stream.schema(),
                kind: SourceKind::Once(Mutex::new(Some(stream))),
            }),
            None,
        )
    }
    pub fn replayable(
        schema: SchemaRef,
        factory: impl Fn() -> Result<BatchStream, String> + Send + Sync + 'static,
    ) -> Self {
        Self(
            Arc::new(Source {
                schema,
                kind: SourceKind::Replayable(Box::new(factory)),
            }),
            None,
        )
    }
    pub fn schema(&self) -> SchemaRef {
        self.0.schema.clone()
    }
    pub fn with_estimated_rows(mut self, rows: u64) -> Self {
        self.1 = Some(rows);
        self
    }
    pub fn estimated_rows(&self) -> Option<u64> {
        self.1
    }
    pub fn is_replayable(&self) -> bool {
        matches!(self.0.kind, SourceKind::Replayable(_))
    }
    pub fn open(&self) -> Result<BatchStream, String> {
        let stream = match &self.0.kind {
            SourceKind::Once(stream) => stream.lock().map_err(|_| "stream lease lock poisoned")?.take().ok_or("one-shot input was already consumed; explicitly spool it or provide a replayable source")?,
            SourceKind::Replayable(factory) => factory()?,
        };
        if stream.schema() != self.0.schema {
            return Err("replayed stream schema differs from registered schema".into());
        }
        Ok(stream)
    }

    /// Explicit fan-out/retry boundary: spool to an IPC file, never an unbounded
    /// Vec of batches. The file is removed when its last source/reader is dropped.
    pub async fn spool(
        mut stream: BatchStream,
        directory: &std::path::Path,
    ) -> Result<Self, String> {
        use std::{
            fs::OpenOptions,
            sync::atomic::{AtomicU64, Ordering},
        };
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let path = directory.join(format!(
            "orchid-exchange-{}-{}-{}.arrow",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)] {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&path).map_err(|e| e.to_string())?;
        let lease = Arc::new(SpoolFile(path));
        let schema = stream.schema();
        let mut writer =
            arrow::ipc::writer::FileWriter::try_new(file, &schema).map_err(|e| e.to_string())?;
        let mut rows = 0u64;
        while let Some(batch) = stream.next().await {
            let batch = batch?;
            rows = rows
                .checked_add(batch.num_rows() as u64)
                .ok_or("exchange row count overflow")?;
            writer.write(&batch).map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
        drop(writer);
        Ok(Self::replayable(schema.clone(), move || {
            let file = std::fs::File::open(&lease.0).map_err(|e| e.to_string())?;
            let reader =
                arrow::ipc::reader::FileReader::try_new(file, None).map_err(|e| e.to_string())?;
            let guard = lease.clone();
            Ok(BatchStream::new(
                schema.clone(),
                futures::stream::iter(reader.map(move |batch| {
                    let _keep_alive = &guard;
                    batch.map_err(|e| e.to_string())
                })),
            ))
        })
        .with_estimated_rows(rows))
    }
}
struct SpoolFile(std::path::PathBuf);
impl Drop for SpoolFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[derive(Clone, Debug)]
pub struct InputRelation {
    pub name: String,
    pub source: BatchSource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::{
        array::Int64Array,
        datatypes::{DataType, Field, Schema},
    };
    fn batch() -> RecordBatch {
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
            vec![Arc::new(Int64Array::from(vec![1, 2]))],
        )
        .unwrap()
    }
    #[tokio::test]
    async fn one_shot_leases_do_not_reissue_reads() {
        let batch = batch();
        let source = BatchSource::once(BatchStream::new(
            batch.schema(),
            futures::stream::iter([Ok(batch)]),
        ));
        let copy = source.clone();
        assert_eq!(
            source.open().unwrap().collect_batches().await.unwrap()[0].num_rows(),
            2
        );
        assert!(copy.open().unwrap_err().contains("already consumed"));
    }
    #[tokio::test]
    async fn explicit_spool_replays_and_releases_file() {
        let batch = batch();
        let directory =
            std::env::temp_dir().join(format!("orchid-stream-test-{}", std::process::id()));
        let source = BatchSource::spool(
            BatchStream::new(batch.schema(), futures::stream::iter([Ok(batch)])),
            &directory,
        )
        .await
        .unwrap();
        assert!(source.is_replayable());
        for _ in 0..2 {
            assert_eq!(
                source.open().unwrap().collect_batches().await.unwrap()[0].num_rows(),
                2
            );
        }
        drop(source);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);
        std::fs::remove_dir(directory).unwrap();
    }
    #[tokio::test]
    async fn terminal_error_drops_producer_without_dropping_stream_handle() {
        use std::sync::atomic::{AtomicBool, Ordering};
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let released = Arc::new(AtomicBool::new(false));
        let guard = Guard(released.clone());
        let batch = batch();
        let mut stream = BatchStream::new(
            batch.schema(),
            futures::stream::unfold((guard, Some(batch)), |(guard, batch)| async move {
                let item = batch
                    .map(Ok)
                    .unwrap_or_else(|| Err("late producer failure".into()));
                Some((item, (guard, None)))
            }),
        );
        assert_eq!(stream.next().await.unwrap().unwrap().num_rows(), 2);
        assert!(!released.load(Ordering::SeqCst));
        assert_eq!(
            stream.next().await.unwrap().unwrap_err(),
            "late producer failure"
        );
        assert!(released.load(Ordering::SeqCst));
        assert!(stream.next().await.is_none());
    }
}
