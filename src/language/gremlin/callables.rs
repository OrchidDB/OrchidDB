//! Callable argument lowering in the production frontend. Only explicitly typed
//! lambdas (or Lambda constructors) contain executable code; string values do not.
use super::{
    parser::{GremlinParseError, Result, tokenize},
    semantics::GValue,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum GremlinBinding {
    Value(GValue),
    /// Trusted Gremlin-Groovy callback body, with optional explicit parameters.
    Lambda(String),
    Predicate {
        operator: String,
        value: GValue,
    },
}

// Encode data as grammar literals without evaluating it. Numeric suffixes and
// typed map keys retain Gremlin's value identity. Single quotes also keep bound
// strings inert if a vertex-program option is subsequently parsed by Groovy.
fn literal_source(value: &GValue) -> Option<String> {
    fn quote(value: &str) -> String {
        let mut out = String::from("'");
        for c in value.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '\'' => out.push_str("\\'"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('\'');
        out
    }
    fn sequence(values: &[GValue]) -> Option<String> {
        Some(
            values
                .iter()
                .map(literal_source)
                .collect::<Option<Vec<_>>>()?
                .join(","),
        )
    }
    Some(match value {
        GValue::Null => "null".into(),
        GValue::Bool(v) => v.to_string(),
        GValue::String(v) => quote(v),
        GValue::Int(v) => format!("{v}I"),
        GValue::Long(v) => format!("{v}L"),
        GValue::Byte(v) => format!("{v}B"),
        GValue::Short(v) => format!("{v}S"),
        GValue::BigInt(v) => format!("{v}N"),
        GValue::Float(v) if v.is_finite() => format!("{v:?}D"),
        GValue::Float32(v) if v.is_finite() => format!("{v:?}F"),
        GValue::BigDecimal(v) => format!(
            "{}M",
            if v.to_string().contains('.') {
                v.to_string()
            } else {
                format!("{v}.0")
            }
        ),
        GValue::DateTime(v) => format!("datetime({})", quote(v)),
        GValue::List(v) => format!("[{}]", sequence(v)?),
        GValue::Set(v) => format!("{{{}}}", sequence(v)?),
        GValue::Map(v) => {
            if v.is_empty() {
                "[:]".into()
            } else {
                format!(
                    "[{}]",
                    v.iter()
                        .map(|(k, v)| Some(format!("{}:{}", quote(k), literal_source(v)?)))
                        .collect::<Option<Vec<_>>>()?
                        .join(",")
                )
            }
        }
        GValue::TypedMap(v) => {
            if v.is_empty() {
                "[:]".into()
            } else {
                format!(
                    "[{}]",
                    v.iter()
                        .map(|(k, v)| Some(format!(
                            "{}:{}",
                            literal_source(k)?,
                            literal_source(v)?
                        )))
                        .collect::<Option<Vec<_>>>()?
                        .join(",")
                )
            }
        }
        GValue::Token(v) if matches!(v.as_str(), "id" | "label" | "key" | "value") => {
            format!("T.{v}")
        }
        GValue::Token(v)
            if matches!(
                v.as_str(),
                "Merge.outV" | "Merge.inV" | "Merge.onCreate" | "Merge.onMatch"
            ) =>
        {
            v.clone()
        }
        GValue::DirectionToken(v) if matches!(v.as_str(), "OUT" | "IN" | "BOTH") => {
            format!("Direction.{v}")
        }
        GValue::VertexRef { id, label } => {
            format!("new Vertex({}, {})", literal_source(id)?, quote(label))
        }
        _ => return None,
    })
}

pub(crate) fn prepare(
    input: &str,
    bindings: &HashMap<String, GremlinBinding>,
) -> Result<(String, HashMap<String, GValue>)> {
    let mut expanded = bindings.clone();
    let mut substitutions = HashMap::new();
    let mut values = HashMap::new();
    for (name, binding) in bindings {
        match binding {
            GremlinBinding::Value(value) => {
                values.insert(name.clone(), value.clone());
                if let Some(literal) = literal_source(value) {
                    substitutions.insert(name.clone(), literal);
                }
            }
            GremlinBinding::Predicate { operator, value } => {
                if !matches!(
                    operator.as_str(),
                    "eq" | "neq" | "lt" | "lte" | "gt" | "gte" | "within" | "without"
                ) {
                    return Err(GremlinParseError::Unsupported(format!(
                        "Predicate operator {operator}"
                    )));
                }
                let mut index = values.len();
                let mut temporary = format!("__crabgraph_parameter_{index}");
                while bindings.contains_key(&temporary)
                    || values.contains_key(&temporary)
                    || input.contains(&temporary)
                {
                    index += 1;
                    temporary = format!("__crabgraph_parameter_{index}");
                }
                values.insert(temporary.clone(), value.clone());
                substitutions.insert(
                    name.clone(),
                    format!(
                        "P.{operator}({})",
                        literal_source(value).unwrap_or(temporary)
                    ),
                );
                expanded.remove(name);
            }
            GremlinBinding::Lambda(_) => {}
        }
    }
    let tokens = tokenize(input)?;
    let input = tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            if index > 0 && tokens[index - 1].text == "." {
                return token.text.clone();
            }
            substitutions
                .get(&token.text)
                .cloned()
                .unwrap_or_else(|| token.text.clone())
        })
        .collect::<Vec<_>>()
        .join(" ");
    Ok((lower(&input, &expanded)?, values))
}

