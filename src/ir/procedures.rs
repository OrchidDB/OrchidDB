//! Registered table-backed procedures with explicit input/output signatures.
use crate::ir::value::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureField {
    pub name: String,
    pub type_name: String,
    pub nullable: bool,
}

impl ProcedureField {
    pub fn accepts(&self, value: &Value) -> bool {
        if matches!(value, Value::Null) {
            return self.nullable;
        }
        match self.type_name.to_ascii_uppercase().as_str() {
            "ANY" => true,
            "STRING" => matches!(value, Value::String(_)),
            "INTEGER" => matches!(
                value,
                Value::Byte(_) | Value::Short(_) | Value::Int(_) | Value::Long(_)
            ),
            "FLOAT" | "NUMBER" => matches!(
                value,
                Value::Float(_) | Value::Float32(_) | Value::Int(_) | Value::Long(_)
            ),
            "BOOLEAN" => matches!(value, Value::Bool(_)),
            "MAP" => matches!(value, Value::Map(_)),
            "NODE" => matches!(value, Value::Node { .. }),
            "RELATIONSHIP" => matches!(value, Value::Edge { .. }),
            "PATH" => matches!(value, Value::Path(_)),
            name if name.starts_with("LIST") => matches!(value, Value::List(_)),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureSignature {
    pub inputs: Vec<ProcedureField>,
    pub outputs: Vec<ProcedureField>,
}

/// Each row contains input fields followed by output fields. Calls select
/// matching input tuples and emit their output tuples in registration order.
#[derive(Debug, Clone)]
pub struct TableProcedure {
    pub signature: ProcedureSignature,
    pub rows: Vec<Vec<Value>>,
}

impl TableProcedure {
    pub fn validate(&self) -> Result<(), String> {
        let fields = self
            .signature
            .inputs
            .iter()
            .chain(&self.signature.outputs)
            .collect::<Vec<_>>();
        for row in &self.rows {
            if row.len() != fields.len()
                || row
                    .iter()
                    .zip(&fields)
                    .any(|(value, field)| !field.accepts(value))
            {
                return Err("Procedure row does not match its declared signature".into());
            }
        }
        Ok(())
    }
}

pub type ProcedureCatalog = BTreeMap<String, TableProcedure>;
