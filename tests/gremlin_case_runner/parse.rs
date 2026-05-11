//! Thin entry point — delegates to `new_graph::language::gremlin::parser`.
//!
//! TinkerPop conformance cases reference vertices/edges by binding names
//! like `vid1` / `eid7`. The harness's modern dataset lays vertices out so
//! those bindings resolve to deterministic per-label row indices; we hand
//! that table to the parser via `parse_traversal_with_bindings`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;

use new_graph::language::gremlin::Traversal;
use new_graph::language::gremlin::parser::parse_traversal_with_bindings;
use new_graph::language::gremlin::semantics::GValue;

pub fn gremlin_with_case(source: &str, source_case: &str) -> Result<Traversal, String> {
    let source = normalize_imported_source(source);
    let mut bindings = modern_bindings().clone();
    add_case_bindings(&mut bindings, &source, source_case);
    parse_traversal_with_bindings(&source, &bindings).map_err(|err| format!("{err}"))
}

fn normalize_imported_source(source: &str) -> Cow<'_, str> {
    let uncommented = uncomment_imported_query(source);
    replace_unbound_side_effect_seed(uncommented)
}

fn uncomment_imported_query(source: &str) -> Cow<'_, str> {
    let mut changed = false;
    let lines: Vec<&str> = source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(uncommented) = trimmed.strip_prefix('#') {
                changed = true;
                uncommented.trim_start()
            } else {
                line
            }
        })
        .collect();
    if changed {
        Cow::Owned(lines.join("\n"))
    } else {
        Cow::Borrowed(source)
    }
}

fn replace_unbound_side_effect_seed(source: Cow<'_, str>) -> Cow<'_, str> {
    let mut out = String::new();
    let mut rest = source.as_ref();
    let mut changed = false;
    while let Some(call_start) = rest.find("withSideEffect(") {
        let (before, after_start) = rest.split_at(call_start);
        out.push_str(before);
        let Some(call_end) = find_call_end(after_start) else {
            out.push_str(after_start);
            return if changed { Cow::Owned(out) } else { source };
        };
        let call = &after_start[..=call_end];
        if let Some(rewritten) = replace_second_arg_xx_seed(call) {
            out.push_str(&rewritten);
            changed = true;
        } else {
            out.push_str(call);
        }
        rest = &after_start[call_end + 1..];
    }
    if changed {
        out.push_str(rest);
        Cow::Owned(out)
    } else {
        source
    }
}

fn find_call_end(source: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in source.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn replace_second_arg_xx_seed(call: &str) -> Option<String> {
    let body = call.strip_prefix("withSideEffect(")?.strip_suffix(')')?;
    let comma = top_level_comma(body)?;
    let after_comma = &body[comma + 1..];
    let value_start = after_comma.len() - after_comma.trim_start().len();
    let value = &after_comma[value_start..];
    let value_end = value.find(',').unwrap_or(value.len());
    let candidate = value[..value_end].trim();
    if !is_xx_var(candidate) {
        return None;
    }
    let mut rewritten = String::from("withSideEffect(");
    rewritten.push_str(&body[..comma + 1]);
    rewritten.push_str(&after_comma[..value_start]);
    rewritten.push_str("{}");
    rewritten.push_str(&value[value_end..]);
    rewritten.push(')');
    Some(rewritten)
}

fn top_level_comma(source: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in source.char_indices() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            ',' => return Some(idx),
            _ => {}
        }
    }
    None
}

fn is_xx_var(source: &str) -> bool {
    let Some(digits) = source.strip_prefix("xx") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

// Synthetic ids exposed to gremlin queries. They include the label/type
// because the IR catalog stores ids per label/relation; a bare row id like
// `1` would otherwise match both `person#1` and `software#1`.
pub const MODERN_VID1: &str = "person#0"; // marko
pub const MODERN_VID2: &str = "person#1"; // vadas
pub const MODERN_VID3: &str = "software#0"; // lop
pub const MODERN_VID4: &str = "person#2"; // josh
pub const MODERN_VID5: &str = "software#1"; // ripple
pub const MODERN_VID6: &str = "person#3"; // peter

pub const MODERN_EID7: &str = "knows#0"; // marko KNOWS vadas
pub const MODERN_EID8: &str = "knows#1"; // marko KNOWS josh
pub const MODERN_EID9: &str = "created#0"; // marko CREATED lop
pub const MODERN_EID10: &str = "created#1"; // josh CREATED ripple
pub const MODERN_EID11: &str = "created#2"; // josh CREATED lop
pub const MODERN_EID12: &str = "created#3"; // peter CREATED lop

fn modern_bindings() -> &'static HashMap<String, GValue> {
    static BINDINGS: OnceLock<HashMap<String, GValue>> = OnceLock::new();
    BINDINGS.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, id) in [
            ("vid1", MODERN_VID1),
            ("vid2", MODERN_VID2),
            ("vid3", MODERN_VID3),
            ("vid4", MODERN_VID4),
            ("vid5", MODERN_VID5),
            ("vid6", MODERN_VID6),
            ("eid7", MODERN_EID7),
            ("eid8", MODERN_EID8),
            ("eid9", MODERN_EID9),
            ("eid10", MODERN_EID10),
            ("eid11", MODERN_EID11),
            ("eid12", MODERN_EID12),
        ] {
            map.insert(name.to_string(), GValue::String(id.to_string()));
        }
        map
    })
}

