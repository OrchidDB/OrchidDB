//! Query-scoped switches for internal compiler diagnostics, never process-wide
//! engine configuration. The flag is installed for each poll, so suspension,
//! task migration, nested scopes and unwinding restore the caller's setting.
use std::{
    cell::Cell,
    future::{Future, poll_fn},
    pin::pin,
};

thread_local! {
    static CONSTANT_FOLDING: Cell<bool> = const { Cell::new(true) };
}

pub(crate) fn constant_folding_enabled() -> bool {
    CONSTANT_FOLDING.with(Cell::get)
}

struct Restore(bool);
impl Drop for Restore {
    fn drop(&mut self) {
        CONSTANT_FOLDING.with(|flag| flag.set(self.0));
    }
}

/// Internal SQL-quality diagnostics: suppress optional frontend/relational
/// constant evaluation and DataFusion expression simplification while polling
/// this future. Literal encoding, type/schema validation, structural rewrites,
/// and target-engine execution optimizations remain enabled. Spawned tasks do
/// not inherit the scope; compiler work must be awaited inside this future.
#[doc(hidden)]
pub async fn without_constant_folding<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    poll_fn(move |cx| {
        let _restore = Restore(CONSTANT_FOLDING.with(|flag| flag.replace(false)));
        future.as_mut().poll(cx)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll};

    #[test]
    fn restores_between_polls_and_after_completion() {
        let mut polls = 0;
        let work = poll_fn(|_| {
            assert!(!constant_folding_enabled());
            polls += 1;
            if polls == 1 {
                Poll::Pending
            } else {
                Poll::Ready(7)
            }
        });
        let mut scoped = pin!(without_constant_folding(work));
        let mut context = Context::from_waker(std::task::Waker::noop());
        assert!(scoped.as_mut().poll(&mut context).is_pending());
        assert!(constant_folding_enabled());
        assert_eq!(scoped.as_mut().poll(&mut context), Poll::Ready(7));
        assert!(constant_folding_enabled());
    }

    #[tokio::test]
    async fn nested_scopes_and_panics_restore_caller_setting() {
        without_constant_folding(async {
            assert!(!constant_folding_enabled());
            without_constant_folding(async {
                assert!(!constant_folding_enabled());
            })
            .await;
            assert!(!constant_folding_enabled());
            assert!(std::thread::spawn(constant_folding_enabled).join().unwrap());
        })
        .await;
        assert!(constant_folding_enabled());
        use futures::FutureExt;
        let result = std::panic::AssertUnwindSafe(without_constant_folding(async {
            assert!(!constant_folding_enabled());
            panic!("diagnostic panic");
        }))
        .catch_unwind()
        .await;
        assert!(result.is_err());
        assert!(constant_folding_enabled());
    }
}
