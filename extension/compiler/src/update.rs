//! Host transport for the existing SPARQL update orchestration and row effects.
use arrow::{
    array::{ArrayRef, BooleanArray, RecordBatch, StringArray},
    datatypes::{DataType, Field, Schema},
};
use orchiddb::{
    compiler,
    ir::{
        policy::ResultForm,
        rel::sql::mutation::{MappedMutation, MutationHost},
        runtime::ReturnedBatches,
    },
    language::sparql::{
        results::{SparqlResults, decode_results},
        update::{UpdateHost, UpdateSession},
    },
};
use serde_json::{Value, json};
use std::{
    ffi::{CStr, CString, c_char, c_void},
    sync::Arc,
};

pub type Callback = unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_char;
pub type Free = unsafe extern "C" fn(*mut c_char);
struct Host {
    state: *mut c_void,
    callback: Callback,
    free: Free,
    request: Value,
}
impl Host {
    fn sql(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<Value, String> {
        let input = CString::new(json!({"sql":sql,"parameters":parameters}).to_string())
            .map_err(|e| e.to_string())?;
        let output = unsafe { (self.callback)(self.state, input.as_ptr()) };
        if output.is_null() {
            return Err("Host returned no SQL result".into());
        }
        let result = unsafe { CStr::from_ptr(output) }
            .to_str()
            .map(str::to_owned)
            .map_err(|e| e.to_string());
        unsafe { (self.free)(output) };
        let result: Value = serde_json::from_str(&result?).map_err(|e| e.to_string())?;
        if result["ok"] != true {
            return Err(result["error"]
                .as_str()
                .unwrap_or("Host execution failed")
                .into());
        }
        Ok(result["result"].clone())
    }
}
impl MutationHost for Host {
    fn count(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<i64, String> {
        self.sql(sql, parameters)?["rows"][0][0]
            .as_i64()
            .ok_or_else(|| "Host count result is not an integer".into())
    }
    fn execute(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<(), String> {
        self.sql(sql, parameters).map(|_| ())
    }
}
impl UpdateHost for Host {
    async fn query(&mut self, query: &orchiddb::spargebra::Query) -> Result<SparqlResults, String> {
        let compiled = compiler::compile_sparql(
            serde_json::from_value(self.request.clone()).map_err(|e| e.to_string())?,
            query,
        )
        .await?;
        if !compiled.transfers.is_empty() {
            return Err("Update query must execute in the host DuckDB".into());
        }
        let result = self.sql(&compiled.sql, vec![])?;
        let names = result["columns"]
            .as_array()
            .ok_or("Host returned no column names")?;
        let rows = result["rows"].as_array().ok_or("Host returned no rows")?;
        let form = match compiled.result_form.as_str() {
            "Boolean" => ResultForm::Boolean,
            "RdfGraph" => ResultForm::RdfGraph,
            _ => ResultForm::RowSet,
        };
        let mut fields = Vec::new();
        let mut columns: Vec<ArrayRef> = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let name = name.as_str().ok_or("Host column name must be text")?;
            if form != ResultForm::Boolean
                && !compiled.fields.iter().any(|field| {
                    field == name
                        || orchiddb::ir::rel::rdf::binding_identity_columns(field)
                            .iter()
                            .any(|identity| identity == name)
                })
            {
                continue;
            }
            if form == ResultForm::Boolean {
                fields.push(Field::new(name, DataType::Boolean, true));
                columns.push(Arc::new(BooleanArray::from(
                    rows.iter()
                        .map(|row| {
                            if row[i].is_null() {
                                Ok(None)
                            } else {
                                row[i]
                                    .as_bool()
                                    .map(Some)
                                    .ok_or("ASK value must be boolean")
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )));
            } else {
                fields.push(Field::new(name, DataType::Utf8, true));
                columns.push(Arc::new(StringArray::from(
                    rows.iter()
                        .map(|row| {
                            if row[i].is_null() {
                                Ok(None)
                            } else {
                                row[i].as_str().map(Some).ok_or_else(|| {
                                    format!("RDF lexical value in {name} must be text: {}", row[i])
                                })
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )));
            }
        }
        let batch = RecordBatch::try_new_with_options(
            Arc::new(Schema::new(fields)),
            columns,
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(rows.len())),
        )
        .map_err(|e| e.to_string())?;
        decode_results(&ReturnedBatches {
            fields: compiled.fields,
            result_form: form,
            batch,
        })
    }
    fn apply_effects(
        &mut self,
        effects: &mut [MappedMutation],
        ordered: bool,
    ) -> Result<(), String> {
        if ordered {
            let result = self.sql("SELECT table_name, referenced_table FROM duckdb_constraints() WHERE constraint_type='FOREIGN KEY'", vec![])?;
            let dependencies = result["rows"]
                .as_array()
                .ok_or("Host returned no constraints")?
                .iter()
                .map(|r| {
                    Ok((
                        r[0].as_str().ok_or("Invalid constraint table")?.to_owned(),
                        r[1].as_str()
                            .ok_or("Invalid constraint reference")?
                            .to_owned(),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            orchiddb::language::sparql::order_mutations(effects, &dependencies)?;
        }
        for effect in effects {
            effect.execute_in(self)?;
        }
        Ok(())
    }
}

pub fn run(
    input: Value,
    state: *mut c_void,
    callback: Callback,
    free: Free,
) -> Result<Value, String> {
    let request: compiler::CompileRequest =
        serde_json::from_value(input["request"].clone()).map_err(|e| e.to_string())?;
    if request.language != "sparql" || request.dialect != "duckdb" || !request.engines.is_empty() {
        return Err("SPARQL updates require the host DuckDB".into());
    }
    let mapping = Arc::new(compiler::rdf_mapping(&request)?);
    let mut host = Host {
        state,
        callback,
        free,
        request: input["request"].clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(
        UpdateSession::new(mapping, request.dataset, &mut host)
            .update(&request.query, input["base"].as_str()),
    )?;
    Ok(json!({"updated": true}))
}
