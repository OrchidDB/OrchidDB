//! Compatibility adapter for the shared SPARQL update orchestration.
use super::{RdfSession, SparqlResults, decode_results};
use crate::language::sparql::update::{UpdateHost, UpdateSession};
use crate::ir::rel::sql::mutation::MappedMutation;

impl RdfSession<'_> {
    pub async fn update(&mut self, source: &str, base: Option<&str>) -> Result<(), String> {
        UpdateSession::new(self.mapping.clone(), self.dataset.clone(), self).update(source, base).await
    }
}
impl UpdateHost for RdfSession<'_> {
    async fn query(&mut self, query: &crate::spargebra::Query) -> Result<SparqlResults, String> {
        decode_results(&self.sparql_parsed(query).await?)
    }
    fn apply_effects(&mut self, effects: &mut [MappedMutation], ordered: bool) -> Result<(), String> {
        let mut executor = self.executor()?;
        if ordered { super::relational_update::order(effects, &mut executor)?; }
        for effect in effects { effect.execute(&mut executor)?; }
        Ok(())
    }
}
