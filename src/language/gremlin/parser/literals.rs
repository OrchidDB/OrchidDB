//! Raw literal, token, and embedded expression decoding.

use antlr4rust::tree::ParseTree;

use super::{
    BySpec, Direction, FormatPart, GValue, GremlinError, MapColumn, MathExpr, OptionKey, Pop, Rc,
    Result, SackOp, SortDir, Step,
};
use crate::grammar::generated::gremlin::gremlinparser::*;
/// Resolve a `traversalOperator` token's raw text to our `SackOp` enum.
/// Accepts both bare keywords (`sum`, `mult`, ...) and the qualified
/// `Operator.X` form. Returns `None` for tokens we don't model.
pub(super) fn sack_op_from_text(raw: &str) -> Option<SackOp> {
    let trimmed = raw.trim();
    let bare = trimmed
        .rsplit_once('.')
        .map(|(_, tail)| tail)
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    Some(match bare.as_str() {
        "sum" => SackOp::Sum,
        "sumlong" => SackOp::SumLong,
        "minus" => SackOp::Minus,
        "mult" => SackOp::Mult,
        "div" => SackOp::Div,
        "min" => SackOp::Min,
        "max" => SackOp::Max,
        "assign" => SackOp::Assign,
        "and" => SackOp::And,
        "or" => SackOp::Or,
        "addall" => SackOp::AddAll,
        _ => return None,
    })
}

/// Maps the raw text of a `traversalPop` (`Pop.first` / `first` /
/// `Pop.last` / `last` / `Pop.all` / `all` / `Pop.mixed` / `mixed`) to
/// our `Pop` discriminant. `all` returns a list of every binding; `mixed`
/// returns a list when there are 2+ bindings, otherwise the scalar.
pub(super) fn pop_from_text(raw: Option<String>) -> Pop {
    let Some(raw) = raw else { return Pop::Last };
    let lower = raw.to_lowercase();
    if lower.contains("mixed") {
        Pop::Mixed
    } else if lower.contains("all") {
        Pop::All
    } else if lower.contains("first") {
        Pop::First
    } else {
        Pop::Last
    }
}

pub(super) fn select_label_from_constant_traversal(steps: &[Step]) -> Option<String> {
    match steps {
        [Step::Constant(GValue::String(label))] => Some(label.clone()),
        _ => None,
    }
}

pub(super) fn constant_value_from_steps(steps: &[Step]) -> Option<GValue> {
    match steps {
        [Step::Constant(value)] => Some(value.clone()),
        _ => None,
    }
}

pub(super) fn map_column_from_text(raw: &str) -> Option<MapColumn> {
    let token = raw
        .trim()
        .strip_prefix("Column.")
        .unwrap_or(raw.trim())
        .to_ascii_lowercase();
    match token.as_str() {
        "keys" => Some(MapColumn::Keys),
        "values" => Some(MapColumn::Values),
        _ => None,
    }
}

/// Reads the raw text of a `traversalComparator` (which always distils to
/// a `traversalOrder` keyword like `Order.desc` / `Order.asc` /
/// `Order.shuffle`) and decides on a sort direction. Anything we don't
/// recognise (notably `shuffle`) falls back to ascending.
pub(super) fn comparator_direction<'input>(
    ctx: Option<Rc<TraversalComparatorContextAll<'input>>>,
) -> SortDir {
    let Some(ctx) = ctx else { return SortDir::Asc };
    order_token_direction(&Some(ctx.get_text()))
}

pub(super) fn order_token_direction(raw: &Option<String>) -> SortDir {
    match raw {
        Some(s) if s.to_lowercase().contains("desc") => SortDir::Desc,
        _ => SortDir::Asc,
    }
}

pub(super) fn by_spec_from_raw_text(raw: &str) -> BySpec {
    let mut spec = if raw.contains("T.key") || raw.contains("Column.keys") || raw.contains("keys") {
        BySpec::key("key")
    } else if raw.contains("T.value") || raw.contains("Column.values") || raw.contains("values") {
        BySpec::key("value")
    } else if raw.contains("T.id") || raw.contains("id()") {
        BySpec::key("id")
    } else if raw.contains("T.label") || raw.contains("label()") {
        BySpec::key("label")
    } else {
        BySpec::default()
    };
    if raw.to_ascii_lowercase().contains("desc") {
        spec.direction = SortDir::Desc;
    }
    spec
}

