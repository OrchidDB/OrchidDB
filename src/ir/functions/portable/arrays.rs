//! Flat-array implementations with explicit ordering and null semantics.
use super::bind;

pub(super) fn mapping(name: &str, pg: bool) -> Option<String> {
    let len = if pg {
        "cardinality(__local1)"
    } else {
        "len(__local1)"
    };
    let rows = "unnest(__local1) WITH ORDINALITY AS __local4(__local5, __local6)";
    let strict = |body: &str| {
        bind(
            1,
            &format!("CASE WHEN __local1 IS NULL THEN NULL ELSE {body} END"),
        )
    };
    Some(match name {
        "make_array" => "ARRAY[__args]".into(),
        "array_append" => "array_append(__arg0, __arg1)".into(),
        "array_prepend" => "array_prepend(__arg0, __arg1)".into(),
        "array_min" | "array_max" => bind(
            1,
            &format!(
                "(SELECT __local5 FROM {rows} WHERE __local5 IS NOT NULL ORDER BY __local5 {} LIMIT 1)",
                if name == "array_min" { "ASC" } else { "DESC" }
            ),
        ),
        "array_distance" => {
            let len2 = len.replace("__local1", "__local2");
            let fail = if pg {
                "CAST('unequal lengths ' || CAST(__local1 AS TEXT) AS DOUBLE PRECISION)"
            } else {
                "error('Both arrays must have the same length')"
            };
            bind(
                2,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL OR EXISTS (SELECT 1 FROM unnest(__local1) AS __local4(__local5) WHERE __local5 IS NULL) OR EXISTS (SELECT 1 FROM unnest(__local2) AS __local6(__local7) WHERE __local7 IS NULL) THEN NULL WHEN {len} <> {len2} THEN {fail} WHEN {len} = 0 THEN CAST('-0' AS DOUBLE PRECISION) ELSE (SELECT sqrt(coalesce(sum(pow(CAST(__local5 AS DOUBLE PRECISION) - CAST(__local7 AS DOUBLE PRECISION), 2) ORDER BY __local8), 0)) FROM unnest(__local1) WITH ORDINALITY AS __local4(__local5, __local8) JOIN unnest(__local2) WITH ORDINALITY AS __local6(__local7, __local9) ON __local8 = __local9) END"
                ),
            )
        }
        "array_any_value" => bind(
            1,
            &format!(
                "(SELECT __local5 FROM {rows} WHERE __local5 IS NOT NULL ORDER BY __local6 LIMIT 1)"
            ),
        ),
        "cardinality" => bind(1, len),
        "empty" => bind(1, &format!("{len} = 0")),
        "array_ndims" => strict("1"),
        "array_dims" => strict(&format!(
            "CASE WHEN {len} = 0 THEN NULL ELSE ARRAY[{len}] END"
        )),
        "flatten" => "__arg0".into(),
        "array_reverse" => strict(&format!(
            "ARRAY(SELECT __local5 FROM {rows} ORDER BY __local6 DESC)"
        )),
        "array_pop_front" => strict(&format!(
            "ARRAY(SELECT __local5 FROM {rows} WHERE __local6 > 1 ORDER BY __local6)"
        )),
        "array_pop_back" => strict(&format!(
            "ARRAY(SELECT __local5 FROM {rows} WHERE __local6 < {len} ORDER BY __local6)"
        )),
        "array_element" => bind(
            2,
            &format!(
                "(SELECT __local5 FROM {rows} WHERE __local6 = CASE WHEN __local2 < 0 THEN {len} + __local2 + 1 ELSE __local2 END)"
            ),
        ),
        "array_distinct" => strict(&format!(
            "ARRAY(SELECT __local5 FROM {rows} GROUP BY __local5 ORDER BY min(__local6))"
        )),
        "array_union" | "array_intersect" | "array_except" => {
            let (source, condition) = match name {
                "array_union" => ("__local1 || __local2".to_owned(), "TRUE".to_owned()),
                "array_except" => ("__local1".to_owned(), "NOT EXISTS (SELECT 1 FROM unnest(__local2) AS __local7(__local8) WHERE __local8 IS NOT DISTINCT FROM __local5)".to_owned()),
                _ => {
                    let len2 = len.replace("__local1", "__local2");
                    (format!("CASE WHEN {len} >= {len2} THEN __local1 ELSE __local2 END"), format!("EXISTS (SELECT 1 FROM unnest(CASE WHEN {len} >= {len2} THEN __local2 ELSE __local1 END) AS __local7(__local8) WHERE __local8 IS NOT DISTINCT FROM __local5)"))
                }
            };
            bind(
                2,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE ARRAY(SELECT __local5 FROM unnest({source}) WITH ORDINALITY AS __local4(__local5, __local6) WHERE {condition} GROUP BY __local5 ORDER BY min(__local6)) END"
                ),
            )
        }
        "array_has" => bind(
            2,
            &format!(
                "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE EXISTS (SELECT 1 FROM {rows} WHERE __local5 = __local2) END"
            ),
        ),
        "array_has_all" | "array_has_any" => {
            let exists = "EXISTS (SELECT 1 FROM unnest(__local1) AS __local4(__local5) WHERE __local5 IS NOT DISTINCT FROM __local8)";
            let body = if name == "array_has_all" {
                format!(
                    "NOT EXISTS (SELECT 1 FROM unnest(__local2) AS __local7(__local8) WHERE NOT {exists})"
                )
            } else {
                format!(
                    "EXISTS (SELECT 1 FROM unnest(__local2) AS __local7(__local8) WHERE {exists})"
                )
            };
            bind(
                2,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE {body} END"
                ),
            )
        }
        "array_positions" => bind(
            2,
            &format!(
                "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT __local6 FROM {rows} WHERE __local5 IS NOT DISTINCT FROM __local2 ORDER BY __local6) END"
            ),
        ),
        "array_remove" | "array_remove_all" | "array_remove_n" | "array_replace"
        | "array_replace_all" | "array_replace_n" => {
            let replace = name.starts_with("array_replace");
            let n = if name.ends_with("_n") {
                if replace { 4 } else { 3 }
            } else if replace {
                3
            } else {
                2
            };
            let limit: String = if name.ends_with("_all") {
                "TRUE".into()
            } else if name.ends_with("_n") {
                if replace {
                    "(__local4 <= 0 OR __local9 <= __local4)".into()
                } else {
                    "__local9 <= __local3".into()
                }
            } else {
                "__local9 <= 1".into()
            };
            // Partitioned row_number counts only matches, including null matches.
            let source = "(SELECT __local5, __local6, (SELECT count(*) FROM unnest(__local1) WITH ORDINALITY AS __local12(__local13, __local14) WHERE __local13 IS NOT DISTINCT FROM __local2 AND __local14 <= __local6) AS __local9 FROM unnest(__local1) WITH ORDINALITY AS __local10(__local5, __local6)) AS __local11";
            let matches = format!("__local5 IS NOT DISTINCT FROM __local2 AND {limit}");
            let body = if replace {
                format!(
                    "ARRAY(SELECT CASE WHEN {matches} THEN __local3 ELSE __local5 END FROM {source} ORDER BY __local6)"
                )
            } else {
                format!(
                    "ARRAY(SELECT __local5 FROM {source} WHERE NOT ({matches}) ORDER BY __local6)"
                )
            };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL {} THEN NULL ELSE {body} END",
                    if replace { "" } else { "OR __local2 IS NULL" }
                ),
            )
        }
        _ => return None,
    })
}

