//! Compiler-only stress cases: quoted Cypher names must survive SQL generation.
use datafusion::sql::sqlparser::{
    dialect::{DuckDbDialect, PostgreSqlDialect},
    parser::Parser,
};
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn cypher_name(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}
fn sql_name(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[tokio::test]
async fn mapped_names_and_parameter_literals_survive_both_sql_dialects() {
    let names = [
        "plain",
        "select",
        "two words",
        "a.b",
        "say\"hello",
        "back`tick",
        "雪",
        "line\nbreak",
        "x;--",
    ];
    let values = [
        "O'Reilly",
        "two''quotes",
        "slash\\'quote",
        "雪",
        "a\nb",
        "'); SELECT 1; --",
    ];
    let mut errors = Vec::new();
    let mut compiled_cases = Vec::new();
    for dialect in ["duckdb", "postgres"] {
        for name in names {
            for value in values {
                let query = format!(
                    "MATCH (n:{label}) WHERE n.{prop} = $value RETURN n.{prop} AS {alias}",
                    label = cypher_name(name),
                    prop = cypher_name(name),
                    alias = cypher_name(name)
                );
                let table = sql_name(name);
                let request = json!({"version":1,"dialect":dialect,"language":"cypher","query":query,
                    "parameters":{"value":value},
                    "tables":[{"name":table,"columns":[{"name":"id","data_type":"int64"},{"name":name,"data_type":"string"}]}],
                    "nodes":[{"label":name,"table":table,"id":"id","properties":{name:name}}]});
                match compile_json(&request.to_string()).await {
                    Ok(response) => {
                        let response: Value = serde_json::from_str(&response).unwrap();
                        let sql = response["sql"].as_str().unwrap();
                        let parsed = if dialect == "duckdb" {
                            Parser::parse_sql(&DuckDbDialect {}, sql)
                        } else {
                            Parser::parse_sql(&PostgreSqlDialect {}, sql)
                        };
                        if let Err(error) = parsed {
                            errors.push(format!("{dialect} {query}: {error}\n{sql}"));
                        }
                        if response["fields"] != json!([name]) {
                            errors.push(format!(
                                "{dialect} {query}: wrong fields {}",
                                response["fields"]
                            ));
                        }
                        compiled_cases
                            .push(json!({"dialect":dialect,"name":name,"value":value,"sql":sql}));
                    }
                    Err(error) => errors.push(format!("{dialect} {query}: {error}")),
                }
            }
        }
    }
    if let Ok(path) = std::env::var("ORCHIDDB_STRESS_SQL_OUTPUT") {
        std::fs::write(path, serde_json::to_vec(&compiled_cases).unwrap()).unwrap();
    }
    assert!(
        errors.is_empty(),
        "{} of {} cases failed:\n{}",
        errors.len(),
        names.len() * values.len() * 2,
        errors.join("\n")
    );
}
