use orchiddb::ir::rel::mapping::{ComputedRelationship, RelationshipStage, RelationshipOrder, SortDirection, NullOrder, RetrievalMode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
pub fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn qualified(parts: &[String]) -> String {
    parts.iter().map(|p| ident(p)).collect::<Vec<_>>().join(".")
}
fn graph_view(parts: &[String]) -> String {
    let mut parts = parts.to_vec();
    *parts.last_mut().unwrap() = format!("__orchid_graph_{}", parts.last().unwrap());
    qualified(&parts)
}

#[derive(Clone, Debug)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    kind: char,
}

// Reuse the SQL tokenizer already used by Orchid's compiler. Only graph DDL
// is parsed here; the Cypher body is forwarded unchanged to the existing parser.
fn tokens(sql: &str) -> Result<Vec<Token>, String> {
    use sqlparser::{
        dialect::{Dialect, GenericDialect, SnowflakeDialect},
        tokenizer::{Location, Token as SqlToken, Tokenizer},
    };
    use std::{any::TypeId, cell::Cell};
    // The tokenizer already implements // comments for Snowflake and backslash
    // escapes for other dialects. Select those lexical rules only in CYPHER
    // statements, preserving DuckDB's // integer division in ordinary SQL.
    #[derive(Debug, Default)]
    struct StatementDialect {
        cypher: Cell<bool>,
    }
    impl Dialect for StatementDialect {
        fn dialect(&self) -> TypeId {
            if self.cypher.get() {
                TypeId::of::<SnowflakeDialect>()
            } else {
                TypeId::of::<GenericDialect>()
            }
        }
        fn is_identifier_start(&self, c: char) -> bool {
            GenericDialect {}.is_identifier_start(c)
        }
        fn is_identifier_part(&self, c: char) -> bool {
            GenericDialect {}.is_identifier_part(c)
        }
        fn supports_nested_comments(&self) -> bool {
            true
        }
        fn supports_string_literal_backslash_escape(&self) -> bool {
            self.cypher.get()
        }
    }
    let mut lines = vec![0];
    for (i, c) in sql.char_indices() {
        if c == '\n' {
            lines.push(i + 1);
        }
    }
    let offset = |loc: Location| {
        let start = lines
            .get(loc.line.saturating_sub(1) as usize)
            .copied()
            .unwrap_or(sql.len());
        start
            + sql[start..]
                .char_indices()
                .nth(loc.column.saturating_sub(1) as usize)
                .map_or(sql.len() - start, |(i, _)| i)
    };
    let dialect = StatementDialect::default();
    let mut prefix = true;
    let mut lexed = vec![];
    Tokenizer::new(&dialect, sql)
        .tokenize_with_location_into_buf_with_mapper(&mut lexed, |token| {
            match &token.token {
                SqlToken::SemiColon => {
                    prefix = true;
                    dialect.cypher.set(false);
                }
                SqlToken::Whitespace(_) => (),
                SqlToken::Word(w)
                    if prefix
                        && w.quote_style.is_none()
                        && ["EXPLAIN", "ANALYZE"]
                            .contains(&w.value.to_ascii_uppercase().as_str()) =>
                {
                    ()
                }
                SqlToken::Word(w)
                    if prefix
                        && w.quote_style.is_none()
                        && ["CYPHER", "GREMLIN"]
                            .iter()
                            .any(|v| w.value.eq_ignore_ascii_case(v)) =>
                {
                    dialect.cypher.set(true);
                    prefix = false;
                }
                _ => prefix = false,
            }
            token
        })
        .map_err(|e| e.to_string())?;
    lexed
        .into_iter()
        .filter(|t| !matches!(t.token, SqlToken::Whitespace(_) | SqlToken::EOF))
        .map(|t| {
            let (value, kind) = match t.token {
                SqlToken::Word(w) => (w.value, if w.quote_style.is_some() { 'i' } else { 'w' }),
                SqlToken::Placeholder(p) => (p, '$'),
                SqlToken::SingleQuotedString(s) | SqlToken::DoubleQuotedString(s) => (s, 's'),
                SqlToken::DollarQuotedString(s) => (s.value, 's'),
                other => (other.to_string(), 'p'),
            };
            Ok(Token {
                value,
                kind,
                start: offset(t.span.start),
                end: offset(t.span.end),
            })
        })
        .collect()
}

