//! Code-configured aliases and typed UDF declarations over an engine catalog.
use super::{FunctionKind, FunctionOverload, OperatorTable};
use arrow::datatypes::DataType;
use datafusion::common::{DFSchema, DataFusionError, Result};
use datafusion::logical_expr::{Expr, ExprSchemable};
use std::{collections::BTreeMap, sync::Arc};

struct Mapping {
    target: String,
    kind: FunctionKind,
    overloads: Vec<FunctionOverload>,
    signatures: Vec<(Vec<DataType>, DataType)>,
}

impl Mapping {
    fn declared_signature(
        &self,
        name: &str,
        args: &[Expr],
        schema: &DFSchema,
    ) -> Result<&(Vec<DataType>, DataType)> {
        let actual = args
            .iter()
            .map(|arg| arg.get_type(schema))
            .collect::<Result<Vec<_>>>()?;
        let matches: Vec<_> =
            self.signatures
                .iter()
                .filter(|(parameters, _)| {
                    parameters.len() == actual.len()
                        && parameters.iter().zip(&actual).all(|(expected, actual)| {
                            actual == expected || *actual == DataType::Null
                        })
                })
                .collect();
        match matches.as_slice() {
            [signature] => Ok(*signature),
            [] => Err(error(format!(
                "no declared UDF overload for `{name}` with {actual:?}"
            ))),
            _ => Err(error(format!(
                "ambiguous declared UDF overload for `{name}` with {actual:?}; cast NULL arguments"
            ))),
        }
    }
}

/// An immutable-at-planning-time catalog overlay configured from Rust code.
/// Register real UDF implementations on the execution connection separately.
/// Mappings cannot override language-defined functions. Select this registry
/// with [`super::with_operator_table`] around both planning and lowering.
pub struct FunctionRegistry {
    base: Arc<dyn OperatorTable>,
    mappings: BTreeMap<String, Mapping>,
}

fn error(message: impl Into<String>) -> DataFusionError {
    DataFusionError::Plan(message.into())
}

impl FunctionRegistry {
    pub fn new(base: Arc<dyn OperatorTable>) -> Self {
        Self {
            base,
            mappings: BTreeMap::new(),
        }
    }

    fn validate_name(name: &str, target: &str, kind: FunctionKind) -> Result<String> {
        if name.is_empty() || target.is_empty() {
            return Err(error("function mapping names must not be empty"));
        }
        let name = name.to_ascii_lowercase();
        if crate::ir::rel::is_language_function(&name)
            || matches!(
                name.as_str(),
                "count"
                    | "count_if"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "collect"
                    | "stdev"
                    | "stdevp"
                    | "percentilecont"
                    | "percentiledisc"
            )
        {
            return Err(error(format!(
                "mapping `{name}` would override a language-defined function"
            )));
        }
        if !matches!(kind, FunctionKind::Scalar | FunctionKind::Aggregate) {
            return Err(error(
                "function mappings support only scalar and aggregate expressions",
            ));
        }
        Ok(name)
    }

    /// Alias a catalog-discovered scalar/macro or aggregate. Engine overload
    /// resolution, coercion and return-type inference remain authoritative.
    pub fn register_mapping(&mut self, name: &str, target: &str, kind: FunctionKind) -> Result<()> {
        let name = Self::validate_name(name, target, kind)?;
        if self.mappings.contains_key(&name) {
            return Err(error(format!("mapping `{name}` already exists")));
        }
        let overloads: Vec<_> = self
            .base
            .overloads(target)
            .iter()
            .filter(|f| f.kind == kind || f.kind == FunctionKind::Macro)
            .map(|f| {
                let mut f = f.clone();
                f.name = name.clone();
                f.kind = kind;
                f
            })
            .collect();
        if overloads.is_empty() {
            return Err(error(format!(
                "target `{target}` has no {kind:?} catalog overloads"
            )));
        }
        self.mappings.insert(
            name,
            Mapping {
                target: target.into(),
                kind,
                overloads,
                signatures: vec![],
            },
        );
        Ok(())
    }

    /// Declare a UDF overload when it cannot be introspected. Argument types
    /// match exactly (NULL is accepted); arguments are explicitly cast to the
    /// declared types so DuckDB selects the same overload for literals and NULL.
    /// Repeated declarations add overloads for the same target and kind. The
    /// caller must register an implementation with these types in the engine.
    pub fn register_typed_mapping(
        &mut self,
        name: &str,
        target: &str,
        kind: FunctionKind,
        parameters: Vec<DataType>,
        return_type: DataType,
    ) -> Result<()> {
        let name = Self::validate_name(name, target, kind)?;
        if let Some(existing) = self.mappings.get(&name) {
            if existing.target != target || existing.kind != kind || existing.signatures.is_empty()
            {
                return Err(error(format!("incompatible existing mapping `{name}`")));
            }
            if existing
                .signatures
                .iter()
                .any(|(args, _)| *args == parameters)
            {
                return Err(error(format!("duplicate UDF signature for `{name}`")));
            }
        }
        let mapping = self
            .mappings
            .entry(name.clone())
            .or_insert_with(|| Mapping {
                target: target.into(),
                kind,
                overloads: vec![],
                signatures: vec![],
            });
        mapping.overloads.push(FunctionOverload {
            name,
            kind,
            parameter_types: parameters.iter().map(ToString::to_string).collect(),
            varargs: None,
            return_type: Some(return_type.to_string()),
            stability: None,
        });
        mapping.signatures.push((parameters, return_type));
        Ok(())
    }
}

impl OperatorTable for FunctionRegistry {
    fn engine(&self) -> &str {
        self.base.engine()
    }
    fn target_name(&self, name: &str) -> String {
        self.mappings
            .get(&name.to_ascii_lowercase())
            .map(|m| self.base.target_name(&m.target))
            .unwrap_or_else(|| self.base.target_name(name))
    }
    fn prepare_args(
        &self,
        name: &str,
        kind: FunctionKind,
        args: Vec<Expr>,
        schema: &DFSchema,
    ) -> Result<Vec<Expr>> {
        let Some(mapping) = self.mappings.get(&name.to_ascii_lowercase()) else {
            return self.base.prepare_args(name, kind, args, schema);
        };
        if mapping.kind != kind {
            return Err(error(format!(
                "`{name}` is a {:?}, not a {kind:?}",
                mapping.kind
            )));
        }
        if mapping.signatures.is_empty() {
            return self.base.prepare_args(&mapping.target, kind, args, schema);
        }
        let (parameters, _) = mapping.declared_signature(name, &args, schema)?;
        args.into_iter()
            .zip(parameters)
            .map(|(arg, expected)| super::typed_argument_cast(arg, expected.clone()))
            .collect()
    }
    fn overloads(&self, name: &str) -> &[FunctionOverload] {
        self.mappings
            .get(&name.to_ascii_lowercase())
            .map(|m| m.overloads.as_slice())
            .unwrap_or_else(|| self.base.overloads(name))
    }
    fn bind(
        &self,
        name: &str,
        kind: FunctionKind,
        args: &[Expr],
        schema: &DFSchema,
    ) -> Result<DataType> {
        let Some(mapping) = self.mappings.get(&name.to_ascii_lowercase()) else {
            return self.base.bind(name, kind, args, schema);
        };
        if mapping.kind != kind {
            return Err(error(format!(
                "`{name}` is a {:?}, not a {kind:?}",
                mapping.kind
            )));
        }
        if mapping.signatures.is_empty() {
            return self.base.bind(&mapping.target, kind, args, schema);
        }
        Ok(mapping.declared_signature(name, args, schema)?.1.clone())
    }
}
