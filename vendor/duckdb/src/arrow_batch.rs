use super::{
    Statement,
    arrow::{datatypes::SchemaRef, record_batch::RecordBatch},
};

/// A handle for the resulting RecordBatch of a query.
#[must_use = "Arrow is lazy and will do nothing unless consumed"]
pub struct Arrow<'stmt> {
    pub(crate) stmt: Option<&'stmt Statement<'stmt>>,
}

#[allow(clippy::needless_lifetimes)]
impl<'stmt> Arrow<'stmt> {
    #[inline]
    pub(crate) fn new(stmt: &'stmt Statement<'stmt>) -> Self {
        Arrow { stmt: Some(stmt) }
    }

    /// return arrow schema
    #[inline]
    pub fn get_schema(&self) -> SchemaRef {
        self.stmt.unwrap().stmt.schema()
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'stmt> Iterator for Arrow<'stmt> {
    type Item = RecordBatch;

    fn next(&mut self) -> Option<Self::Item> {
        Some(RecordBatch::from(&self.stmt?.step()?))
    }
}

/// A handle for the resulting RecordBatch of a query in streaming
#[must_use = "Arrow stream is lazy and will not fetch data unless consumed"]
#[allow(clippy::needless_lifetimes)]
pub struct ArrowStream<'stmt> {
    pub(crate) stmt: Option<&'stmt Statement<'stmt>>,
    pub(crate) schema: SchemaRef,
}

#[allow(clippy::needless_lifetimes)]
impl<'stmt> ArrowStream<'stmt> {
    #[inline]
    pub(crate) fn new(stmt: &'stmt Statement<'stmt>, schema: SchemaRef) -> Self {
        ArrowStream {
            stmt: Some(stmt),
            schema,
        }
    }

    /// Fetch one batch without silently converting a late query error to EOF.
    /// Errors and EOF both finish the stream; subsequent calls return None.
    pub fn next_result(&mut self) -> super::Result<Option<RecordBatch>> {
        let Some(stmt) = self.stmt else { return Ok(None); };
        match stmt.stmt.streaming_step_result(self.schema.clone()) {
            Ok(Some(array)) => Ok(Some(RecordBatch::from(&array))),
            Ok(None) => { self.stmt = None; Ok(None) },
            Err(error) => { self.stmt = None; Err(error) },
        }
    }

    /// return arrow schema
    #[inline]
    pub fn get_schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}

#[allow(clippy::needless_lifetimes)]
impl<'stmt> Iterator for ArrowStream<'stmt> {
    type Item = RecordBatch;

    fn next(&mut self) -> Option<Self::Item> {
        Some(RecordBatch::from(&self.stmt?.stream_step(self.get_schema())?))
    }
}
