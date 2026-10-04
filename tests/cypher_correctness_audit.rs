//! Focused correctness audit; all queries execute through the local relational runtime.
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn rows(graph: &PropertyGraph, query: &str) -> Result<Vec<String>, String> {
    let ast = parse_query(query).map_err(|e| e.to_string())?;
    let plan = CypherPlanner::new().plan(&ast).map_err(|e| e.to_string())?;
    let (result, _) = orchiddb::ir::rel::runtime::execute(&plan, graph, None).await?;
    Ok((0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect())
}

async fn check(graph: &PropertyGraph, cases: &[(&str, &[&str])]) {
    let mut failures = Vec::new();
    for (query, expected) in cases {
        match rows(graph, query).await {
            Ok(actual) if actual == *expected => {}
            actual => failures.push(format!(
                "{query}\nexpected: {expected:?}\nactual: {actual:?}"
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[tokio::test]
async fn null_semantics() {
    check(&PropertyGraph::new(), &[
        ("RETURN null = null, null <> null, null < 1, NOT null", &["|||"]),
        ("RETURN false AND null, true AND null, false OR null, true OR null", &["false|||true"]),
        ("RETURN null IN [], null IN [1], 1 IN [null,1], 2 IN [null,1]", &["false||true|"]),
        ("RETURN [null,1] = [null,2], {a:null,b:1} = {a:null,b:2}", &["false|false"]),
        ("RETURN any(x IN [null,false] WHERE x), all(x IN [null,false] WHERE x), none(x IN [null,true] WHERE x), single(x IN [true,true,null] WHERE x)", &["|false|false|false"]),
        ("RETURN [x IN [null,1,2] WHERE x > 1 | x], CASE null WHEN null THEN 1 ELSE 2 END", &["[2]|2"]),
        ("UNWIND [null,1] AS x WITH x WHERE x = null RETURN x", &[]),
        ("UNWIND [null,1] AS x WITH x WHERE x IS NULL RETURN x IS NULL", &["true"]),
        ("RETURN size(null), reverse(null), head(null), coalesce(null, false, true)", &["|||false"]),
    ]).await;
}

#[tokio::test]
async fn variable_scope() {
    check(&PropertyGraph::new(), &[
        ("WITH 10 AS x RETURN [x IN [1,2] | x + 1], x", &["[2,3]|10"]),
        ("WITH [1,2] AS x RETURN [x IN x | x + 1], x", &["[2,3]|[1,2]"]),
        ("RETURN [x IN [1,2] | [x IN [3,4] | x]], [x IN [1,2] | x]", &["[[3,4],[3,4]]|[1,2]"]),
        ("WITH 7 AS x RETURN any(x IN [1,2] WHERE x = 2), x", &["true|7"]),
        ("UNWIND [1,1,2] AS n RETURN n, [x IN [1,2] | [y IN [3,4] WHERE y > n + x | y]] ORDER BY n", &["1|[[3,4],[4]]", "1|[[3,4],[4]]", "2|[[4],[]]"]),
        ("RETURN [x IN [1,2] | [y IN [] | y]], [x IN [] | [y IN [1] | y]]", &["[[],[]]|[]"]),
        ("WITH 1 AS x, 2 AS y WITH y AS x, x AS y RETURN x, y", &["2|1"]),
        ("WITH 1 AS x RETURN EXISTS { WITH 2 AS y RETURN y }, x", &["true|1"]),
    ]).await;
    for query in [
        "WITH 1 AS x WITH 2 AS y RETURN x",
        "RETURN [x IN [1,2] | x] AS ys, x",
        "WITH 1 AS x RETURN x AS y, y",
        "WITH 1 AS x RETURN EXISTS { WITH 2 AS y RETURN y } AS yes, y",
        "WITH 1 AS x RETURN x UNION RETURN x",
        "UNWIND [1,2] AS x RETURN DISTINCT x + 1 AS y ORDER BY x",
    ] {
        let ast = parse_query(query).unwrap();
        assert!(
            CypherPlanner::new().plan(&ast).is_err(),
            "out-of-scope variable accepted: {query}"
        );
    }
}

#[tokio::test]
async fn optional_match() {
    let graph = PropertyGraph::new();
    rows(
        &graph,
        "CREATE (a:P {name:'a'}), (b:P {name:'b'}), (c:P {name:'c'}), (a)-[:R]->(b), (b)-[:R]->(b)",
    )
    .await
    .unwrap();
    check(&graph, &[
        ("MATCH (a:P {name:'c'}) OPTIONAL MATCH (a)-[r:R]->(b) RETURN a.name, r IS NULL, b IS NULL", &["c|true|true"]),
        ("MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[r:R]->(b) WHERE false RETURN a.name, r IS NULL, b IS NULL", &["a|true|true"]),
        ("MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[r:R]->(b), (b)-[:Missing]->(c) RETURN a.name, r IS NULL, b IS NULL, c IS NULL", &["a|true|true|true"]),
        ("MATCH (a:P {name:'c'}) OPTIONAL MATCH (a)-[:R]->(b) OPTIONAL MATCH (b)-[:R]->(c) RETURN a.name, b IS NULL, c IS NULL", &["c|true|true"]),
        ("MATCH (a:P {name:'a'}), (b:P {name:'c'}) OPTIONAL MATCH (a)-[r:R]->(b) RETURN a.name,b.name,r IS NULL", &["a|c|true"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R]->(b), (b)-[s:R]->(c) RETURN r IS NULL, s IS NULL, b IS NULL, c IS NULL", &["true|true|true|true"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R]->(b) OPTIONAL MATCH (b)-[s:R]->(c) RETURN r = s", &["true"]),
        ("OPTIONAL MATCH (a:Missing) RETURN count(*),count(a),collect(a)", &["1|0|[]"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R]->(b), (b)-[s:R]->(c {name:a.name}) RETURN r IS NULL,s IS NULL", &["true|true"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R*1..1]->(b), (b)-[s:R*1..1]->(c) RETURN r IS NULL,s IS NULL", &["true|true"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R]->(b), (b)-[s:R]->(c) WHERE null RETURN r IS NULL,s IS NULL", &["true|true"]),
        ("MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[r:R*1..1]->(b), (b)-[s:R*1..1]->(c) RETURN b.name,c.name,size(r),size(s)", &["b|b|1|1"]),
        ("MATCH (a:P {name:'b'}) OPTIONAL MATCH (a)-[r:R*1..1]->(b) OPTIONAL MATCH (b)-[s:R*1..1]->(c) RETURN r = s", &["true"]),
        ("MATCH (a:P {name:'b'}) MATCH (a)-[r:R*1..1]->(b), (b)-[s:R*1..1]->(c) RETURN count(*)", &["0"]),
    ]).await;
}

#[tokio::test]
async fn aggregation() {
    check(&PropertyGraph::new(), &[
        ("UNWIND [] AS x RETURN sum(x)", &["0"]),
        ("UNWIND [] AS x RETURN min(x)", &[""]),
        ("UNWIND [] AS x RETURN max(x)", &[""]),
        ("UNWIND [] AS x RETURN avg(x)", &[""]),
        ("UNWIND [] AS x RETURN avg(DISTINCT x)", &[""]),
        ("UNWIND [null,null] AS x RETURN avg(x)", &[""]),
        ("RETURN avg(null)", &[""]),
        ("UNWIND [1,2,3] AS x RETURN avg(x),avg(DISTINCT x)", &["2.0|2.0"]),
        ("UNWIND [1,2] AS x WITH x WHERE false RETURN avg(x)", &[""]),
        ("UNWIND [] AS x RETURN collect(x), count(*), stDev(x), stDevP(x)", &["[]|0|0.0|0.0"]),
        ("UNWIND [] AS x RETURN x, count(*)", &[]),
        ("UNWIND [null,null] AS x RETURN sum(x),avg(x),min(x),max(x),collect(x),count(x),count(*)", &["0||||[]|0|2"]),
        ("UNWIND [1,1.0,2,null] AS x RETURN count(DISTINCT x),sum(DISTINCT x),avg(DISTINCT x)", &["2|3|1.5"]),
        ("UNWIND [null,null,1] AS x RETURN x,count(*) ORDER BY x", &["1|1", "|2"]),
        ("UNWIND [[1,null],[1,null],[1,2]] AS x RETURN count(DISTINCT x)", &["2"]),
        ("UNWIND [3,1,2,null] AS x WITH x ORDER BY x RETURN collect(x)", &["[1,2,3]"]),
    ]).await;
}

#[tokio::test]
async fn unicode_handling() {
    check(&PropertyGraph::new(), &[
        ("RETURN size('é'), size('👩‍💻'), size('😀')", &["2|3|1"]),
        ("RETURN substring('éx',1,1), left('éx',1), right('xé',1)", &["́|e|́"]),
        ("RETURN reverse('éx'), reverse('👩‍💻')", &["x́e|💻‍👩"]),
        ("WITH 'éx' AS s RETURN size(s), substring(s,1,1), left(s,1), right(s,1), reverse(s)", &["3|́|e|x|x́e"]),
        ("RETURN size(split('é','')), size(split('👩‍💻',''))", &["2|3"]),
        ("WITH 7 AS `café`, 9 AS `变量` RETURN `café`, `变量`", &["7|9"]),
        (r"RETURN '\uD83D\uDE00' = '😀', '\U0001F600' = '😀'", &["true|true"]),
        ("RETURN 'é' = 'é', toUpper('é'), toLower('É')", &["false|É|é"]),
        ("UNWIND ['',null,'á','👩‍💻','😀'] AS s RETURN reverse(s),reverse(s) IS NULL", &["|false", "|true", "́a|false", "💻‍👩|false", "😀|false"]),
        (r"WITH 'a\nb' AS s RETURN reverse(s)", &["b\na"]),
        ("RETURN substring('é',0,0),substring('é',9,2),left('é',9),right('é',9)", &["||é|é"]),
    ]).await;
}
