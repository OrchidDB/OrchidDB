//! PostgreSQL mappings for the shared relational expression vocabulary.
use super::{SqlResult, functions::template};
use datafusion::sql::sqlparser::ast;

pub(super) fn adapt(expr: &mut ast::Expr, name: &str, args: &[ast::Expr]) -> SqlResult<()> {
    if super::postgres_lists::adapt(expr, name, args)? {
        return Ok(());
    }
    if name == "encode" && args.len() == 2 {
        // Preserve existing binary bytes; text uses UTF-8 rather than bytea's
        // backslash escape parser. Bind once for volatile input expressions.
        *expr = template(
            "(SELECT CASE WHEN CAST(pg_typeof(v) AS TEXT) = 'bytea' THEN encode(CAST(v AS BYTEA), e) ELSE encode(convert_to(CAST(v AS TEXT), 'UTF8'), e) END FROM (SELECT __arg0 AS v, __arg1 AS e) AS __local0)",
            args,
        )?;
        return Ok(());
    }
    if name == "__orchiddb_utf16_substring" && matches!(args.len(), 2 | 3) {
        let mut args = args.to_vec();
        if args.len() == 2 {
            args.push(ast::Expr::Value(
                ast::Value::Number("9223372036854775807".into(), false).into(),
            ));
        }
        *expr = template(
            "(SELECT CASE WHEN s IS NULL THEN NULL ELSE coalesce((SELECT string_agg(CASE WHEN p >= lo AND p + w <= hi THEN ch ELSE '�' END, '' ORDER BY p) FROM (SELECT ch, w, sum(w) OVER (ORDER BY ord ROWS UNBOUNDED PRECEDING) - w AS p FROM (SELECT ch, ord, CASE WHEN ascii(ch) > 65535 THEN 2 ELSE 1 END AS w FROM regexp_split_to_table(s, '') WITH ORDINALITY AS __local3(ch,ord)) AS __local2) AS __local1 WHERE hi > lo AND p < hi AND p + w > lo), '') END FROM (SELECT s, greatest(0,least(n,CASE WHEN a < 0 THEN n+a ELSE a END)) AS lo, greatest(0,least(n,CASE WHEN b < 0 THEN n+b ELSE b END)) AS hi FROM (SELECT s,a,b,coalesce((SELECT sum(CASE WHEN ascii(ch)>65535 THEN 2 ELSE 1 END) FROM regexp_split_to_table(s,'') AS __local2(ch)),0) AS n FROM (SELECT __arg0 AS s,coalesce(__arg1,0) AS a,coalesce(__arg2,9223372036854775807) AS b) AS __local1) AS __local2) AS __local0)",
            &args,
        )?;
        return Ok(());
    }
    if name == "strftime" && args.len() == 2 && args[1].to_string() == "'%Y-%m-%dT%H:%M:%S.%fZ'" {
        *expr = template("to_char(__arg0, 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')", args)?;
        return Ok(());
    }
    let rule = match (name, args.len()) {
        ("quantile_disc", 2) => "percentile_disc(__arg1) WITHIN GROUP (ORDER BY __arg0)",
        ("quantile_cont", 2) => "percentile_cont(__arg1) WITHIN GROUP (ORDER BY __arg0)",
        ("list_extract", 2) => "(__arg0)[__arg1]",
        ("regexp_full_match", 2) => "__arg0 ~ ('^(?:' || __arg1 || ')$')",
        ("regexp_full_match", 3) => "regexp_match(__arg0, '^(?:' || __arg1 || ')$', __arg2) IS NOT NULL",
        ("regexp_extract", 3) => "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL OR __arg2 IS NULL THEN NULL ELSE coalesce((regexp_match(__arg0, '(' || __arg1 || ')'))[__arg2 + 1], '') END",
        ("uuid", 0) => "CAST(gen_random_uuid() AS TEXT)",
        ("hex", 1) => "upper(encode(convert_to(__arg0, 'UTF8'), 'hex'))",
        ("epoch_us", 1) => "CAST(extract(epoch FROM __arg0) * 1000000 AS BIGINT)",
        ("make_timestamp", 1) => "TIMESTAMP '1970-01-01' + __arg0 * INTERVAL '1 microsecond'",

        ("array_has", 2) => {
            "(SELECT CASE WHEN a IS NULL OR v IS NULL THEN NULL ELSE array_position(a, v) IS NOT NULL END FROM (SELECT __arg0 AS a, __arg1 AS v OFFSET 0) AS __local0)"
        }
        ("array_length", 1) => "cardinality(__arg0)",
        ("array_element", 2) => "(__arg0)[__arg1]",
        ("array_slice", 3) => "(__arg0)[__arg1:__arg2]",
        ("array_empty", 1) => "cardinality(__arg0) = 0",
        ("array_concat", 2) => "array_cat(__arg0, __arg1)",
        ("array_min", 1) => "(SELECT min(v) FROM unnest(__arg0) AS __local0(v))",
        ("array_max", 1) => "(SELECT max(v) FROM unnest(__arg0) AS __local0(v))",
        ("array_sum", 1) => "(SELECT sum(v) FROM unnest(__arg0) AS __local0(v))",
        ("array_intersect", 2) => {
            "(SELECT CASE WHEN a IS NULL OR b IS NULL THEN NULL ELSE ARRAY(SELECT v FROM unnest(CASE WHEN cardinality(a) < cardinality(b) THEN b ELSE a END) WITH ORDINALITY AS __local1(v,n) WHERE array_position(CASE WHEN cardinality(a) < cardinality(b) THEN a ELSE b END,v) IS NOT NULL GROUP BY v ORDER BY min(n)) END FROM (SELECT __arg0 AS a, __arg1 AS b) AS __local0)"
        }
        ("array_replace_all", 3) => "array_replace(__arg0, __arg1, __arg2)",
        ("array_distinct", 1) => {
            "(SELECT CASE WHEN a IS NULL THEN NULL ELSE ARRAY(SELECT v FROM unnest(a) WITH ORDINALITY AS __local1(v, n) GROUP BY v ORDER BY min(n)) END FROM (SELECT __arg0 AS a) AS __local0)"
        }
        ("array_reverse", 1) => {
            "(SELECT CASE WHEN a IS NULL THEN NULL ELSE ARRAY(SELECT v FROM unnest(a) WITH ORDINALITY AS __local1(v, n) ORDER BY n DESC) END FROM (SELECT __arg0 AS a) AS __local0)"
        }
        ("array_to_string", 2) => "array_to_string(__arg0, __arg1)",
        ("contains", 2) => "strpos(__arg0, __arg1) > 0",
        ("starts_with", 2) => "starts_with(__arg0, __arg1)",
        ("ends_with", 2) => "right(__arg0, length(__arg1)) = __arg1",
        ("strpos", 2) => "strpos(__arg0, __arg1)",
        ("isnan", 1) => "__arg0 = CAST('NaN' AS DOUBLE PRECISION)",
        ("iszero", 1) => "__arg0 = 0",
        ("isfinite", 1) => {
            "__arg0 NOT IN (CAST('Infinity' AS DOUBLE PRECISION), CAST('-Infinity' AS DOUBLE PRECISION), CAST('NaN' AS DOUBLE PRECISION))"
        }
        ("signum", 1) => "sign(__arg0)",
        ("log", 2) => "ln(__arg1) / ln(__arg0)",
        ("log2", 1) => "ln(__arg0) / ln(2.0)",
        ("log10", 1) => "log(__arg0)",
        ("trunc", 2) => {
            "CAST(trunc(CAST(__arg0 AS NUMERIC), CAST(__arg1 AS INTEGER)) AS DOUBLE PRECISION)"
        }
        ("round", 2) => {
            "CAST(round(CAST(__arg0 AS NUMERIC), CAST(__arg1 AS INTEGER)) AS DOUBLE PRECISION)"
        }
        ("nanvl", 2) => {
            "(SELECT CASE WHEN a = CAST('NaN' AS DOUBLE PRECISION) THEN b ELSE a END FROM (SELECT __arg0 AS a, __arg1 AS b) AS __local0)"
        }
        ("regexp_like", 2) => "__arg0 ~ __arg1",
        ("regexp_like", 3) => {
            "(SELECT CASE WHEN s IS NULL OR r IS NULL OR f IS NULL THEN NULL ELSE regexp_match(s,r,f) IS NOT NULL END FROM (SELECT __arg0 AS s,__arg1 AS r,__arg2 AS f) AS __local0)"
        }
        ("__orchiddb_utf16_length", 1) => {
            "(SELECT CASE WHEN s IS NULL THEN NULL ELSE CAST(coalesce((SELECT sum(CASE WHEN ascii(c) > 65535 THEN 2 ELSE 1 END) FROM regexp_split_to_table(s, '') AS __local1(c)), 0) AS INTEGER) END FROM (SELECT __arg0 AS s) AS __local0)"
        }
        _ => return Ok(()),
    };
    *expr = template(rule, args)?;
    Ok(())
}

