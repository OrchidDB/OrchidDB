//! Pristine fixture checkpoints, independent of the mutable engine graph.
use std::collections::VecDeque;
use orchiddb::{engine::{GraphEngine, ManagedGraphCheckpoint}, ir::catalog::PropertyGraph};

#[derive(Default)]
pub struct FixtureCache(VecDeque<(String, ManagedGraphCheckpoint)>);

impl FixtureCache {
    pub fn install(&mut self, engine: &mut GraphEngine, key: Option<&str>, graph: PropertyGraph) -> Result<(), String> {
        let checkpoint = ManagedGraphCheckpoint::new(graph)?;
        engine.restore_graph_checkpoint(&checkpoint)?;
        if let Some(key) = key {
            self.0.retain(|(name, _)| name != key);
            self.0.push_back((key.to_owned(), checkpoint));
            if self.0.len() > 16 { self.0.pop_front(); }
        }
        Ok(())
    }

    pub fn reset(&mut self, engine: &mut GraphEngine, key: &str) -> Result<(), String> {
        let index = self.0.iter().position(|(name, _)| name == key)
            .ok_or_else(|| format!("Unknown cached fixture: {key}"))?;
        engine.restore_graph_checkpoint(&self.0[index].1)?;
        let entry = self.0.remove(index).unwrap();
        self.0.push_back(entry);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchiddb::ir::value::Value;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn pristine_fixture_survives_writes_deletes_and_other_fixtures() {
        let mut engine = GraphEngine::in_memory().unwrap();
        let graph = PropertyGraph::new();
        graph.insert_node("P", BTreeMap::from([("name".into(), Value::String("original".into()))]));
        let mut cache = FixtureCache::default();
        cache.install(&mut engine, Some("first"), graph).unwrap();
        engine.cypher("MATCH (n) SET n.name='changed' CREATE (:P {name:'extra'})").await.unwrap();
        cache.reset(&mut engine, "first").unwrap();
        let result = engine.cypher("MATCH (n) RETURN n.name").await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1);
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "original");
        engine.cypher("MATCH (n) DETACH DELETE n").await.unwrap();
        cache.install(&mut engine, Some("empty"), PropertyGraph::new()).unwrap();
        cache.reset(&mut engine, "first").unwrap();
        assert_eq!(engine.cypher("MATCH (n) RETURN n").await.unwrap().returned.batch.num_rows(), 1);
        assert!(cache.reset(&mut engine, "missing").is_err());
        assert_eq!(engine.cypher("MATCH (n) RETURN n").await.unwrap().returned.batch.num_rows(), 1);
        cache.reset(&mut engine, "empty").unwrap();
        assert_eq!(engine.cypher("MATCH (n) RETURN n").await.unwrap().returned.batch.num_rows(), 0);
    }

    #[tokio::test]
    async fn cached_fixture_retains_public_ids_multi_properties_and_null_policy() {
        use orchiddb::ir::catalog::Cardinality;
        let graph = PropertyGraph::new();
        graph.enable_null_property_values(true);
        let vertex = graph.insert_node("P", BTreeMap::new());
        graph.set_element_public_id(&vertex, Value::String("external-id".into())).unwrap();
        for id in [11, 12] {
            let property = graph.set_vertex_property(&vertex, "name", Value::String("same".into()),
                Cardinality::List, BTreeMap::from([("source".into(), Value::Int(id))])).unwrap();
            graph.set_vertex_property_public_id(&property, Value::Long(id)).unwrap();
        }
        graph.set_vertex_property(&vertex, "nullable", Value::Null, Cardinality::Single, BTreeMap::new()).unwrap();
        let mut engine = GraphEngine::in_memory().unwrap();
        let mut cache = FixtureCache::default();
        cache.install(&mut engine, Some("typed"), graph).unwrap();
        let queries = ["g.V().id()", "g.V().properties('name').id()", "g.V().properties('name').values('source')", "g.V().values('nullable')"];
        let mut expected = Vec::new();
        for query in queries {
            let r = engine.gremlin(query).await.unwrap();
            expected.push(r.returned.batch);
        }
        engine.gremlin("g.V().drop()").await.unwrap();
        cache.reset(&mut engine, "typed").unwrap();
        for (query, expected) in queries.into_iter().zip(expected) {
            let r = engine.gremlin(query).await.unwrap();
            assert_eq!(r.returned.batch, expected, "{query}");
        }
    }
}