fn add_case_bindings(bindings: &mut HashMap<String, GValue>, source: &str, source_case: &str) {
    add_direct_case_bindings(bindings, source, source_case);

    let xx_vars = xx_vars_in_order(source);
    let values = source_case_values(source_case);
    let mut values = values.into_iter();
    for name in xx_vars {
        match (bindings.get(&name), values.next()) {
            (Some(existing), Some(value)) if existing == &value => {}
            (Some(_), Some(_)) => {}
            (Some(_), None) => {}
            (None, Some(value)) => {
                bindings.insert(name, value);
            }
            (None, None) => break,
        }
    }
}

fn add_direct_case_bindings(
    bindings: &mut HashMap<String, GValue>,
    source: &str,
    source_case: &str,
) {
    bind_direct_vertex_source(bindings, source, source_case);
    bind_direct_edge_source(bindings, source, source_case);
    bind_direct_has_id(bindings, source, source_case);
}

fn bind_direct_vertex_source(
    bindings: &mut HashMap<String, GValue>,
    source: &str,
    source_case: &str,
) {
    let Some(name) = first_call_xx_var(source, "g.V(") else {
        return;
    };
    if bindings.contains_key(&name) {
        return;
    }
    if let Some(token) = source_case
        .strip_prefix("g_VX")
        .and_then(|rest| rest.split_once('X'))
        .map(|(token, _)| token)
        .filter(|token| *token != "list")
    {
        if let Some(value) = vertex_id_value(token) {
            bindings.insert(name, value);
        }
    } else if let Some(list) = extract_list_after(source_case, "g_VXlistX") {
        bindings.insert(name, GValue::List(parse_id_list(list, IdKind::Vertex)));
    }
}

fn bind_direct_edge_source(
    bindings: &mut HashMap<String, GValue>,
    source: &str,
    source_case: &str,
) {
    let name = first_call_xx_var(source, "g.E(").or_else(|| first_call_xx_var(source, ".E("));
    let Some(name) = name else {
        return;
    };
    if bindings.contains_key(&name) {
        return;
    }
    if let Some(list) = extract_list_after(source_case, "g_EXlistX") {
        bindings.insert(name, GValue::List(parse_id_list(list, IdKind::Edge)));
    }
}

fn bind_direct_has_id(bindings: &mut HashMap<String, GValue>, source: &str, source_case: &str) {
    let Some(name) = first_xx_after(source, "hasId(") else {
        return;
    };
    if bindings.contains_key(&name) {
        return;
    }
    if let Some(token) = extract_after(source_case, "hasIdX") {
        match token {
            "empty" => {
                bindings.insert(name, GValue::List(Vec::new()));
            }
            "neq" => {
                if let Some(value) = extract_after(source_case, "neqX").and_then(vertex_id_value) {
                    bindings.insert(name, value);
                }
            }
            _ => {
                bindings.insert(name, GValue::List(parse_id_list(token, IdKind::Vertex)));
            }
        }
    } else if let Some(token) = extract_after(source_case, "hasXid_") {
        bindings.insert(name, GValue::List(parse_id_list(token, IdKind::Vertex)));
    }
}

fn source_case_values(source_case: &str) -> Vec<GValue> {
    let mut values = Vec::new();
    let mut i = 0;
    while i < source_case.len() {
        let rest = &source_case[i..];
        if let Some(list) = rest
            .strip_prefix("listX")
            .and_then(|after| after.split_once('X'))
            .map(|(list, _)| list)
        {
            let kind = if list.split('_').any(|item| item.starts_with('e')) {
                IdKind::Edge
            } else {
                IdKind::Vertex
            };
            values.push(GValue::List(parse_id_list(list, kind)));
            i += "listX".len() + list.len() + 1;
            continue;
        }
        if let Some(token) = rest
            .strip_prefix("optionX")
            .and_then(|after| split_case_token(after))
        {
            if let Some(value) = parse_case_numeric_value(token) {
                values.push(value);
            }
            i += "optionX".len() + token.len();
            continue;
        }
        if let Some(token) = rest
            .strip_prefix("isX")
            .and_then(|after| split_case_token(after))
        {
            if let Some(value) = parse_case_numeric_value(token) {
                values.push(value);
            }
            i += "isX".len() + token.len();
            continue;
        }
        if rest.starts_with("var") || rest.starts_with("vaar") {
            let suffix_len = if rest.starts_with("vaar") { 4 } else { 3 };
            if let Some(token) = preceding_case_token(source_case, i) {
                if let Some(value) = parse_case_value(token) {
                    values.push(value);
                }
            }
            i += suffix_len;
            continue;
        }
        i += 1;
    }
    values
}

