//! Ordinary scalar SQL implementations. Templates bind repeated arguments once.
use super::{bind, unary};

pub(super) fn mapping(name: &str, pg: bool) -> Option<String> {
    let unary = |body: &str| super::bind_unary(body, pg);
    let double = if pg { "DOUBLE PRECISION" } else { "DOUBLE" };
    let nan = format!("CAST('NaN' AS {double})");
    let inf = format!("CAST('Infinity' AS {double})");
    Some(match name {
        "random" => "random()".into(),
        "degrees" | "radians" => format!("{name}(__arg0)"),
        "power" if !pg => "pow(__arg0, __arg1)".into(),
        "power" => {
            // PostgreSQL pow raises on IEEE overflow/underflow and domain
            // cases. Exact common powers avoid logarithmic threshold guesses.
            let mut zero = 2_f64.powf(-537.5);
            while zero * zero != 0.0 { zero = zero.next_down(); }
            while zero.next_up() * zero.next_up() == 0.0 { zero = zero.next_up(); }
            let mut finite = f64::MAX.sqrt();
            while !(finite * finite).is_finite() { finite = finite.next_down(); }
            bind(2, &format!("CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL WHEN __local2=0 THEN 1::float8 WHEN __local2=1 THEN __local1 WHEN __local1={nan} THEN {nan} WHEN __local2=2 THEN CASE WHEN abs(__local1)>{finite:.17e} THEN {inf} WHEN abs(__local1)<={zero:.17e} THEN 0::float8 ELSE __local1*__local1 END WHEN abs(__local1)={inf} THEN {inf} WHEN __local1=0 THEN 0::float8 WHEN __local1<0 THEN {nan} ELSE power(__local1,0.5::float8) END"))
        },
        "digest" => {
            let hashes: &[&str] = if pg { &["sha224", "sha256", "sha384", "sha512"] } else { &["sha256"] };
            let mut cases = hashes.iter().map(|h| format!("WHEN '{h}' THEN {}", if pg { format!("{h}(__local1)") } else { format!("from_hex({h}(__local1))") })).collect::<Vec<_>>();
            cases.push(format!("WHEN 'md5' THEN {}", if pg { "decode(md5(__local1), 'hex')" } else { "from_hex(md5(__local1))" }));
            format!("(SELECT CASE __local2 {} END FROM (SELECT {} AS __local1, __arg1 AS __local2 OFFSET 0) AS __local3)", cases.join(" "), bytes(pg))
        }
        "gcd" | "lcm" => format!("{name}(__arg0, __arg1)"),
        "greatest" | "least" => format!("{name}(__args)"),
        "exp" if pg => unary(&format!("CASE WHEN __local1 = {nan} THEN __local1 WHEN __local1 > 709.782712893384 THEN {inf} WHEN __local1 < -745.1332191019411 THEN CAST(0 AS {double}) ELSE exp(__local1) END")),
        "exp" => "exp(__arg0)".into(),
        "cosh" | "sinh" if pg => unary(&format!("CASE WHEN __local1 = {nan} THEN __local1 WHEN abs(__local1) > 710.4758600739439 THEN {} ELSE {name}(__local1) END", if name == "sinh" { format!("sign(__local1) * {inf}") } else { inf.clone() })),
        "cosh" | "sinh" => format!("{name}(__arg0)"),
        "cot" => unary(&format!("CASE WHEN __local1 = 0 THEN CASE WHEN {} THEN -{inf} ELSE {inf} END WHEN __local1 IN ({inf}, -{inf}) THEN {nan} ELSE 1.0 / tan(__local1) END", if pg { "get_byte(float8send(__local1), 0) >= 128" } else { "signbit(__local1)" })),
        "factorial" => unary("CASE WHEN __local1 < 0 THEN 1 ELSE CAST(factorial(CAST(__local1 AS INTEGER)) AS BIGINT) END"),
        "left" | "right" | "repeat" => format!("{name}(__arg0, CAST(__arg1 AS INTEGER))"),
        "split_part" => "split_part(__arg0, __arg1, CAST(__arg2 AS INTEGER))".into(),
        "substr_index" => {
            let reverse = |x: &str| if pg { format!("reverse({x})") } else { format!("array_to_string(list_reverse(string_split({x}, '')), '')") };
            let input = format!("CASE WHEN __local3 < 0 THEN {} ELSE __local1 END", reverse("__local1"));
            let delimiter = format!("CASE WHEN __local3 < 0 THEN {} ELSE __local2 END", reverse("__local2"));
            let split = if pg { "string_to_array" } else { "string_split" };
            let result = format!("(SELECT coalesce(string_agg(CASE WHEN __local6 = 1 THEN __local5 ELSE {delimiter} || __local5 END, '' ORDER BY __local6), '') FROM unnest({split}({input}, {delimiter})) WITH ORDINALITY AS __local4(__local5, __local6) WHERE __local6 <= abs(CAST(__local3 AS DECIMAL(20, 0))))");
            bind(3, &format!("CASE WHEN __local1 IS NULL OR __local2 IS NULL OR __local3 IS NULL THEN NULL WHEN __local1 = '' OR __local2 = '' OR __local3 = 0 THEN '' ELSE (SELECT CASE WHEN __local3 < 0 THEN {} ELSE __local7 END FROM (SELECT {result} AS __local7 OFFSET 0) AS __local8) END", reverse("__local7")))
        }
        "find_in_set" => if pg {
            "(SELECT CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL WHEN strpos(__local1, ',') > 0 THEN 0 WHEN __local2 = '' THEN CASE WHEN __local1 = '' THEN 1 ELSE 0 END ELSE coalesce(array_position(string_to_array(__local2, ','), __local1), 0) END FROM (SELECT __arg0 AS __local1, __arg1 AS __local2 OFFSET 0) AS __local0)"
        } else {
            "(SELECT CASE WHEN __local1 IS NULL OR __local2 IS NULL THEN NULL WHEN strpos(__local1, ',') > 0 THEN 0 ELSE coalesce(list_position(string_split(__local2, ','), __local1), 0) END FROM (SELECT __arg0 AS __local1, __arg1 AS __local2 OFFSET 0) AS __local0)"
        }.into(),
        "to_hex" if pg => "to_hex(__arg0)".into(),
        "to_hex" => unary("CASE WHEN typeof(__local1) IN ('INTEGER', 'UINTEGER') THEN right(lower(hex(__local1)), 8) ELSE lower(hex(__local1)) END"),
        "nvl2" => "CASE WHEN __arg0 IS NOT NULL THEN __arg1 ELSE __arg2 END".into(),
        "sha224" | "sha256" | "sha384" | "sha512" if pg => format!("{name}({})", bytes(true)),
        "sha256" => format!("from_hex(sha256({}))", bytes(false)),
        "current_date" => "CURRENT_DATE".into(),
        "current_time" => "CAST(CURRENT_TIME AS TIME)".into(),
        "now" => "CURRENT_TIMESTAMP".into(),
        "make_date" => "make_date(CAST(__arg0 AS INTEGER), CAST(__arg1 AS INTEGER), CAST(__arg2 AS INTEGER))".into(),
        // Numeric inputs are guarded at the typed call boundary. Text parsing
        // uses a different grammar on each engine and must not silently drift.
        "to_timestamp" | "to_timestamp_seconds" | "from_unixtime" | "to_timestamp_millis" | "to_timestamp_micros" => {
            let units = match name { "to_timestamp_millis" => 1000_i64, "to_timestamp_micros" => 1000000, _ => 1 };
            if pg {
                unary(&format!("TIMESTAMP '1970-01-01' + (__local1 / {}) * INTERVAL '1 day' + ((__local1 % {}) / {units}) * INTERVAL '1 second' + (__local1 % {units}) * INTERVAL '{} microseconds'", 86400 * units, 86400 * units, 1000000 / units))
            } else { format!("make_timestamp(CAST(__arg0 AS BIGINT) * {})", 1000000 / units) }
        }
        "to_timestamp_nanos" if pg => unary("TIMESTAMP '1970-01-01' + (__local1 / 86400000000000) * INTERVAL '1 day' + ((__local1 % 86400000000000) / 1000) * INTERVAL '1 microsecond'"),
        "to_timestamp_nanos" if !pg => "make_timestamp_ns(CAST(__arg0 AS BIGINT))".into(),
        "to_unixtime" => "CAST(trunc(extract(epoch FROM __arg0)) AS BIGINT)".into(),
        "to_time" => "CAST(__arg0 AS TIME)".into(),
        "chr" if pg => unary("chr(CAST((__local1 % 4294967296 + 4294967296) % 4294967296 AS INTEGER))"),
        "chr" if !pg => unary("chr(CAST((__local1 % 4294967296 + 4294967296) % 4294967296 AS INTEGER))"),
        "to_date" => "CAST(__arg0 AS DATE)".into(),
        "to_local_time" => "__arg0".into(),
        "make_time" => {
            let fail = if pg { "CAST('invalid hour ' || CAST(__local1 AS TEXT) AS TIME)" } else { "error('invalid hour')" };
            bind(3, &format!("CASE WHEN __local1 IS NULL OR __local2 IS NULL OR __local3 IS NULL THEN NULL WHEN __local1 < 0 OR __local1 >= 24 THEN {fail} ELSE make_time(CAST(__local1 AS INTEGER), CAST(__local2 AS INTEGER), CAST(__local3 AS DOUBLE PRECISION)) END"))
        }

        _ => return None,
    })
}

