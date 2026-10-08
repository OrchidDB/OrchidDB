pub(super) fn mapping(name: &str) -> Option<String> {
    Some(match name {
        "abs" | "ceil" | "floor" | "atan" | "md5" | "reverse" => format!("{name}(__arg0)"),
        "atan2" => "atan2(__arg0, __arg1)".into(),
        "pi" => "pi()".into(),
        "iszero" => "(__arg0 = 0)".into(),
        "contains" => "(instr(__arg0, __arg1) > 0)".into(),
        "starts_with" | "ends_with" => format!("{name}(__arg0, __arg1)"),
        "character_length" => "char_length(__arg0)".into(),
        "octet_length" => "length(__arg0)".into(),
        "bit_length" => "(length(__arg0) * 8)".into(),
        "strpos" => "instr(__arg0, __arg1)".into(),
        "replace" => "replace(__arg0, __arg1, __arg2)".into(),
        "coalesce" => "coalesce(__args)".into(),
        "nullif" => "nullif(__arg0, __arg1)".into(),
        "nvl" => "coalesce(__arg0, __arg1)".into(),
        "cardinality" => "array_length(__arg0)".into(),
        "array_reverse" => "array_reverse(__arg0)".into(),
        _ => return None,
    })
}
pub(super) fn overload(name: &str, arity: usize) -> Option<String> {
    Some(match (name, arity) {
        ("array_length", 1) => "array_length(__arg0)".into(),
        ("concat", 0) => "''".into(),
        ("concat", _) => format!(
            "concat({})",
            (0..arity)
                .map(|i| format!("coalesce(__arg{i}, '')"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ("btrim" | "ltrim" | "rtrim", 1) => {
            format!("{}(__arg0)", if name == "btrim" { "trim" } else { name })
        }
        _ => return None,
    })
}