struct Parser<'a> {
    sql: &'a str,
    tokens: &'a [Token],
    pos: usize,
}
impl Parser<'_> {
    fn take(&mut self, value: &str) -> bool {
        if self
            .tokens
            .get(self.pos)
            .is_some_and(|t| t.kind != 'i' && t.kind != 's' && t.value.eq_ignore_ascii_case(value))
        {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, value: &str) -> Result<(), String> {
        if self.take(value) {
            Ok(())
        } else {
            Err(format!(
                "expected {value} at byte {}",
                self.tokens.get(self.pos).map_or(0, |t| t.start)
            ))
        }
    }
    fn name(&mut self) -> Result<String, String> {
        let t = self
            .tokens
            .get(self.pos)
            .ok_or("expected identifier at end of statement")?;
        if !matches!(t.kind, 'w' | 'i') {
            return Err(format!("expected identifier at byte {}", t.start));
        }
        self.pos += 1;
        Ok(t.value.clone())
    }
    fn path(&mut self) -> Result<Vec<String>, String> {
        let mut parts = vec![self.name()?];
        while self.take(".") {
            parts.push(self.name()?);
        }
        if parts.len() > 3 {
            return Err("names support at most catalog.schema.table".into());
        }
        Ok(parts)
    }
    fn keys(&mut self) -> Result<Vec<String>, String> {
        self.expect("(")?;
        let mut keys = vec![self.name()?];
        while self.take(",") {
            keys.push(self.name()?);
        }
        self.expect(")")?;
        if keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
            return Err("duplicate key column".into());
        }
        Ok(keys)
    }
    fn element(&mut self, edge: bool) -> Result<Element, String> {
        let source = self.path()?;
        let alias = if self.take("AS") {
            self.name()?
        } else {
            source.last().unwrap().clone()
        };
        let mut e = Element {
            source,
            label: alias.clone(),
            alias,
            key: vec![],
            properties: None,
            from: None,
            to: None,
        };
        let mut seen = BTreeSet::new();
        loop {
            let clause = self
                .tokens
                .get(self.pos)
                .map(|t| t.value.to_ascii_uppercase())
                .unwrap_or_default();
            if !["KEY", "LABEL", "PROPERTIES", "NO", "SOURCE", "DESTINATION"]
                .contains(&clause.as_str())
            {
                break;
            }
            if !seen.insert(clause.clone()) {
                return Err(format!("duplicate {clause} clause"));
            }
            match clause.as_str() {
                "KEY" => {
                    self.pos += 1;
                    e.key = self.keys()?;
                }
                "LABEL" => {
                    self.pos += 1;
                    e.label = self.name()?;
                }
                "NO" => {
                    self.pos += 1;
                    self.expect("PROPERTIES")?;
                    e.properties = Some(BTreeMap::new());
                }
                "PROPERTIES" => {
                    self.pos += 1;
                    self.take("ARE");
                    if self.take("ALL") {
                        self.expect("COLUMNS")?;
                    } else {
                        self.expect("(")?;
                        let mut properties = BTreeMap::new();
                        loop {
                            let col = self.name()?;
                            let prop = if self.take("AS") {
                                self.name()?
                            } else {
                                col.clone()
                            };
                            if properties.insert(prop, col).is_some() {
                                return Err("duplicate property".into());
                            }
                            if !self.take(",") {
                                break;
                            }
                        }
                        self.expect(")")?;
                        e.properties = Some(properties);
                    }
                }
                "SOURCE" | "DESTINATION" if edge => {
                    self.pos += 1;
                    self.expect("KEY")?;
                    let columns = self.keys()?;
                    self.expect("REFERENCES")?;
                    let node = self.name()?;
                    let references = self.keys()?;
                    let endpoint = Endpoint {
                        columns,
                        node,
                        references,
                    };
                    if clause == "SOURCE" {
                        e.from = Some(endpoint);
                    } else {
                        e.to = Some(endpoint);
                    }
                }
                _ => return Err(format!("{clause} is only valid for edges")),
            }
        }
        if seen.contains("NO") && seen.contains("PROPERTIES") {
            return Err("conflicting property clauses".into());
        }
        if e.key.is_empty() {
            return Err(format!(
                "{} requires an explicit KEY (...) in this version",
                e.alias
            ));
        }
        if edge && (e.from.is_none() || e.to.is_none()) {
            return Err("edge requires SOURCE and DESTINATION".into());
        }
        Ok(e)
    }
    fn elements(&mut self, edge: bool) -> Result<Vec<Element>, String> {
        self.expect("(")?;
        let mut out = vec![self.element(edge)?];
        while self.take(",") {
            out.push(self.element(edge)?);
        }
        self.expect(")")?;
        Ok(out)
    }
    // Only separate DDL clauses here. Expression parsing, type checking, ranking,
    // function semantics and candidate planning remain in the shared compiler.
    fn expression_items(&mut self) -> Result<Vec<Vec<Token>>, String> {
        self.expect("(")?;
        let mut items = vec![];
        let mut start = self.pos;
        let mut nesting = 0;
        loop {
            let t = self.tokens.get(self.pos).ok_or("unterminated expression list")?;
            if t.kind == 'p' {
                if nesting == 0 && (t.value == "," || t.value == ")") {
                    if self.pos == start { return Err("expected expression".into()); }
                    items.push(self.tokens[start..self.pos].to_vec());
                    self.pos += 1;
                    if t.value == ")" { return Ok(items); }
                    start = self.pos;
                    continue;
                }
                match t.value.as_str() {
                    "(" | "[" | "{" => nesting += 1,
                    ")" | "]" | "}" => nesting -= 1,
                    _ => (),
                }
                if nesting < 0 { return Err("unbalanced expression".into()); }
            }
            self.pos += 1;
        }
    }
    fn expression_text(&self, tokens: &[Token]) -> Result<String, String> {
        let first = tokens.first().ok_or("expected expression")?;
        Ok(self.sql[first.start..tokens.last().unwrap().end].to_string())
    }
    fn computed_stage(&mut self, rule: &mut ComputedRelationship, candidate: bool) -> Result<(), String> {
        let mut seen = BTreeSet::new();
        loop {
            let clause = self.tokens.get(self.pos)
                .filter(|t| t.kind == 'w').map(|t| t.value.to_ascii_uppercase()).unwrap_or_default();
            if !["WHERE", "PROPERTIES", "ORDER", "LIMIT", "RETRIEVAL", "CANDIDATES"].contains(&clause.as_str()) { break; }
            if !seen.insert(clause.clone()) { return Err(format!("duplicate computed-edge {clause} clause")); }
            self.pos += 1;
            match clause.as_str() {
                "WHERE" => {
                    let expressions = self.expression_items()?;
                    if expressions.len() != 1 { return Err("WHERE requires one parenthesized predicate".into()); }
                    rule.predicate = Some(self.expression_text(&expressions[0])?);
                }
                "PROPERTIES" => {
                    for item in self.expression_items()? {
                        let alias = item.iter().rposition(|t| t.kind == 'w' && t.value.eq_ignore_ascii_case("AS"))
                            .ok_or("computed properties require expression AS name")?;
                        if alias + 2 != item.len() || !matches!(item[alias + 1].kind, 'w' | 'i') {
                            return Err("computed properties require expression AS name".into());
                        }
                        let name = item[alias + 1].value.clone();
                        if rule.properties.insert(name.clone(), self.expression_text(&item[..alias])?).is_some() {
                            return Err(format!("duplicate computed property {name}"));
                        }
                    }
                }
                "ORDER" => {
                    self.expect("BY")?;
                    for mut item in self.expression_items()? {
                        let keyword = |t: &Token, word: &str| t.kind == 'w' && t.value.eq_ignore_ascii_case(word);
                        let mut nulls = NullOrder::Last;
                        if item.len() >= 2 && keyword(&item[item.len()-2], "NULLS") {
                            nulls = if keyword(item.last().unwrap(), "FIRST") { NullOrder::First }
                                else if keyword(item.last().unwrap(), "LAST") { NullOrder::Last }
                                else { return Err("expected NULLS FIRST or LAST".into()); };
                            item.truncate(item.len()-2);
                        }
                        let mut direction = SortDirection::Asc;
                        if item.last().is_some_and(|t| keyword(t,"ASC") || keyword(t,"DESC")) {
                            if keyword(item.last().unwrap(), "DESC") { direction = SortDirection::Desc; }
                            item.pop();
                        }
                        rule.order_by.push(RelationshipOrder { expression: self.expression_text(&item)?, direction, nulls });
                    }
                }
                "LIMIT" => {
                    self.expect("PER")?;
                    self.expect("SOURCE")?;
                    let token = self.tokens.get(self.pos).ok_or("expected nonnegative per-source limit")?;
                    if token.kind != 'p' { return Err("expected nonnegative per-source limit".into()); }
                    rule.limit_per_source = Some(token.value.parse::<u64>().map_err(|_| "expected nonnegative per-source limit")?);
                    self.pos += 1;
                }
                "RETRIEVAL" if !candidate => {
                    rule.retrieval = if self.take("EXACT") { RetrievalMode::Exact }
                        else if self.take("APPROXIMATE_ALLOWED") { RetrievalMode::ApproximateAllowed }
                        else { return Err("expected EXACT or APPROXIMATE_ALLOWED".into()); };
                }
                "CANDIDATES" if !candidate => {
                    self.expect("(")?;
                    let mut stage = rule.clone();
                    stage.predicate = None; stage.properties.clear(); stage.order_by.clear(); stage.limit_per_source = None;
                    self.computed_stage(&mut stage, true)?;
                    self.expect(")")?;
                    rule.candidates = Some(RelationshipStage { predicate: stage.predicate, properties: stage.properties,
                        order_by: stage.order_by, limit_per_source: stage.limit_per_source.ok_or("CANDIDATES requires LIMIT PER SOURCE")? });
                }
                _ => return Err(format!("{clause} is not valid inside CANDIDATES")),
            }
        }
        Ok(())
    }
    fn computed_edges(&mut self) -> Result<Vec<ComputedRelationship>, String> {
        self.expect("(")?;
        let mut rules = vec![];
        loop {
            let name = self.name()?;
            self.expect("SOURCE")?;
            let source = self.name()?;
            self.expect("DESTINATION")?;
            let target = self.name()?;
            let mut rule = ComputedRelationship { name, source, target, predicate: None, properties: BTreeMap::new(),
                order_by: vec![], limit_per_source: None, retrieval: RetrievalMode::default(), candidates: None };
            self.computed_stage(&mut rule, false)?;
            rules.push(rule);
            if !self.take(",") { break; }
        }
        self.expect(")")?;
        Ok(rules)
    }
    fn end(&self) -> Result<(), String> {
        if self.pos == self.tokens.len() {
            Ok(())
        } else {
            Err(format!(
                "unexpected token at byte {}",
                self.tokens[self.pos].start
            ))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub version: u32,
    pub name: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_table: Option<String>,
    pub vertices: Vec<Element>,
    pub edges: Vec<Element>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub computed_relationships: Vec<ComputedRelationship>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub source: Vec<String>,
    pub alias: String,
    pub key: Vec<String>,
    pub label: String,
    pub properties: Option<BTreeMap<String, String>>,
    pub from: Option<Endpoint>,
    pub to: Option<Endpoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub columns: Vec<String>,
    pub node: String,
    pub references: Vec<String>,
}

impl Graph {
    pub fn mappings(&self, tables: &[Value]) -> Result<(Vec<Value>, Vec<Value>), String> {
        if self.version != 1 {
            return Err("unsupported graph definition version".into());
        }
        if self.vertices.is_empty() {
            return Err("a property graph requires at least one vertex source".into());
        }
        if tables.len() != self.vertices.len() + self.edges.len() {
            return Err("source schema count mismatch".into());
        }
        let mut aliases = BTreeSet::new();
        let mut labels = BTreeSet::new();
        let mut nodes = vec![];
        let mut edges = vec![];
        for (i, e) in self.vertices.iter().chain(&self.edges).enumerate() {
            if !aliases.insert(e.alias.to_lowercase()) {
                return Err(format!("duplicate element alias {}", e.alias));
            }
            if !labels.insert((i < self.vertices.len(), &e.label)) {
                return Err(format!("duplicate label {} is not supported yet", e.label));
            }
            let columns = tables[i]["columns"].as_array().ok_or("missing columns")?;
            let column = |name: &str| {
                columns
                    .iter()
                    .find(|c| {
                        c["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(name))
                    })
                    .ok_or_else(|| format!("unknown column {name} on {}", e.alias))
            };
            let names = |input: &[String]| -> Result<Vec<String>, String> {
                input
                    .iter()
                    .map(|name| Ok(column(name)?["name"].as_str().unwrap().to_string()))
                    .collect()
            };
            if e.key.is_empty() {
                return Err(format!("{} requires a key", e.alias));
            }
            let keys = names(&e.key)?;
            let mut props = e.properties.clone().unwrap_or_else(|| {
                columns
                    .iter()
                    .map(|c| {
                        let n = c["name"].as_str().unwrap().to_string();
                        (n.clone(), n)
                    })
                    .collect()
            });
            for c in props.values_mut() {
                *c = column(c)?["name"].as_str().unwrap().to_string();
            }
            if i < self.vertices.len() {
                nodes.push(json!({"label": e.label, "table": tables[i]["name"], "id": keys, "properties": props}));
            } else {
                let mut endpoints = vec![];
                for endpoint in [&e.from, &e.to] {
                    let ep = endpoint.as_ref().ok_or("missing edge endpoint")?;
                    let (n, node) = self
                        .vertices
                        .iter()
                        .enumerate()
                        .find(|(_, v)| v.alias.eq_ignore_ascii_case(&ep.node))
                        .ok_or_else(|| format!("unknown vertex alias {}", ep.node))?;
                    if node.key.len() != ep.references.len()
                        || !node
                            .key
                            .iter()
                            .zip(&ep.references)
                            .all(|(a, b)| a.eq_ignore_ascii_case(b))
                        || ep.columns.len() != ep.references.len()
                    {
                        return Err(format!(
                            "endpoint must reference the complete ordered key of {}",
                            ep.node
                        ));
                    }
                    for (src, dst) in ep.columns.iter().zip(&ep.references) {
                        let source_type = &column(src)?["data_type"];
                        let target_type = &tables[n]["columns"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|c| {
                                c["name"]
                                    .as_str()
                                    .is_some_and(|n| n.eq_ignore_ascii_case(dst))
                            })
                            .ok_or("missing referenced key")?["data_type"];
                        if source_type != target_type {
                            return Err(format!(
                                "endpoint type mismatch: {}.{src} -> {}.{dst}",
                                e.alias, ep.node
                            ));
                        }
                    }
                    endpoints.push((ep, node));
                }
                edges.push(json!({"label": e.label, "table": tables[i]["name"], "id": keys,
                    "source": names(&endpoints[0].0.columns)?, "source_label": endpoints[0].1.label,
                    "target": names(&endpoints[1].0.columns)?, "target_label": endpoints[1].1.label, "properties": props}));
            }
        }
        Ok((nodes, edges))
    }
}

fn rewrite_statement(sql: &str, ts: &[Token]) -> Result<Option<String>, String> {
    let mut p = Parser { sql, tokens: ts, pos: 0 };
    let explain = p.take("EXPLAIN");
    let analyze = explain && p.take("ANALYZE");
    let language = if p.take("CYPHER") {
        Some("cypher")
    } else if p.take("GREMLIN") {
        Some("gremlin")
    } else {
        None
    };
    if let Some(language) = language {
        let graph = p.path()?;
        let start = p
            .tokens
            .get(p.pos)
            .ok_or("expected Cypher after graph name")?
            .start;
        let end = ts.last().unwrap().end;
        let mut params = BTreeSet::new();
        for token in &ts[p.pos..] {
            if token.kind == '$' && token.value.starts_with('$') {
                params.insert(token.value[1..].to_string());
            }
        }
        let args = if params.is_empty() {
            String::new()
        } else {
            format!(
                ", parameters := struct_pack({})",
                params
                    .iter()
                    .map(|n| format!("{} := ${}", ident(n), ident(n)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        return Ok(Some(format!(
            "{}SELECT * FROM orchid_{}({}, {}{})",
            if analyze {
                "EXPLAIN ANALYZE "
            } else if explain {
                "EXPLAIN "
            } else {
                ""
            },
            language,
            literal(&qualified(&graph)),
            literal(&sql[start..end]),
            args
        )));
    }
    if explain {
        return Ok(None);
    }
    p.pos = 0;
    if p.take("CREATE") {
        let replace = if p.take("OR") {
            p.expect("REPLACE")?;
            true
        } else {
            false
        };
        if !p.take("PROPERTY") {
            return Ok(None);
        }
        p.expect("GRAPH")?;
        let if_missing = if p.take("IF") {
            p.expect("NOT")?;
            p.expect("EXISTS")?;
            true
        } else {
            false
        };
        if replace && if_missing {
            return Err("OR REPLACE cannot be combined with IF NOT EXISTS".into());
        }
        let name = p.path()?;
        let managed = p.take("MANAGED") || p.pos == p.tokens.len();
        if managed {
            p.end()?;
            if replace || if_missing {
                return Err(
                    "managed graph creation does not yet support OR REPLACE or IF NOT EXISTS"
                        .into(),
                );
            }
            return Ok(Some(format!(
                "CALL orchid_graph_create({})",
                literal(&qualified(&name))
            )));
        }
        p.expect("VERTEX")?;
        p.expect("TABLES")?;
        let vertices = p.elements(false)?;
        let edges = if p.take("EDGE") {
            p.expect("TABLES")?;
            p.elements(true)?
        } else {
            vec![]
        };
        let computed_relationships = if p.take("COMPUTED") {
            p.expect("EDGES")?;
            p.computed_edges()?
        } else { vec![] };
        p.end()?;
        let graph = Graph {
            version: 1,
            name: name.clone(),
            managed_table: None,
            vertices,
            edges,
            computed_relationships,
        };
        return Ok(Some(format!(
            "CREATE {}VIEW {}{} AS SELECT * FROM orchid_graph_definition({})",
            if replace { "OR REPLACE " } else { "" },
            if if_missing { "IF NOT EXISTS " } else { "" },
            graph_view(&name),
            literal(&serde_json::to_string(&graph).unwrap())
        )));
    }
    p.pos = 0;
    if p.take("DROP") && p.take("PROPERTY") {
        p.expect("GRAPH")?;
        let if_exists = if p.take("IF") {
            p.expect("EXISTS")?;
            true
        } else {
            false
        };
        let name = p.path()?;
        p.end()?;
        return Ok(Some(format!(
            "DROP VIEW {}{}",
            if if_exists { "IF EXISTS " } else { "" },
            graph_view(&name)
        )));
    }
    p.pos = 0;
    if (p.take("DESCRIBE") || p.take("DESC")) && p.take("PROPERTY") {
        p.expect("GRAPH")?;
        let name = p.path()?;
        p.end()?;
        return Ok(Some(format!(
            "SELECT * FROM orchid_graph_info({})",
            literal(&qualified(&name))
        )));
    }
    Ok(None)
}

pub fn rewrite(sql: &str) -> Result<Option<String>, String> {
    let ts = tokens(sql)?;
    let mut output = vec![];
    let mut changed = false;
    let mut begin = 0;
    let mut slice = 0;
    for end in 0..=ts.len() {
        if end != ts.len() && !(ts[end].kind == 'p' && ts[end].value == ";") {
            continue;
        }
        let char_end = if end == ts.len() {
            sql.len()
        } else {
            ts[end].start
        };
        if begin < end {
            if let Some(rewritten) = rewrite_statement(sql, &ts[begin..end])? {
                output.push(rewritten);
                changed = true;
            } else {
                output.push(sql[slice..char_end].to_string());
            }
        }
        slice = if end == ts.len() {
            sql.len()
        } else {
            ts[end].end
        };
        begin = end + 1;
    }
    Ok(changed.then(|| output.join(";\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_literals_comments_and_mixed_sql() {
        let s = rewrite("SELECT $$x;y$$; /* CYPHER fake */ CYPHER g MATCH (n) WHERE n.name = 'a;b' RETURN n.name; SELECT 3").unwrap().unwrap();
        assert!(s.contains("SELECT $$x;y$$"));
        assert!(s.contains("''a;b''"));
        assert!(s.contains("SELECT 3"));
        assert!(rewrite("SELECT 'CYPHER g RETURN 1'").unwrap().is_none());
        let s = rewrite("SELECT 8 // 2; CYPHER g // 'comment; $ignored\nRETURN 'it\\'s ok' AS x")
            .unwrap()
            .unwrap();
        assert!(s.contains("SELECT 8 // 2"));
        assert!(!s.contains("struct_pack"));
        assert!(s.contains("it\\''s ok"));
    }
    #[test]
    fn managed_declaration_reuses_graph_name_parser() {
        assert_eq!(
            rewrite("CREATE PROPERTY GRAPH g").unwrap().unwrap(),
            "CALL orchid_graph_create('\"g\"')"
        );
        assert_eq!(
            rewrite("CREATE PROPERTY GRAPH \"My Schema\".g MANAGED;")
                .unwrap()
                .unwrap(),
            "CALL orchid_graph_create('\"My Schema\".\"g\"')"
        );
        assert!(rewrite("CREATE PROPERTY GRAPH g MANAGED VERTEX TABLES (p)").is_err());
        assert!(rewrite("CREATE OR REPLACE PROPERTY GRAPH g").is_err());
    }
    #[test]
    fn ddl_and_parameters() {
        let s = rewrite("CREATE PROPERTY GRAPH g VERTEX TABLES (people KEY(id) LABEL Person) EDGE TABLES (links KEY(id) SOURCE KEY(src) REFERENCES people(id) DESTINATION KEY(dst) REFERENCES people(id) LABEL KNOWS)").unwrap().unwrap();
        assert!(s.starts_with("CREATE VIEW \"__orchid_graph_g\""));
        let s = rewrite("EXPLAIN CYPHER g MATCH (n) WHERE n.name=$name RETURN n.name")
            .unwrap()
            .unwrap();
        assert!(s.contains("parameters := struct_pack"));
        assert!(
            rewrite("CREATE PROPERTY GRAPH g VERTEX TABLES (p)")
                .unwrap_err()
                .contains("explicit KEY")
        );
    }
}
