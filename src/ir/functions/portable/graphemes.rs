//! UAX #29 extended grapheme clusters, Unicode 17 (native segmentation version).
//! The generated ranges are from unicode-segmentation 1.13.2; see the adjacent
//! license and scripts/dev/generate_grapheme_sql.py. No database extension is needed.
use std::sync::LazyLock;
static RANGES: LazyLock<String> = LazyLock::new(|| {
    include_str!("grapheme_ranges.csv")
        .lines()
        .map(|s| format!("({s})"))
        .collect::<Vec<_>>()
        .join(",")
});
// Categories: Any=0, CR=1, Control=2, Extend=3, EP=4, InCB_Consonant=5,
// L=6, LF=7, LV=8, LVT=9, Prepend=10, RI=11, SpacingMark=12,T=13,V=14,ZWJ=15.
fn split(value: &str, pg: bool) -> String {
    let chars = if pg {
        format!("string_to_array({value}, NULL)")
    } else {
        format!("string_split({value}, '')")
    };
    let code = if pg { "ascii" } else { "unicode" };
    format!(
        "(WITH RECURSIVE __local300(__local301,__local302,__local303,__local304,__local305) AS (VALUES {}), __local306 AS (SELECT __local307,__local308,coalesce(__local303,0) AS __local309,coalesce(__local304,0) AS __local310,coalesce(__local305,0) AS __local311 FROM unnest({chars}) WITH ORDINALITY AS __local312(__local307,__local308) LEFT JOIN __local300 ON {code}(__local307) BETWEEN __local301 AND __local302 WHERE __local307 <> ''), __local313(__local314,__local315,__local316,__local317,__local318,__local319) AS (SELECT 0::bigint,0::bigint,0,0::bigint,0,0 UNION ALL SELECT __local308, __local315 + CASE WHEN __local314=0 THEN 1 WHEN __local316=1 AND __local309=7 THEN 0 WHEN __local316 IN (1,2,7) OR __local309 IN (1,2,7) THEN 1 WHEN __local316=6 AND __local309 IN (6,8,9,14) THEN 0 WHEN __local316 IN (8,14) AND __local309 IN (13,14) THEN 0 WHEN __local316 IN (9,13) AND __local309=13 THEN 0 WHEN __local309 IN (3,12,15) OR __local316=10 THEN 0 WHEN __local309=5 AND __local319=2 THEN 0 WHEN __local309=4 AND __local318=2 THEN 0 WHEN __local316=11 AND __local309=11 AND __local317%2=1 THEN 0 ELSE 1 END, __local309, CASE WHEN __local309=11 THEN __local317+1 ELSE 0 END, CASE WHEN __local309=4 THEN 1 WHEN __local309=3 AND __local318=1 THEN 1 WHEN __local309=15 AND __local318=1 THEN 2 ELSE 0 END, CASE WHEN __local309=5 THEN 1 WHEN __local311=1 AND __local319>0 THEN 2 WHEN __local310=1 THEN __local319 ELSE 0 END FROM __local313 JOIN __local306 ON __local308=__local314+1) SELECT ARRAY(SELECT string_agg(__local307,'' ORDER BY __local308) FROM __local313 JOIN __local306 ON __local308=__local314 GROUP BY __local315 ORDER BY __local315))",
        *RANGES
    )
}
pub(super) fn mapping(name: &str, pg: bool) -> Option<String> {
    if name != "translate" {
        return None;
    }
    let a = split("__local1", pg);
    let b = split("__local2", pg);
    let c = split("__local3", pg);
    let position = if pg {
        "array_position"
    } else {
        "list_position"
    };
    let ascii = |v: &str| {
        if pg {
            format!("octet_length({v})=length({v})")
        } else {
            format!("octet_length(encode({v}))=length({v})")
        }
    };
    let ascii_only = format!(
        "{} AND {} AND {}",
        ascii("__local1"),
        ascii("__local2"),
        ascii("__local3")
    );
    Some(super::bind(
        3,
        &format!(
            "CASE WHEN __local1 IS NULL OR __local2 IS NULL OR __local3 IS NULL THEN NULL WHEN {ascii_only} THEN translate(__local1,__local2,__local3) ELSE (SELECT coalesce(string_agg(CASE WHEN {position}(__local21,__local11) IS NULL THEN __local11 ELSE coalesce(__local22[{position}(__local21,__local11)],'') END,'' ORDER BY __local12),'') FROM (SELECT {a} AS __local20, {b} AS __local21, {c} AS __local22 OFFSET 0) AS __local23 CROSS JOIN unnest(__local20) WITH ORDINALITY AS __local10(__local11,__local12)) END"
        ),
    ))
}
pub(super) fn overload(name: &str, pg: bool, arity: usize) -> Option<String> {
    if !matches!(name, "lpad" | "rpad") || !matches!(arity, 2 | 3) {
        return None;
    }
    let a = split("__local1", pg);
    let fill = if arity == 3 { "__local3" } else { "' '" };
    let len = if pg { "cardinality" } else { "len" };
    let truncated = "(SELECT coalesce(string_agg(__local11,'' ORDER BY __local12),'') FROM unnest(__local20) WITH ORDINALITY AS __local10(__local11,__local12) WHERE __local12 <= __local2)";
    // Native truncates by grapheme clusters but pads by code points.
    let pad = format!(
        "left(repeat({fill}, CAST(ceil(CAST(greatest(__local2-{len}(__local20),0) AS DOUBLE PRECISION)/nullif(length({fill}),0)) AS INTEGER)), CAST(greatest(__local2-{len}(__local20),0) AS INTEGER))"
    );
    let padded = if name == "lpad" {
        format!("{pad} || __local1")
    } else {
        format!("__local1 || {pad}")
    };
    let ascii = |v: &str| {
        if pg {
            format!("octet_length({v})=length({v})")
        } else {
            format!("octet_length(encode({v}))=length({v})")
        }
    };
    let ascii_only = format!("{} AND {}", ascii("__local1"), ascii(fill));
    Some(super::bind(
        arity,
        &format!(
            "CASE WHEN __local1 IS NULL OR __local2 IS NULL OR {fill} IS NULL THEN NULL WHEN {ascii_only} THEN {name}(__local1, CAST(__local2 AS INTEGER), {fill}) ELSE (SELECT CASE WHEN __local2 < {len}(__local20) THEN {truncated} WHEN {fill}='' THEN __local1 ELSE {padded} END FROM (SELECT {a} AS __local20 OFFSET 0) AS __local21) END"
        ),
    ))
}