pub(super) fn direction_from_to_arg(raw: &str) -> Option<Direction> {
    if raw.contains("Direction.OUT") || raw.contains("OUT") {
        Some(Direction::Out)
    } else if raw.contains("Direction.IN") || raw.contains("IN") {
        Some(Direction::In)
    } else if raw.contains("Direction.BOTH") || raw.contains("BOTH") {
        Some(Direction::Both)
    } else {
        None
    }
}

/// Parses a Gremlin `format()` template into a sequence of literal
/// segments and placeholders. Recognises `{N}`-indexed Gremlin
/// placeholders and the `%s` printf shorthand. Other `%` escapes are
/// preserved as literals.
pub(super) fn parse_format_template(raw: &str) -> Vec<FormatPart> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                // Match `{<digits>}` placeholders.
                let mut idx = String::new();
                while let Some(&peek) = chars.peek() {
                    if peek.is_ascii_digit() {
                        idx.push(peek);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&'}') && !idx.is_empty() {
                    chars.next(); // consume '}'
                    if !buf.is_empty() {
                        parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                    }
                    parts.push(FormatPart::Placeholder { key: None });
                } else {
                    // Not a placeholder — treat as literal.
                    buf.push('{');
                    buf.push_str(&idx);
                }
            }
            // `%{name}` — TinkerPop's named placeholder; the key resolves
            // to a property on the current element or a labelled binding.
            // Bare `%{_}` (or empty) defers to the matching by(...) modulator.
            '%' if chars.peek() == Some(&'{') => {
                chars.next(); // consume '{'
                let mut key = String::new();
                let mut closed = false;
                while let Some(&peek) = chars.peek() {
                    if peek == '}' {
                        chars.next();
                        closed = true;
                        break;
                    }
                    key.push(peek);
                    chars.next();
                }
                if closed {
                    if !buf.is_empty() {
                        parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                    }
                    let key_opt = if key.is_empty() || key == "_" {
                        None
                    } else {
                        Some(key)
                    };
                    parts.push(FormatPart::Placeholder { key: key_opt });
                } else {
                    // Unterminated — emit as literal text.
                    buf.push('%');
                    buf.push('{');
                    buf.push_str(&key);
                }
            }
            '%' if chars.peek() == Some(&'s') => {
                chars.next();
                if !buf.is_empty() {
                    parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                }
                parts.push(FormatPart::Placeholder { key: None });
            }
            other => buf.push(other),
        }
    }
    if !buf.is_empty() {
        parts.push(FormatPart::Literal(buf));
    }
    parts
}

/// Maps a Gremlin Direction token to our internal `Direction` enum. The
/// keyword forms (`Direction.OUT`, `OUT`, `Direction.IN`, `IN`, `from`,
/// `to`, ...) all show up at parse time as the textual representation of
/// the matched alternative; we just check for the relevant keyword
/// substring.
pub(super) fn direction_from_text(raw: &str) -> Direction {
    let s = raw.to_uppercase();
    if s.contains("OUT") || s.ends_with("FROM") || s.contains(".FROM") {
        Direction::Out
    } else if s.contains("IN") || s.ends_with("TO") || s.contains(".TO") {
        Direction::In
    } else {
        Direction::Both
    }
}