pub(crate) fn lower(input: &str, bindings: &HashMap<String, GremlinBinding>) -> Result<String> {
    let tokens = tokenize(input)?;
    let mut output = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let text = &tokens[i].text;
        if matches!(
            text.as_str(),
            "pageRank" | "peerPressure" | "connectedComponent" | "shortestPath"
        ) && i > 0
            && tokens[i - 1].text == "."
            && tokens.get(i + 1).is_some_and(|t| t.text == "(")
        {
            let mut end = i + 1;
            loop {
                let mut depth = 0;
                while end < tokens.len() {
                    match tokens[end].text.as_str() {
                        "(" => depth += 1,
                        ")" => depth -= 1,
                        _ => {}
                    }
                    end += 1;
                    if depth == 0 {
                        break;
                    }
                }
                if end + 2 < tokens.len()
                    && tokens[end].text == "."
                    && tokens[end + 1].text == "with"
                    && tokens[end + 2].text == "("
                {
                    end += 2;
                } else {
                    break;
                }
            }
            let operator = tokens[i..end]
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let script = format!("traversal.{operator}");
            let values = bindings
                .iter()
                .filter_map(|(name, value)| {
                    matches!(value, GremlinBinding::Value(_))
                        .then(|| format!("{}:{name}", serde_json::to_string(name).unwrap()))
                })
                .collect::<Vec<_>>()
                .join(",");
            let bound = if values.is_empty() {
                String::new()
            } else {
                format!(",'bindings':[{values}]")
            };
            output.push(format!(
                "call('crabgraph.jvm.computer',['script':{}{bound}])",
                serde_json::to_string(&script).unwrap()
            ));
            i = end;
            continue;
        }
        if text == "by" && tokens.get(i + 1).is_some_and(|t| t.text == "(") {
            let mut depth = 1;
            let mut end = i + 2;
            let mut comma = None;
            while end < tokens.len() && depth > 0 {
                match tokens[end].text.as_str() {
                    "(" | "[" | "{" => depth += 1,
                    ")" | "]" | "}" => depth -= 1,
                    "," if depth == 1 => comma = Some(end),
                    _ => {}
                }
                if depth > 0 {
                    end += 1;
                }
            }
            let argument = comma.map_or(i + 2, |j| j + 1);
            if end == argument + 1 {
                if let Some(GremlinBinding::Lambda(body)) = bindings.get(&tokens[argument].text) {
                    if body
                        .split_once("->")
                        .is_some_and(|(args, _)| args.contains(','))
                    {
                        let mut options =
                            format!("'script':{}", serde_json::to_string(body).unwrap());
                        if let Some(comma) = comma {
                            if comma != i + 3 {
                                return Err(GremlinParseError::Unsupported(
                                    "Comparator projection must be a property key".into(),
                                ));
                            }
                            let key = super::parser::decode_callable_string(&tokens[i + 2].text)?;
                            options += &format!(",'key':{}", serde_json::to_string(&key).unwrap());
                        }
                        output.push(format!(
                            "by(__.call('crabgraph.jvm.comparator',[{options}]))"
                        ));
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        let mut consumed = 1;
        let callable = if let Some(GremlinBinding::Lambda(body)) = bindings.get(text) {
            Some(body.clone())
        } else if text == "Lambda"
            && tokens.get(i + 1).is_some_and(|t| t.text == ".")
            && tokens.get(i + 3).is_some_and(|t| t.text == "(")
            && tokens.get(i + 5).is_some_and(|t| t.text == ")")
        {
            let constructor = tokens[i + 2].text.as_str();
            if !matches!(
                constructor,
                "function" | "predicate" | "consumer" | "biFunction" | "comparator"
            ) {
                return Err(GremlinParseError::Unsupported(format!(
                    "Lambda.{constructor}"
                )));
            }
            consumed = 6;
            Some(super::parser::decode_callable_string(&tokens[i + 4].text)?)
        } else {
            None
        };
        if let Some(body) = callable {
            let method = stack.last().map(String::as_str).unwrap_or("");
            let (mode, argument) = match method {
                "map" | "branch" => ("map", "__crabgraph_traverser"),
                "flatMap" => ("flatMap", "__crabgraph_traverser"),
                "filter" | "until" | "emit" => ("filter", "__crabgraph_traverser"),
                "choose" => ("filter", "current"),
                "by" => ("map", "current"),
                _ => {
                    return Err(GremlinParseError::Unsupported(format!(
                        "callable argument for {method}()"
                    )));
                }
            };
            if body
                .split_once("->")
                .is_some_and(|(args, _)| args.contains(','))
            {
                return Err(GremlinParseError::Unsupported(
                    "binary callable requires a comparator operator".into(),
                ));
            }
            let body = body.trim();
            let body = body
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or(body);
            let script = format!("({{{body}}}).call({argument})");
            // The original callback is encoded as a string argument. It cannot
            // introduce syntax into the surrounding traversal.
            output.push(format!(
                "__.call('crabgraph.jvm', ['script':{},'mode':'{mode}'])",
                serde_json::to_string(&script).unwrap()
            ));
        } else {
            if text == "(" {
                stack.push(
                    i.checked_sub(1)
                        .map(|j| tokens[j].text.clone())
                        .unwrap_or_default(),
                );
            } else if text == ")" {
                stack.pop();
            }
            output.push(text.clone());
        }
        i += consumed;
    }
    Ok(output.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bound_data_round_trips_in_literal_only_grammar_positions() {
        let value = GValue::TypedMap(vec![
            (GValue::Token("id".into()), GValue::Long(42)),
            (
                GValue::String("payload".into()),
                GValue::List(vec![
                    GValue::Byte(7),
                    GValue::Float32(1.25),
                    GValue::String("'\\\n${throw new Exception()}".into()),
                ]),
            ),
        ]);
        let bindings = HashMap::from([("data".into(), GremlinBinding::Value(value.clone()))]);
        let (source, values) = prepare("g.inject(data)", &bindings).unwrap();
        let traversal = super::super::parse_traversal_with_bindings(&source, &values).unwrap();
        assert_eq!(
            traversal.steps,
            vec![super::super::ast::Step::Inject(vec![value])]
        );
    }

    #[test]
    fn only_typed_code_is_lowered_and_strings_remain_data() {
        let values = HashMap::from([(
            "callback".into(),
            GremlinBinding::Lambda("it.get() + 1".into()),
        )]);
        let text = lower(
            "g.inject('callback', 'Lambda.function(42)').map(callback)",
            &values,
        )
        .unwrap();
        assert!(text.contains("'callback'"));
        assert!(text.contains("'Lambda.function(42)'"));
        assert!(text.contains("__crabgraph_traverser"));
        super::super::parse_traversal(&text).unwrap();
    }
    #[test]
    fn inline_constructor_is_a_callback_not_a_traversal_fallback() {
        let text = lower(
            "g.V().map(Lambda.function(\"it.get().value('name')\"))",
            &HashMap::new(),
        )
        .unwrap();
        let traversal = super::super::parse_traversal(&text).unwrap();
        let plan = super::super::GremlinPlanner::new()
            .plan(&traversal)
            .unwrap();
        let explain = crate::ir::explain(&plan);
        assert!(explain.contains("GraphJvm"));
        assert!(explain.contains("GraphNodeScan"));
    }
}