fn xx_vars_in_order(source: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i] == b'x' && bytes[i + 1] == b'x' && bytes[i + 2].is_ascii_digit() {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            vars.push(source[start..i].to_string());
        } else {
            i += 1;
        }
    }
    vars
}

fn first_call_xx_var(source: &str, call: &str) -> Option<String> {
    let start = source.find(call)? + call.len();
    let rest = &source[start..];
    xx_var_at_start(rest)
}

fn first_xx_after(source: &str, marker: &str) -> Option<String> {
    let start = source.find(marker)? + marker.len();
    let rest = &source[start..];
    let xx_start = rest.find("xx")?;
    xx_var_at_start(&rest[xx_start..])
}

fn xx_var_at_start(source: &str) -> Option<String> {
    let name_len = source
        .strip_prefix("xx")?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count()
        + 2;
    Some(source[..name_len].to_string())
}

fn extract_after<'a>(source_case: &'a str, prefix: &str) -> Option<&'a str> {
    let start = source_case.find(prefix)? + prefix.len();
    split_case_token(&source_case[start..])
}

fn extract_list_after<'a>(source_case: &'a str, prefix: &str) -> Option<&'a str> {
    let start = source_case.find(prefix)? + prefix.len();
    source_case[start..].split_once('X').map(|(token, _)| token)
}

fn split_case_token(input: &str) -> Option<&str> {
    let end = input
        .find("__")
        .or_else(|| input.find('X'))
        .or_else(|| input.find('_'))
        .unwrap_or(input.len());
    (end > 0).then_some(&input[..end])
}

fn preceding_case_token(source_case: &str, suffix_start: usize) -> Option<&str> {
    let start = source_case[..suffix_start]
        .rfind(['_', 'X'])
        .map(|idx| idx + 1)
        .unwrap_or(0);
    (start < suffix_start).then_some(&source_case[start..suffix_start])
}

fn parse_case_value(token: &str) -> Option<GValue> {
    let token = token.trim_end_matches(['L', 'l', 'i']);
    if token.is_empty() || matches!(token, "none" | "any") {
        return None;
    }
    if token == "null" {
        return Some(GValue::Null);
    }
    if let Ok(value) = token.parse::<i64>() {
        return Some(GValue::Int(value));
    }
    Some(GValue::String(token.to_string()))
}

fn parse_case_numeric_value(token: &str) -> Option<GValue> {
    let token = token.trim_end_matches(['L', 'l', 'i']);
    token.parse::<i64>().ok().map(GValue::Int)
}

#[derive(Clone, Copy)]
enum IdKind {
    Vertex,
    Edge,
}

fn parse_id_list(list: &str, kind: IdKind) -> Vec<GValue> {
    if list == "empty" {
        return Vec::new();
    }
    list.split('_')
        .filter_map(|item| match item {
            "null" => Some(GValue::Null),
            _ => id_value(item, kind),
        })
        .collect()
}

fn vertex_id_value(token: &str) -> Option<GValue> {
    id_value(token, IdKind::Vertex)
}

fn id_value(token: &str, kind: IdKind) -> Option<GValue> {
    let num = token
        .strip_prefix(['v', 'e'])
        .unwrap_or(token)
        .parse::<usize>()
        .ok()?;
    let id = match kind {
        IdKind::Vertex => modern_vid(num)?,
        IdKind::Edge => modern_eid(num)?,
    };
    Some(GValue::String(id.to_string()))
}

fn modern_vid(num: usize) -> Option<&'static str> {
    match num {
        1 => Some(MODERN_VID1),
        2 => Some(MODERN_VID2),
        3 => Some(MODERN_VID3),
        4 => Some(MODERN_VID4),
        5 => Some(MODERN_VID5),
        6 => Some(MODERN_VID6),
        _ => None,
    }
}

fn modern_eid(num: usize) -> Option<&'static str> {
    match num {
        7 => Some(MODERN_EID7),
        8 => Some(MODERN_EID8),
        9 => Some(MODERN_EID9),
        10 => Some(MODERN_EID10),
        11 => Some(MODERN_EID11),
        12 => Some(MODERN_EID12),
        _ => None,
    }
}