/// Parses a Gremlin `math()` expression. We model the common shapes:
///   * `_ OP literal` / `literal OP _` (`_ + 1`, `2 - _`, ...)
///   * `_ OP _` (binary on self / by-modulator pair)
///   * `_ OP name` / `name OP _` (self combined with a named binding /
///     side-effect / property — resolved at the planner)
///   * `name OP name` (both operands are named bindings)
///   * bare `name` (single named binding, no op)
/// Anything else (e.g. `sin _`, parenthesised expressions, multi-op
/// chains) falls back to `MathExpr::Identity`.
pub(super) fn parse_math_expr(raw: &str) -> MathExpr {
    use crate::language::gremlin::ast::MathOp;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return MathExpr::Identity;
    }
    if trimmed == "_" {
        return MathExpr::Identity;
    }
    if let Some((func, operand)) = parse_unary_math_call(trimmed) {
        if is_supported_unary_math_func(func) {
            let func = func.to_ascii_lowercase();
            if operand == "_" {
                return MathExpr::UnaryFn(func);
            }
            match parse_math_expr(operand) {
                MathExpr::Add(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Add,
                        value,
                    };
                }
                MathExpr::Sub(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Sub,
                        value,
                    };
                }
                MathExpr::Mul(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Mul,
                        value,
                    };
                }
                MathExpr::Div(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Div,
                        value,
                    };
                }
                _ => {}
            }
        }
    }
    if is_simple_math_name(trimmed) {
        return MathExpr::Var(trimmed.to_string());
    }
    // Tokenise into [LHS] [OP] [RHS] at the first non-leading binary op.
    fn tokenise(s: &str) -> Option<(&str, char, &str)> {
        for (i, c) in s.char_indices() {
            if matches!(c, '+' | '-' | '*' | '/') && i > 0 {
                // Skip leading sign when the prior char is also an op (e.g.
                // `_*-2`); detect by walking back to find a non-space.
                let prev = s[..i].chars().rev().find(|c| !c.is_whitespace());
                if matches!(prev, Some('+' | '-' | '*' | '/')) {
                    continue;
                }
                let lhs = s[..i].trim();
                let rhs = s[i + 1..].trim();
                if !lhs.is_empty() && !rhs.is_empty() {
                    return Some((lhs, c, rhs));
                }
            }
        }
        None
    }
    let (lhs, op_char, rhs) = match tokenise(trimmed) {
        Some(t) => t,
        None => return MathExpr::Identity,
    };
    let op = match op_char {
        '+' => MathOp::Add,
        '-' => MathOp::Sub,
        '*' => MathOp::Mul,
        '/' => MathOp::Div,
        _ => return MathExpr::Identity,
    };
    let lhs_is_self = lhs == "_";
    let rhs_is_self = rhs == "_";
    let lhs_lit = lhs.parse::<f64>().ok();
    let rhs_lit = rhs.parse::<f64>().ok();
    let lhs_name = is_simple_math_name(lhs).then(|| lhs.to_string());
    let rhs_name = is_simple_math_name(rhs).then(|| rhs.to_string());
    match (lhs_is_self, rhs_is_self) {
        (true, true) => MathExpr::BinSelf(op),
        (true, false) => {
            if let Some(v) = rhs_lit {
                return match op {
                    MathOp::Add => MathExpr::Add(v),
                    MathOp::Sub => MathExpr::Sub(v),
                    MathOp::Mul => MathExpr::Mul(v),
                    MathOp::Div => MathExpr::Div(v),
                };
            }
            if let Some(name) = rhs_name {
                return MathExpr::SelfRhsName(op, name);
            }
            MathExpr::Identity
        }
        (false, true) => {
            if let Some(v) = lhs_lit {
                return match op {
                    MathOp::Add => MathExpr::Add(v), // commutative
                    MathOp::Sub => MathExpr::SubFromLit(v),
                    MathOp::Mul => MathExpr::Mul(v), // commutative
                    MathOp::Div => MathExpr::DivByLit(v),
                };
            }
            if let Some(name) = lhs_name {
                return MathExpr::SelfLhsName(op, name);
            }
            MathExpr::Identity
        }
        (false, false) => match (lhs_name, rhs_name, lhs_lit, rhs_lit) {
            (Some(a), Some(b), _, _) => MathExpr::BothNamed(op, a, b),
            (Some(a), _, _, Some(b)) => MathExpr::NameRhsLit(op, a, b),
            (_, Some(b), Some(a), _) => MathExpr::LitRhsName(op, a, b),
            _ => MathExpr::Identity,
        },
    }
}

