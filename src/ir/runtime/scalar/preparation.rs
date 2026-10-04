//! Reusable scalar metadata owned by one execution context. A synchronous
//! kernel installs its context for the duration of the call, never across await.
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{cast_conversion::CastTarget, registry};

#[derive(Debug)]
pub(crate) struct Memo<T>(Mutex<HashMap<String, T>>);

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

impl<T: Clone> Memo<T> {
    pub(crate) fn get(&self, key: &str, build: impl FnOnce() -> T) -> T {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = entries.get(key) {
            return value.clone();
        }
        entries.entry(key.to_owned()).or_insert_with(build).clone()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

pub(crate) type RegexMemo = Memo<Result<Arc<regex::Regex>, regex::Error>>;

#[derive(Debug)]
pub(super) struct ResolvedCall {
    pub(super) canonical: String,
    pub(super) known: bool,
}

fn resolve(name: &str) -> Arc<ResolvedCall> {
    let canonical = registry::canonical_name(name).into_owned();
    let known = registry::is_known_canonical(&canonical);
    Arc::new(ResolvedCall { canonical, known })
}

#[derive(Debug, Default)]
pub(crate) struct ScalarPreparation {
    regexes: RegexMemo,
    calls: Memo<Arc<ResolvedCall>>,
    casts: Memo<Arc<CastTarget>>,
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<ScalarPreparation>>> = const { RefCell::new(None) };
}

pub(crate) fn with_preparation<T>(cache: &Arc<ScalarPreparation>, run: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<ScalarPreparation>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(ACTIVE.with(|active| active.replace(Some(cache.clone()))));
    run()
}

pub(super) fn call(name: &str) -> Arc<ResolvedCall> {
    ACTIVE.with(|active| match active.borrow().as_ref() {
        Some(cache) => cache.calls.get(name, || resolve(name)),
        None => resolve(name),
    })
}

pub(super) fn cast(name: &str) -> Arc<CastTarget> {
    ACTIVE.with(|active| match active.borrow().as_ref() {
        Some(cache) => cache.casts.get(name, || Arc::new(CastTarget::parse(name))),
        None => Arc::new(CastTarget::parse(name)),
    })
}

pub(crate) fn regex(pattern: &str) -> Result<Arc<regex::Regex>, regex::Error> {
    ACTIVE.with(|active| match active.borrow().as_ref() {
        Some(cache) => cache
            .regexes
            .get(pattern, || regex::Regex::new(pattern).map(Arc::new)),
        None => regex::Regex::new(pattern).map(Arc::new),
    })
}

#[cfg(test)]
mod tests;