pub(super) fn bytes(pg: bool) -> &'static str {
    if pg {
        "(SELECT CASE WHEN CAST(pg_typeof(__local1) AS TEXT) = 'bytea' THEN CAST(__local1 AS BYTEA) ELSE convert_to(CAST(__local1 AS TEXT), 'UTF8') END FROM (SELECT __arg0 AS __local1 OFFSET 0) AS __local0)"
    } else {
        "(SELECT CASE WHEN typeof(__local1) = 'BLOB' THEN CAST(__local1 AS BLOB) ELSE encode(CAST(__local1 AS VARCHAR)) END FROM (SELECT __arg0 AS __local1 OFFSET 0) AS __local0)"
    }
}

pub(super) fn overload(name: &str, pg: bool, n: usize) -> Option<String> {
    Some(match (name, n) {
        ("round" | "trunc", 1) => {
            if pg && name == "round" {
                unary(
                    "CASE WHEN abs(__local1) >= 4503599627370496 THEN __local1 WHEN __local1 >= 0 THEN floor(__local1) + CASE WHEN __local1 - floor(__local1) >= 0.5 THEN 1 ELSE 0 END ELSE ceil(__local1) - CASE WHEN ceil(__local1) - __local1 >= 0.5 THEN 1 ELSE 0 END END",
                )
            } else {
                format!("{name}(__arg0)")
            }
        }
        ("round" | "trunc", 2) if !pg => format!("{name}(__arg0, CAST(__arg1 AS INTEGER))"),
        ("substr", 2) => "substr(__arg0, CAST(greatest(__arg1, 1) AS INTEGER))".into(),
        ("substr", 3) => bind(
            3,
            "substr(__local1, CAST(greatest(__local2, 1) AS INTEGER), CAST(greatest(__local3 + least(__local2 - 1, 0), 0) AS INTEGER))",
        ),
        ("encode", 2) => {
            let hex = if pg {
                "encode(__local1, 'hex')"
            } else {
                "lower(hex(__local1))"
            };
            let base64 = if pg {
                "replace(encode(__local1, 'base64'), chr(10), '')"
            } else {
                "to_base64(__local1)"
            };
            format!(
                "(SELECT CASE __local2 WHEN 'hex' THEN {hex} WHEN 'base64' THEN rtrim({base64}, '=') WHEN 'base64pad' THEN {base64} END FROM (SELECT {} AS __local1, __arg1 AS __local2 OFFSET 0) AS __local3)",
                bytes(pg)
            )
        }
        ("decode", 2) => {
            let valid = if pg {
                "__local1 ~ '^[0-9A-Fa-f]*$'"
            } else {
                "regexp_full_match(__local1, '[0-9A-Fa-f]*')"
            };
            let decode = if pg {
                "decode(__local1, 'hex')"
            } else {
                "from_hex(__local1)"
            };
            let fail = if pg {
                "decode(__local1 || '!', 'hex')"
            } else {
                "error('invalid encoding')"
            };
            let alphabet = if pg {
                "__local1 ~ '^[A-Za-z0-9+/]*={0,2}$'"
            } else {
                "regexp_full_match(__local1, '[A-Za-z0-9+/]*={0,2}')"
            };
            let padded = "rtrim(__local1, '=') || repeat('=', CAST((4 - length(rtrim(__local1, '=')) % 4) % 4 AS INTEGER))";
            let base64 = if pg {
                format!("decode({padded}, 'base64')")
            } else {
                format!("from_base64({padded})")
            };
            let canonical = if pg {
                "rtrim(replace(encode(__local4, 'base64'), chr(10), ''), '=')"
            } else {
                "rtrim(to_base64(__local4), '=')"
            };
            bind(2, &format!("CASE WHEN __local1 IS NULL THEN NULL WHEN __local2 = 'hex' THEN CASE WHEN length(__local1) % 2 = 0 AND {valid} THEN {decode} ELSE {fail} END ELSE CASE WHEN {alphabet} AND length(rtrim(__local1, '=')) % 4 <> 1 THEN CASE WHEN {} = rtrim(__local1, '=') THEN {base64} ELSE {fail} END ELSE {fail} END END", canonical.replace("__local4", &base64)))
                .replace("__arg0", if pg { "convert_from(__arg0, 'UTF8')" } else { "decode(__arg0)" })
        }
        ("to_char", 2) => {
            if pg {
                "to_char(__arg0, CASE __arg1 WHEN '%Y-%m-%d' THEN 'YYYY-MM-DD' WHEN '%H:%M:%S' THEN 'HH24:MI:SS' WHEN '%Y-%m-%d %H:%M:%S' THEN 'YYYY-MM-DD HH24:MI:SS' END)".into()
            } else {
                "strftime(__arg0, __arg1)".into()
            }
        }
        ("date_trunc", 2) => "date_trunc(__arg0, __arg1)".into(),
        ("date_part", 2) => bind(
            2,
            &format!(
                "CASE WHEN __local1 = 'second' THEN floor(date_part(__local1, __local2)) WHEN __local1 = 'isodow' THEN date_part(__local1, __local2) - 1 ELSE date_part(__local1, __local2) END"
            ),
        ),
        ("date_bin", 3) if pg => "date_bin(__arg0, __arg1, __arg2)".into(),
        ("date_bin", 3) => "time_bucket(__arg0, __arg1, __arg2)".into(),
        _ => return None,
    })
}