pub(super) fn overload(name: &str, pg: bool, n: usize) -> Option<String> {
    let len = if pg {
        "cardinality(__local1)"
    } else {
        "len(__local1)"
    };
    Some(match (name, n) {
        ("array_length", 1) => bind(1, len),
        ("array_length", 2) => bind(
            2,
            &format!("CASE WHEN __local2 = 1 THEN {len} ELSE NULL END"),
        ),
        ("array_position", 2) => bind(
            2,
            "(SELECT min(__local6) FROM unnest(__local1) WITH ORDINALITY AS __local4(__local5, __local6) WHERE __local5 IS NOT DISTINCT FROM __local2)",
        ),
        ("array_position", 3) => {
            let fail = if pg {
                "CAST('start_from out of bounds ' || CAST(__local3 AS TEXT) AS BIGINT)"
            } else {
                "error('start_from out of bounds')"
            };
            bind(
                3,
                &format!(
                    "CASE WHEN {len} < __local3 - 1 THEN {fail} ELSE (SELECT min(__local6) FROM unnest(__local1) WITH ORDINALITY AS __local4(__local5, __local6) WHERE __local5 IS NOT DISTINCT FROM __local2 AND __local6 >= __local3) END"
                ),
            )
        }

        ("array_resize", 2 | 3) => {
            let default = if n == 3 { "__local3" } else { "NULL" };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT CASE WHEN __local6 <= {len} THEN (SELECT __local8 FROM unnest(__local1) WITH ORDINALITY AS __local7(__local8, __local9) WHERE __local9 = __local6) ELSE {default} END FROM generate_series(1, __local2) AS __local5(__local6) ORDER BY __local6) END"
                ),
            )
        }
        ("array_slice", 3 | 4) => {
            let step = if n == 4 { "__local4" } else { "1" };
            let from = format!(
                "greatest(CASE WHEN __local2 < 0 THEN {len} + __local2 + 1 ELSE __local2 END, 1)"
            );
            let to = format!(
                "CASE WHEN __local3 < 0 THEN {len} + __local3 + 1 ELSE least(__local3, {len}) END"
            );
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL OR __local3 IS NULL THEN NULL ELSE ARRAY(SELECT __local6 FROM unnest(__local1) WITH ORDINALITY AS __local5(__local6, __local7) WHERE __local7 >= {from} AND __local7 <= {to} AND (__local7 - {from}) % {step} = 0 ORDER BY __local7) END"
                ),
            )
        }
        ("array_sort", 2 | 3) => {
            let first = if n == 3 {
                "__local3 = 'NULLS FIRST'"
            } else {
                "TRUE"
            };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT __local5 FROM unnest(__local1) AS __local4(__local5) ORDER BY CASE WHEN {first} THEN __local5 IS NOT NULL ELSE __local5 IS NULL END, CASE WHEN __local2 = 'ASC' THEN __local5 END ASC, CASE WHEN __local2 = 'DESC' THEN __local5 END DESC) END"
                ),
            )
        }
        ("string_to_array", 2 | 3) => {
            let split = if pg {
                "string_to_array(__local1, __local2)"
            } else {
                "string_split(__local1, coalesce(__local2, ''))"
            };
            let empty = if pg {
                "ARRAY[]::TEXT[]"
            } else {
                "ARRAY[]::VARCHAR[]"
            };
            let chars = if pg {
                "string_to_array(__local1, NULL)"
            } else {
                "string_split(__local1, '')"
            };
            let split = format!(
                "CASE WHEN __local2 = '' THEN ARRAY[__local1] WHEN __local2 IS NULL THEN CASE WHEN __local1 = '' THEN {empty} ELSE {chars} END WHEN __local1 = '' THEN ARRAY[''] ELSE {split} END"
            );
            let value = if n == 3 {
                "CASE WHEN __local5 = __local3 THEN NULL ELSE __local5 END"
            } else {
                "__local5"
            };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT {value} FROM unnest({split}) WITH ORDINALITY AS __local4(__local5, __local6) ORDER BY __local6) END"
                ),
            )
        }
        ("range" | "generate_series", 1..=3) => {
            let (start, end, step) = match n {
                1 => ("0", "__local1", "1"),
                2 => ("__local1", "__local2", "1"),
                _ => ("__local1", "__local2", "__local3"),
            };
            let check = (1..=n)
                .map(|i| format!("__local{i} IS NULL"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let filter = if name == "range" {
                format!("WHERE __local5 <> {end}")
            } else {
                String::new()
            };
            bind(
                n,
                &format!(
                    "CASE WHEN {check} THEN NULL ELSE ARRAY(SELECT __local5 FROM generate_series({start}, {end}, {step}) WITH ORDINALITY AS __local4(__local5, __local6) {filter} ORDER BY __local6) END"
                ),
            )
        }
        ("array_sort", 1) => bind(
            1,
            "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT __local5 FROM unnest(__local1) AS __local4(__local5) ORDER BY __local5 ASC NULLS FIRST) END",
        ),
        ("array_concat", 2) => "array_cat(__arg0, __arg1)".into(),
        ("array_repeat", 2) => bind(
            2,
            "ARRAY(SELECT __local1 FROM generate_series(1, __local2) AS __local5)",
        ),
        ("array_to_string", 2 | 3) if !pg => {
            let input = if n == 3 {
                "list_transform(__local1, lambda __local4: coalesce(__local4, __local3))"
            } else {
                "__local1"
            };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE (SELECT CASE WHEN len(__local5) = 0 THEN '' ELSE list_reduce(__local5, lambda (__local6, __local7): __local6 || __local10 || __local7) END FROM (SELECT list_filter({input}, lambda __local8: __local8 IS NOT NULL) AS __local5, __local2 AS __local10 OFFSET 0) AS __local9) END"
                ),
            )
        }
        ("array_to_string", 2 | 3) => {
            let value = if n == 2 {
                "array_to_string(__local1, __local2)"
            } else if pg {
                "array_to_string(__local1, __local2, __local3)"
            } else {
                "array_to_string(list_transform(__local1, lambda __local4: coalesce(__local4, __local3)), __local2)"
            };
            bind(
                n,
                &format!(
                    "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE coalesce({value}, '') END"
                ),
            )
        }

        _ => return None,
    })
}
