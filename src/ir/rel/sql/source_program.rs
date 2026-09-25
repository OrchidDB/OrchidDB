//! Prepare a SQL IR region and its external source dependencies without I/O.
//! The shared `dag` executor plans these sources as DataFusion children of the
//! DuckDB region. This module contains no execution loop or Graph IR.
use super::*;

pub(crate) struct PreparedSourceProgram {
    pub sql: PreparedSql,
    pub sources: Vec<(String, LogicalPlan)>,
}

impl PreparedSourceProgram {
    pub async fn prepare(
        lowered: &LoweredPlan,
        dialect: SqlDialect,
        external: &BTreeSet<String>,
    ) -> SqlResult<Self> {
        let mut sources = BTreeMap::new();
        lowered.plan.apply_with_subqueries(|node| {
            if let LogicalPlan::TableScan(scan) = node {
                if let Ok(provider) = datafusion::datasource::source_as_provider(&scan.source) {
                    if provider
                        .as_any()
                        .is::<super::super::rdf_service::ServiceSource>()
                    {
                        let name = scan.table_name.to_string();
                        let source = TableScan::try_new(
                            name.clone(),
                            scan.source.clone(),
                            None,
                            vec![],
                            None,
                        )?;
                        sources
                            .entry(name)
                            .or_insert(LogicalPlan::TableScan(source));
                    }
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })?;
        let mut excluded = external.clone();
        excluded.extend(sources.keys().cloned());
        let sql = prepare_with_external(lowered, dialect, &excluded).await?;
        Ok(Self {
            sql,
            sources: sources.into_iter().collect(),
        })
    }
}