/// Emulate lenient casts without database functions or exception handlers.
/// JSONPath's silent conversion handles float overflow/underflow safely; a
/// decimal exponent computed from text supplies the IEEE overflow result.
pub(super) fn safe_cast(ty: &str) -> SqlResult<String> {
    let numeric = "v ~ '^[+-]?([0-9]+([.][0-9]*)?|[.][0-9]+)([eE][+-]?[0-9]+)?$'";
    let parts = "FROM (SELECT v, ltrim(digits,'0') AS digits, length(split_part(ltrim(m,'+-'),'.',1)) - length(digits) + length(ltrim(digits,'0')) - 1 + CASE WHEN length(ltrim(e,'+-0')) > 10 THEN CASE WHEN left(e,1)='-' THEN -4000000000 ELSE 4000000000 END WHEN ltrim(e,'+-0')='' THEN 0 ELSE CAST(e AS BIGINT) END AS power FROM (SELECT v, m, e, replace(ltrim(m,'+-'),'.','') AS digits FROM (SELECT v, split_part(lower(v),'e',1) AS m, split_part(lower(v),'e',2) AS e FROM (SELECT btrim(CAST(__arg0 AS TEXT)) AS v OFFSET 0) AS __local3) AS __local2) AS __local1) AS __local0";
    // Exponents are parsed only after syntax validation. The clamp exceeds
    // PostgreSQL's maximum text length, so even a long mantissa cannot cancel it.
    let parts = parts.replace(
        "ELSE CAST(e AS BIGINT)",
        "ELSE CAST(CASE WHEN e ~ '^[+-]?[0-9]+$' THEN e ELSE '0' END AS BIGINT)",
    );
    match ty {
        "DOUBLE PRECISION" | "DOUBLE" | "REAL" | "FLOAT" => {
            let double = format!("(SELECT CASE WHEN {numeric} THEN coalesce(CAST(jsonb_path_query_first(to_jsonb(v), '$.double()', '{{}}', true) #>> '{{}}' AS DOUBLE PRECISION), CASE WHEN digits='' OR power<0 THEN CASE WHEN left(v,1)='-' THEN CAST('-0' AS DOUBLE PRECISION) ELSE CAST('0' AS DOUBLE PRECISION) END ELSE CASE WHEN left(v,1)='-' THEN CAST('-Infinity' AS DOUBLE PRECISION) ELSE CAST('Infinity' AS DOUBLE PRECISION) END END) WHEN lower(v) IN ('nan','+nan','-nan') THEN CAST('NaN' AS DOUBLE PRECISION) WHEN lower(v) IN ('inf','+inf','infinity','+infinity') THEN CAST('Infinity' AS DOUBLE PRECISION) WHEN lower(v) IN ('-inf','-infinity') THEN CAST('-Infinity' AS DOUBLE PRECISION) ELSE NULL END {parts})");
            Ok(if ty == "REAL" {
                format!("(SELECT CASE WHEN f IN (CAST('NaN' AS DOUBLE PRECISION),CAST('Infinity' AS DOUBLE PRECISION),CAST('-Infinity' AS DOUBLE PRECISION)) THEN CAST(f AS REAL) WHEN abs(f)>=3.4028235677973366e38 THEN CASE WHEN f<0 THEN CAST('-Infinity' AS REAL) ELSE CAST('Infinity' AS REAL) END WHEN abs(f)<=7.006492321624085e-46 THEN CASE WHEN f<0 OR left(CAST(f AS TEXT),1)='-' THEN CAST('-0' AS REAL) ELSE CAST('0' AS REAL) END ELSE CAST(f AS REAL) END FROM (SELECT {double} AS f) AS __local4)")
            } else { double })
        }
        "BIGINT" | "INTEGER" | "SMALLINT" => {
            let (lo,hi) = match ty { "BIGINT" => ("-9223372036854775808","9223372036854775807"), "INTEGER" => ("-2147483648","2147483647"), _ => ("-32768","32767") };
            Ok(format!("(SELECT CASE WHEN v ~ '^[+-]?[0-9]+$' AND length(digits)<=19 THEN CASE WHEN CAST(coalesce(nullif(digits,''),'0') AS NUMERIC) * CASE WHEN left(v,1)='-' THEN -1 ELSE 1 END BETWEEN {lo} AND {hi} THEN CAST(CASE WHEN left(v,1)='-' THEN '-' ELSE '' END || coalesce(nullif(digits,''),'0') AS {ty}) ELSE NULL END ELSE NULL END FROM (SELECT v,ltrim(ltrim(v,'+-'),'0') AS digits FROM (SELECT btrim(CAST(__arg0 AS TEXT)) AS v OFFSET 0) AS __local1) AS __local0)"))
        }
        _ if ty.starts_with("DECIMAL(") || ty.starts_with("NUMERIC(") => {
            let parameters = ty.split_once('(').unwrap().1.trim_end_matches(')');
            let (p,s) = parameters.split_once(',').ok_or_else(|| super::SqlError::Unsupported(format!("PostgreSQL safe cast to {ty}")))?;
            let p: i32 = p.trim().parse().map_err(|_| super::SqlError::Unsupported(ty.into()))?;
            let s: i32 = s.trim().parse().map_err(|_| super::SqlError::Unsupported(ty.into()))?;
            // Bound both the coefficient and exponent before NUMERIC parsing.
            let value = format!("CASE WHEN {numeric} THEN CASE WHEN digits='' OR power<{} THEN 0 WHEN power>={} THEN NULL ELSE round(CAST((CASE WHEN left(v,1)='-' THEN '-' ELSE '' END) || left(digits,{}) || 'e' || CAST(power-least(length(digits),{})+1 AS TEXT) AS NUMERIC),{s}) END ELSE NULL END", -s-1,p-s,p+2,p+2);
            Ok(format!("(SELECT CASE WHEN abs(n)<power(CAST(10 AS NUMERIC),{}) THEN CAST(n AS {ty}) ELSE NULL END FROM (SELECT {value} AS n {parts}) AS __local4)",p-s))
        }
        "BOOLEAN" => Ok("(SELECT CASE WHEN lower(v) IN ('true','false') THEN CAST(v AS BOOLEAN) ELSE NULL END FROM (SELECT CAST(__arg0 AS TEXT) AS v OFFSET 0) AS __local0)".into()),
        _ if ty.starts_with("VARCHAR") || ty == "TEXT" => Ok(format!("CAST(__arg0 AS {ty})")),
        _ => Err(super::SqlError::Unsupported(format!("PostgreSQL safe cast to {ty}"))),
    }
}