pub(super) fn parse_unary_math_call(s: &str) -> Option<(&str, &str)> {
    if let Some((func, operand)) = s.split_once(' ') {
        let func = func.trim();
        let operand = operand.trim();
        if !func.is_empty() && !operand.is_empty() {
            return Some((func, operand));
        }
    }
    let open = s.find('(')?;
    let func = s[..open].trim();
    let operand = s[open + 1..].strip_suffix(')')?.trim();
    if func.is_empty() || operand.is_empty() {
        None
    } else {
        Some((func, operand))
    }
}

pub(super) fn is_supported_unary_math_func(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "abs"
            | "ceil"
            | "floor"
            | "round"
            | "sqrt"
            | "cbrt"
            | "sign"
            | "exp"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
    )
}

/// Returns true if `s` is a simple identifier suitable for a `math()`
/// expression operand: a leading letter or `_`, then alphanumerics,
/// underscores, or `.` (for namespaced bindings like `a.b`).
pub(super) fn is_simple_math_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

pub(super) fn parse_integer_literal_signed_unsigned<'input>(
    ctx: &IntegerLiteralContext<'input>,
    step: &str,
) -> Result<u64> {
    let text = ctx.get_text();
    let parsed = parse_integer_literal(&text)?;
    if parsed < 0 {
        return Err(GremlinError::Parse(format!(
            "{step}() expected non-negative integer, got {parsed}"
        )));
    }
    Ok(parsed as u64)
}

pub(super) fn parse_integer_literal(raw: &str) -> Result<i64> {
    let mut value = strip_numeric_suffix(raw, "bBsSnNiIlL").replace('_', "");
    let sign = if let Some(rest) = value.strip_prefix('-') {
        value = rest.to_string();
        -1i64
    } else if let Some(rest) = value.strip_prefix('+') {
        value = rest.to_string();
        1i64
    } else {
        1i64
    };

    let (radix, digits) = if let Some(rest) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (16, rest)
    } else if value.len() > 1 && value.starts_with('0') {
        (8, value.as_str())
    } else {
        (10, value.as_str())
    };

    i64::from_str_radix(digits, radix)
        .map(|parsed| parsed * sign)
        .map_err(|err| GremlinError::Parse(format!("invalid integer literal `{raw}`: {err}")))
}

pub(super) fn date_unit_from_text(raw: &str) -> String {
    raw.rsplit('.').next().unwrap_or(raw).to_ascii_lowercase()
}

pub(super) fn parse_date_literal_ctx<'input>(ctx: &DateLiteralContext<'input>) -> Option<String> {
    let literal = ctx.stringLiteral()?;
    decode_string_literal(&literal.get_text()).ok()
}

pub(super) fn date_diff_traversal_arg<'input>(
    ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
) -> GValue {
    let Some(nested) = ctx.nestedTraversal() else {
        return GValue::Null;
    };
    let text = nested.get_text();
    if text.contains("constant(null)") {
        return GValue::Null;
    }
    if text.contains("inject(") {
        return GValue::String("__current_datetime__".to_string());
    }
    extract_datetime_literal_arg(&text)
        .map(GValue::DateTime)
        .unwrap_or(GValue::Null)
}

pub(super) fn extract_datetime_literal_arg(raw: &str) -> Option<String> {
    let open = raw
        .find("datetime(")
        .map(|idx| idx + "datetime(".len())
        .or_else(|| raw.find("DateTime(").map(|idx| idx + "DateTime(".len()))?;
    let rest = raw.get(open..)?;
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut escaped = false;
    let mut out = String::new();
    for ch in rest[quote.len_utf8()..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

pub(super) fn parse_float_literal(raw: &str) -> Result<f64> {
    let value = strip_numeric_suffix(raw, "fFdDmM").replace('_', "");
    if value == "Infinity" || value == "+Infinity" {
        Ok(f64::INFINITY)
    } else if value == "-Infinity" {
        Ok(f64::NEG_INFINITY)
    } else if value == "NaN" {
        Ok(f64::NAN)
    } else {
        value
            .parse::<f64>()
            .map_err(|err| GremlinError::Parse(format!("invalid floating literal `{raw}`: {err}")))
    }
}

pub(super) fn strip_numeric_suffix<'a>(raw: &'a str, suffixes: &str) -> &'a str {
    raw.char_indices()
        .last()
        .and_then(|(idx, ch)| suffixes.contains(ch).then_some(&raw[..idx]))
        .unwrap_or(raw)
}

