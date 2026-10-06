//! Host-bindable DAG of existing callable kernels and relational SQL islands.
//! This is a physical-adapter description, not a GraphIR interpreter.
use super::*;
use arrow::datatypes::SchemaRef;
use datafusion::{
    datasource::{empty::EmptyTable, provider_as_source},
    logical_expr::LogicalPlanBuilder,
};
use serde_json::{Value as Json, json};
use std::collections::HashMap;

pub enum StageOperation {
    Kernel(RowKernel),
    Sql(String),
}
pub struct CompiledStage {
    pub id: usize,
    pub operation: StageOperation,
    pub inputs: Vec<usize>,
    pub schema: SchemaRef,
}
pub struct CompiledProgram {
    pub root: usize,
    pub stages: Vec<CompiledStage>,
}
impl CompiledProgram {
    pub fn new(plan: &LogicalPlan) -> std::result::Result<Self, QueryExecutionError> {
        let mut program = Self {
            root: 0,
            stages: Vec::new(),
        };
        program.root = program
            .add(plan, &mut HashMap::new())
            .map_err(QueryExecutionError::from_error)?;
        Ok(program)
    }
    fn add(&mut self, plan: &LogicalPlan, seen: &mut HashMap<u64, usize>) -> Result<usize> {
        if let Some(kernel) = RowKernel::from_plan(plan) {
            if let Some(id) = seen.get(&kernel.id()) {
                return Ok(*id);
            }
            let inputs = kernel
                .inputs()
                .iter()
                .map(|input| self.add(input, seen))
                .collect::<Result<Vec<_>>>()?;
            let id = self.stages.len();
            self.stages.push(CompiledStage {
                id,
                operation: StageOperation::Kernel(kernel.clone()),
                inputs,
                schema: transport::host_schema(),
            });
            seen.insert(kernel.id(), id);
            return Ok(id);
        }
        let mut inputs = Vec::new();
        // SQL operators may surround native boundaries after compiler rewrites.
        // Give each boundary a named, typed relation that the host binds to the
        // matching physical child; never collect it during compilation.
        let rewritten = plan
            .clone()
            .transform_down_with_subqueries(|node| {
                if RowKernel::from_plan(&node).is_none() {
                    return Ok(Transformed::no(node));
                }
                let id = self.add(&node, seen)?;
                if !inputs.contains(&id) {
                    inputs.push(id);
                }
                let schema = node.schema().as_arrow().clone();
                let scan = LogicalPlanBuilder::scan(
                    format!("__orchid_input_{id}"),
                    provider_as_source(Arc::new(EmptyTable::new(Arc::new(schema)))),
                    None,
                )?
                .build()?;
                Ok(Transformed::yes(scan))
            })?
            .data;
        let fields = rewritten
            .schema()
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect();
        let schema = transport::host_schema_for(rewritten.schema().as_arrow());
        let lowered = LoweredPlan {
            plan: rewritten,
            fields,
            result_form: crate::ir::policy::ResultForm::RowSet,
            islands: Default::default(),
        };
        let sql = super::super::sql::unparse(&lowered, super::super::sql::SqlDialect::DuckDb)
            .map_err(|error| failure(error.to_string()))?;
        let id = self.stages.len();
        self.stages.push(CompiledStage {
            id,
            operation: StageOperation::Sql(sql),
            inputs,
            schema,
        });
        Ok(id)
    }
    pub fn manifest(&self) -> Json {
        let nodes = self.stages.iter().map(|stage| {
            match &stage.operation {
                StageOperation::Kernel(kernel) => json!({
                    "id":stage.id,"kind":"kernel","name":kernel.name(),"inputs":stage.inputs,
                    "streaming":kernel.streaming() && (kernel.is_source() || kernel.slice().is_none()),"source":kernel.is_source(),
                    "relational_fields":kernel.relational_fields(),
                    "required_bindings":kernel.required_bindings(),"written_bindings":kernel.written_bindings(),
                    "slice":kernel.slice().map(|slice| json!({"offset":slice.offset,"fetch":slice.fetch,"tail":slice.tail})),
                    "single_partition":true
                }),
                StageOperation::Sql(sql) => json!({"id":stage.id,"kind":"sql","sql":sql,
                    "inputs":stage.inputs.iter().map(|id| json!({"node":id,"relation":format!("__orchid_input_{id}")})).collect::<Vec<_>>() }),
            }
        }).collect::<Vec<_>>();
        json!({"root":self.root,"nodes":nodes})
    }
    pub fn stage(&self, id: usize) -> std::result::Result<&CompiledStage, QueryExecutionError> {
        self.stages
            .get(id)
            .ok_or_else(|| format!("Unknown compiled stage {id}").into())
    }
}
