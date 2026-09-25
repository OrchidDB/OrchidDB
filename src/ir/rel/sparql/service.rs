use super::*;
use datafusion::datasource::provider_as_source;

impl Lowerer<'_, '_> {
    pub(super) fn service(
        &mut self,
        endpoint: &RdfTerm,
        query: &str,
        outputs: &[String],
        silent: bool,
    ) -> RelResult<Sol> {
        let endpoints = self
            .ctx
            .options
            .rdf_datasets
            .as_ref()
            .map(|mapping| mapping.service_endpoints.clone())
            .unwrap_or_default();
        match endpoint {
            RdfTerm::Iri(iri) => {
                self.service_scan(endpoints.get(iri).unwrap_or(iri), query, outputs, silent)
            }
            RdfTerm::Variable(_) => {
                unsupported("Variable SERVICE needs a correlated external-read stage")
            }
            _ => unsupported("SERVICE endpoint must be an IRI or variable"),
        }
    }

    fn service_scan(
        &mut self,
        url: &str,
        query: &str,
        outputs: &[String],
        silent: bool,
    ) -> RelResult<Sol> {
        let provider = Arc::new(super::super::rdf_service::ServiceSource::new(
            url.into(),
            query.into(),
            outputs.to_vec(),
            silent,
        ));
        let plan =
            LogicalPlanBuilder::scan(self.fresh("service"), provider_as_source(provider), None)?
                .build()?;
        Ok(Sol {
            plan,
            vars: outputs.iter().map(|name| (name.clone(), false)).collect(),
            keys: BTreeSet::new(),
            ord: None,
        })
    }
}
