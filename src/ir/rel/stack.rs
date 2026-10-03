//! Recursive planning must remain safe on small async worker stacks.
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

const STACK_BYTES: usize = 64 * 1024 * 1024;

pub(super) fn on_query_stack<F: Future>(future: F) -> impl Future<Output = F::Output> {
    QueryStack {
        future: Some(Box::pin(future)),
    }
}

struct QueryStack<F: Future> {
    future: Option<Pin<Box<F>>>,
}

impl<F: Future> Future for QueryStack<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Grow for every poll: an async future may resume on another worker.
        // The boxed future stays pinned while the synchronous poll changes stacks.
        stacker::maybe_grow(STACK_BYTES, STACK_BYTES, || {
            self.get_mut().future.as_mut().unwrap().as_mut().poll(cx)
        })
    }
}

impl<F: Future> Drop for QueryStack<F> {
    fn drop(&mut self) {
        // Cancellation can destroy recursive logical/physical plans before
        // their normal completion path runs. Protect that destruction too.
        stacker::maybe_grow(STACK_BYTES, STACK_BYTES, || drop(self.future.take()));
    }
}