pub(super) fn decode_string_literal(raw: &str) -> Result<String> {
    let quote = raw
        .chars()
        .next()
        .ok_or_else(|| GremlinError::Parse("empty string token".to_string()))?;
    if quote != '\'' && quote != '"' {
        return Err(GremlinError::Parse(format!(
            "expected string literal, got `{raw}`"
        )));
    }
    if !raw.ends_with(quote) {
        return Err(GremlinError::Parse(format!(
            "unterminated string literal `{raw}`"
        )));
    }

    let inner = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    let mut decoded = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| GremlinError::Parse(format!("invalid escape in `{raw}`")))?;
        match escaped {
            'b' => decoded.push('\u{0008}'),
            't' => decoded.push('\t'),
            'n' => decoded.push('\n'),
            'f' => decoded.push('\u{000C}'),
            'r' => decoded.push('\r'),
            '"' => decoded.push('"'),
            '\'' => decoded.push('\''),
            '\\' => decoded.push('\\'),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' => {}
            'u' => {
                while chars.peek() == Some(&'u') {
                    chars.next();
                }
                let mut hex = String::new();
                for _ in 0..4 {
                    hex.push(chars.next().ok_or_else(|| {
                        GremlinError::Parse(format!("incomplete unicode escape in `{raw}`"))
                    })?);
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|err| {
                    GremlinError::Parse(format!("invalid unicode escape `\\u{hex}`: {err}"))
                })?;
                decoded.push(char::from_u32(code).ok_or_else(|| {
                    GremlinError::Parse(format!("invalid unicode scalar `\\u{hex}`"))
                })?);
            }
            '0'..='7' => {
                let mut octal = String::from(escaped);
                let max_extra = if escaped <= '3' { 2 } else { 1 };
                for _ in 0..max_extra {
                    if matches!(chars.peek(), Some('0'..='7')) {
                        octal.push(chars.next().expect("peeked octal digit"));
                    }
                }
                let value = u32::from_str_radix(&octal, 8).map_err(|err| {
                    GremlinError::Parse(format!("invalid octal escape `\\{octal}`: {err}"))
                })?;
                decoded.push(char::from_u32(value).ok_or_else(|| {
                    GremlinError::Parse(format!("invalid octal scalar `\\{octal}`"))
                })?);
            }
            other => {
                return Err(GremlinError::Parse(format!(
                    "unsupported escape `\\{other}` in `{raw}`"
                )));
            }
        }
    }
    Ok(decoded)
}

/// Extracts the first top-level (`true` / `false`) boolean argument from
/// a method-call text fragment, ignoring args inside nested parentheses.
/// Returns `None` if no top-level boolean is present.
pub(super) fn first_boolean_arg(raw: &str) -> Option<bool> {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
                i += 1;
            }
            _ if depth == 1 => {
                if bytes[i..].starts_with(b"true") {
                    let next = i + 4;
                    if next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ') {
                        return Some(true);
                    }
                }
                if bytes[i..].starts_with(b"false") {
                    let next = i + 5;
                    if next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ') {
                        return Some(false);
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

pub(super) fn value_map_token_selection(value: Option<&GValue>) -> (bool, bool) {
    let Some(GValue::String(value)) = value else {
        return (true, true);
    };
    match value
        .rsplit('.')
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase()
        .as_str()
    {
        "id" | "ids" => (true, false),
        "label" | "labels" => (false, true),
        "tokens" => (true, true),
        _ => (true, true),
    }
}

/// Returns true when a method-call text fragment references the
/// `Scope.local` token at the top-level argument position. Used by
/// aggregate dispatch to choose between global and per-list-traverser
/// evaluation without relying on grammar-specific accessor names.
pub(super) fn has_local_scope_arg(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    let needles: [&[u8]; 2] = [b"Scope.local", b"local"];
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
                i += 1;
            }
            _ => {
                if depth == 1 {
                    for needle in needles {
                        if bytes[i..].starts_with(needle) {
                            // Make sure we're at a token boundary on both sides.
                            let prev_ok = i == 0 || matches!(bytes[i - 1], b'(' | b',' | b' ');
                            let next = i + needle.len();
                            let next_ok =
                                next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ');
                            if prev_ok && next_ok {
                                return true;
                            }
                        }
                    }
                }
                i += 1;
            }
        }
    }
    false
}

