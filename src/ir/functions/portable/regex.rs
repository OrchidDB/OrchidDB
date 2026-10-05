//! The common, case-sensitive regex subset. Eligibility rejects syntax with
//! different Rust/RE2/POSIX meanings before any SQL is emitted.
use super::bind;
pub(super) fn overload(name: &str, pg: bool, n: usize) -> Option<String> {
    let result: String = match (name, n) {
        ("regexp_like", 2) => if pg { "(SELECT CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE regexp_match(__local1, __local2, 'p') IS NOT NULL END FROM (SELECT __arg0 AS __local1, __arg1 AS __local2 OFFSET 0) AS __local0)" } else { "regexp_matches(__arg0, __arg1)" }.into(),
        ("regexp_count", 2) => if pg {
            bind(2, "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN 0 ELSE (SELECT count(*) FROM regexp_matches(__local1, __local2, 'gp')) END")
        } else { "coalesce(len(regexp_extract_all(__arg0, __arg1)), 0)".into() },
        ("regexp_instr", 2) => if pg {
            bind(2, "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL ELSE coalesce(strpos(__local1, (regexp_match(__local1, __local2, 'p'))[1]), 0) END")
        } else {
            bind(2, "CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL WHEN regexp_matches(__local1, __local2) THEN strpos(__local1, regexp_extract(__local1, __local2)) ELSE 0 END")
        },
        ("regexp_match", 2) => if pg { "regexp_match(__arg0, __arg1, 'p')".into() } else {
            bind(2, "CASE WHEN regexp_matches(__local1, __local2) THEN ARRAY[regexp_extract(__local1, __local2)] ELSE NULL END")
        },
        ("regexp_replace", 3) => if pg { "regexp_replace(__arg0, __arg1, __arg2, 'p')" } else { "regexp_replace(__arg0, __arg1, __arg2)" }.into(),
        _ => return None,
    };
    Some(if pg {
        result.replace("__arg0", "(__arg0 COLLATE \"C\")")
    } else {
        result
    })
}
