//! Process-wide indexes for immutable built-in functions. Each function family
//! is initialized on first use; query providers and residual batches share it.
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use datafusion::logical_expr::{AggregateUDF, ScalarUDF, WindowUDF};

pub(super) struct FunctionCatalog<T> {
    by_name: HashMap<String, Arc<T>>,
    names: Vec<String>,
}

impl<T> FunctionCatalog<T> {
    fn new(functions: Vec<Arc<T>>, metadata: fn(&T) -> (&str, &[String])) -> Self {
        let mut by_name = HashMap::new();
        let mut names = Vec::with_capacity(functions.len());
        for function in functions {
            let (name, aliases) = metadata(&function);
            names.push(name.to_owned());
            for name in std::iter::once(name).chain(aliases.iter().map(String::as_str)) {
                // Preserve the previous linear lookup's first-match precedence,
                // including collisions between aliases and canonical names.
                by_name
                    .entry(name.to_owned())
                    .or_insert_with(|| Arc::clone(&function));
            }
        }
        Self { by_name, names }
    }

    pub(super) fn get(&self, name: &str) -> Option<&Arc<T>> {
        self.by_name.get(name)
    }

    pub(super) fn names(&self) -> Vec<String> {
        self.names.clone()
    }
}

pub(super) fn scalar() -> &'static FunctionCatalog<ScalarUDF> {
    static CATALOG: LazyLock<FunctionCatalog<ScalarUDF>> = LazyLock::new(|| {
        FunctionCatalog::new(datafusion::functions::all_default_functions(), |f| {
            (f.name(), f.aliases())
        })
    });
    &CATALOG
}

pub(super) fn nested() -> &'static FunctionCatalog<ScalarUDF> {
    static CATALOG: LazyLock<FunctionCatalog<ScalarUDF>> = LazyLock::new(|| {
        FunctionCatalog::new(
            datafusion::functions_nested::all_default_nested_functions(),
            |f| (f.name(), f.aliases()),
        )
    });
    &CATALOG
}

pub(super) fn aggregate() -> &'static FunctionCatalog<AggregateUDF> {
    static CATALOG: LazyLock<FunctionCatalog<AggregateUDF>> = LazyLock::new(|| {
        FunctionCatalog::new(
            datafusion::functions_aggregate::all_default_aggregate_functions(),
            |f| (f.name(), f.aliases()),
        )
    });
    &CATALOG
}

pub(super) fn window() -> &'static FunctionCatalog<WindowUDF> {
    static CATALOG: LazyLock<FunctionCatalog<WindowUDF>> = LazyLock::new(|| {
        FunctionCatalog::new(
            datafusion::functions_window::all_default_window_functions(),
            |f| (f.name(), f.aliases()),
        )
    });
    &CATALOG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_preserve_every_builtin_name_alias_and_listing() {
        macro_rules! check {
            ($catalog:expr, $functions:expr) => {{
                let catalog = $catalog;
                let functions = $functions;
                assert_eq!(
                    catalog.names(),
                    functions
                        .iter()
                        .map(|f| f.name().to_owned())
                        .collect::<Vec<_>>()
                );
                for function in &functions {
                    for name in std::iter::once(function.name())
                        .chain(function.aliases().iter().map(String::as_str))
                    {
                        let expected = functions
                            .iter()
                            .find(|f| {
                                f.name() == name || f.aliases().iter().any(|alias| alias == name)
                            })
                            .unwrap();
                        assert_eq!(
                            catalog.get(name).unwrap().as_ref(),
                            expected.as_ref(),
                            "{name}"
                        );
                    }
                }
                assert!(catalog.get("__orchiddb_missing_function").is_none());
            }};
        }
        check!(scalar(), datafusion::functions::all_default_functions());
        check!(
            nested(),
            datafusion::functions_nested::all_default_nested_functions()
        );
        check!(
            aggregate(),
            datafusion::functions_aggregate::all_default_aggregate_functions()
        );
        check!(
            window(),
            datafusion::functions_window::all_default_window_functions()
        );
    }

    #[test]
    fn first_alias_match_wins_over_a_later_canonical_name() {
        let first = Arc::new(("first".to_owned(), vec!["second".to_owned()]));
        let second = Arc::new(("second".to_owned(), vec!["first".to_owned()]));
        let catalog = FunctionCatalog::new(vec![first.clone(), second], |f| (&f.0, &f.1));
        assert!(Arc::ptr_eq(catalog.get("first").unwrap(), &first));
        assert!(Arc::ptr_eq(catalog.get("second").unwrap(), &first));
        assert_eq!(catalog.names(), ["first", "second"]);
    }
}