pub(super) fn contains_scope_local(raw: &str) -> bool {
    has_local_scope_arg(raw)
}

pub(super) fn is_scope_local_arg(raw: &str) -> bool {
    matches!(raw.trim(), "Scope.local" | "local")
}

pub(super) fn extract_top_level_args(raw: &str) -> Vec<&str> {
    let Some(open) = raw.find('(') else {
        return Vec::new();
    };
    let mut args = Vec::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut depth = 0i32;
    let mut start = open + 1;
    for (idx, ch) in raw[open + 1..].char_indices() {
        let idx = open + 1 + idx;
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                let arg = raw[start..idx].trim();
                if !arg.is_empty() {
                    args.push(arg);
                }
                return args;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                let arg = raw[start..idx].trim();
                if !arg.is_empty() {
                    args.push(arg);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    args
}

/// Extracts the first top-level (depth-1) quoted string argument from a
/// method-call text fragment such as `tree("a")` or `subgraph('sg')`.
/// Returns the decoded value (handles `\\`, `\'`, `\"` escapes); returns
/// `None` when no top-level string literal is present. Used by the parser
/// to recover label/name arguments without depending on grammar variant
/// naming.
pub(super) fn extract_first_string_arg(raw: &str) -> Option<String> {
    extract_top_level_string_args(raw).into_iter().next()
}

/// Extracts every top-level (depth-1, comma-separated) quoted string
/// argument from a method-call text fragment. Strings inside nested
/// expressions (e.g. `call("a", __.has("nested", "x"))`) are skipped so
/// only the literal args at the outermost call site are returned.
pub(super) fn extract_top_level_string_args(raw: &str) -> Vec<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return out;
                }
                i += 1;
            }
            b'\'' | b'"' => {
                let quote = c;
                let start = i;
                let mut j = i + 1;
                let mut closed = false;
                while j < bytes.len() {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == quote {
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                if !closed {
                    return out;
                }
                if depth == 1 {
                    if let Ok(decoded) = decode_string_literal(&raw[start..=j]) {
                        out.push(decoded);
                    }
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

pub(super) fn option_key_text(text: &str) -> Option<String> {
    let inner = text.strip_prefix("option(")?.strip_suffix(')')?;
    let mut depth = 0i32;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return Some(inner[..idx].to_string()),
            _ => {}
        }
    }
    None
}

pub(super) fn parse_pick_key(text: &str) -> Option<OptionKey> {
    match text.trim() {
        "any" | "Pick.any" => Some(OptionKey::PickAny),
        "none" | "Pick.none" => Some(OptionKey::PickNone),
        "unproductive" | "Pick.unproductive" => Some(OptionKey::PickUnproductive),
        _ => None,
    }
}

/// Map a `GType.X` identifier (or bare `X`) to a numeric cast refinement.
/// Returns `None` for non-numeric refinements; the caller falls back to
/// the un-refined `CastTarget::Number`.
pub(super) fn numeric_cast_from_token(
    text: &str,
) -> Option<crate::language::gremlin::ast::NumericCast> {
    use crate::language::gremlin::ast::NumericCast;
    let normalised = text
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase();
    Some(match normalised.as_str() {
        "byte" => NumericCast::Byte,
        "short" => NumericCast::Short,
        "int" | "integer" => NumericCast::Int,
        "long" => NumericCast::Long,
        "bigint" | "biginteger" => NumericCast::BigInt,
        "float" => NumericCast::Float,
        "double" => NumericCast::Double,
        "bigdecimal" | "decimal" => NumericCast::BigDecimal,
        _ => return None,
    })
}
