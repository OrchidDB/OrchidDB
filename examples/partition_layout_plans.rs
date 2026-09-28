//! Generate actual compiler plans: cargo run --example partition_layout_plans
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn mapping() -> Value {
    let columns = json!([
        {"name":"id","data_type":"int64"},
        {"name":"customer_id","data_type":"int64"},
        {"name":"region","data_type":"string"}
    ]);
    let tables = ["events_by_id", "events_by_region", "events_by_customer"]
        .into_iter()
        .map(|name| json!({"name":name,"columns":columns}))
        .collect::<Vec<_>>();
    let buckets = (0..16)
        .map(|bucket| {
            json!({
                "spec_id":0,"bytes":1048576,"files":1,"values":{"1002":bucket.to_string()}
            })
        })
        .collect::<Vec<_>>();
    json!({
        "version":1,"dialect":"duckdb","language":"cypher","tables":tables,
        "nodes":[{"label":"Event","table":"events","id":"id","properties":{"id":"id","customer_id":"customer_id","region":"region"}}],
        "rdf":[{"table":"events","subject":{"kind":"template","prefix":"urn:event:","columns":["id"]},"predicate":{"kind":"constant","value":"urn:region"},"object":{"kind":"literal","column":"region"}}],
        "logical_sources":[{"name":"events","default_table":"events_by_id","layouts":[
            {"table":"events_by_id","specs":[{"spec_id":0,"fields":[{"source_column":"id","source_id":1,"field_id":1000,"transform":{"kind":"identity"}}]}],
             "partitions":[
                {"spec_id":0,"bytes":8388608,"files":8,"bounds":{"id":{"min":"0","max":"49"}}},
                {"spec_id":0,"bytes":8388608,"files":8,"bounds":{"id":{"min":"50","max":"99"}}}
             ]},
            {"table":"events_by_region","specs":[{"spec_id":0,"fields":[{"source_column":"region","source_id":3,"field_id":1001,"transform":{"kind":"identity"}}]}],
             "partitions":[
                {"spec_id":0,"bytes":8388608,"files":8,"values":{"1001":"east"}},
                {"spec_id":0,"bytes":8388608,"files":8,"values":{"1001":"west"}}
             ]},
            {"table":"events_by_customer","specs":[{"spec_id":0,"fields":[{"source_column":"customer_id","source_id":2,"field_id":1002,"transform":{"kind":"bucket","buckets":16}}]}],"partitions":buckets}
        ]}]
    })
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mapping = mapping();
    let mut examples = Vec::new();
    for (name, language, query) in [
        (
            "id_filter",
            "cypher",
            "MATCH (e:Event) WHERE e.id = 5 RETURN e.id, e.region",
        ),
        (
            "region_filter",
            "cypher",
            "MATCH (e:Event) WHERE e.region = 'west' RETURN e.id, e.region",
        ),
        (
            "bucket_filter",
            "cypher",
            "MATCH (e:Event) WHERE e.customer_id = 34 RETURN e.id, e.customer_id",
        ),
        (
            "self_join",
            "cypher",
            "MATCH (a:Event), (b:Event) WHERE a.id = 5 AND b.region = 'west' RETURN a.id, b.id",
        ),
        (
            "rdf_filter",
            "sparql",
            "SELECT ?region WHERE { ?s <urn:region> ?region FILTER(?region = \"west\") }",
        ),
    ] {
        let mut request = mapping.clone();
        request["language"] = json!(language);
        request["query"] = json!(query);
        let compiled: Value = serde_json::from_str(
            &compile_json(&request.to_string())
                .await
                .map_err(std::io::Error::other)?,
        )?;
        examples.push(json!({"name":name,"language":language,"query":query,"compiled":compiled}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"mapping":mapping,"examples":examples}))?
    );
    Ok(())
}
