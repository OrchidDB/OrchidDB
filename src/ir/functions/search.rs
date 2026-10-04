//! Native Arrow scoring kernels and portable SQL implementations.
use super::logical::{LogicalFunction, SqlFunctionMapping, SqlOrderingMapping};
use arrow::{
    array::{
        Array, ArrayRef, FixedSizeListArray, Float64Array, LargeListArray, LargeStringArray,
        ListArray, StringArray, StringViewArray,
    },
    datatypes::DataType,
};
use datafusion::{
    common::{DataFusionError, Result, ScalarValue},
    logical_expr::{
        ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
    },
};
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, LazyLock},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Score {
    Cosine,
    Dot,
    L2,
    MaxSim,
    Bm25,
}
#[derive(Debug, PartialEq, Eq, Hash)]
struct Kernel {
    score: Score,
    signature: Signature,
}
fn invalid(message: &str) -> DataFusionError {
    DataFusionError::Execution(message.into())
}
fn child(array: &dyn Array, row: usize) -> Result<ArrayRef> {
    if let Some(a) = array.as_any().downcast_ref::<ListArray>() {
        return Ok(a.value(row));
    }
    if let Some(a) = array.as_any().downcast_ref::<LargeListArray>() {
        return Ok(a.value(row));
    }
    if let Some(a) = array.as_any().downcast_ref::<FixedSizeListArray>() {
        return Ok(a.value(row));
    }
    Err(invalid("vector input must be a list or fixed-size list"))
}
fn vector(array: &dyn Array) -> Result<Vec<f64>> {
    if array.is_empty() {
        return Err(invalid("vectors must be nonempty"));
    }
    (0..array.len())
        .map(|i| {
            let v = match ScalarValue::try_from_array(array, i)? {
                ScalarValue::Float32(Some(v)) => v as f64,
                ScalarValue::Float64(Some(v)) => v,
                _ => {
                    return Err(invalid(
                        "vector components must be non-null float32 or float64",
                    ));
                }
            };
            if !v.is_finite() {
                return Err(invalid("vector components must be finite"));
            }
            Ok(v)
        })
        .collect()
}
fn dot(a: &[f64], b: &[f64]) -> Result<f64> {
    if a.len() != b.len() {
        return Err(invalid("vector dimensions differ"));
    }
    Ok(a.iter().zip(b).map(|(a, b)| a * b).sum())
}
/// ColBERT-style sum of maximum inner products. Inputs are caller-encoded token
/// matrices; normalization is the caller's model contract. No model is loaded.
pub fn maxsim(query: &[Vec<f64>], document: &[Vec<f64>]) -> Result<f64> {
    if query.is_empty() || document.is_empty() {
        return Err(invalid("token matrices must be nonempty"));
    }
    let dimension = query[0].len();
    if dimension == 0
        || query
            .iter()
            .chain(document)
            .any(|v| v.len() != dimension || v.iter().any(|x| !x.is_finite()))
    {
        return Err(invalid(
            "token matrices require equal, nonzero dimensions and finite components",
        ));
    }
    let score = query
        .iter()
        .map(|q| {
            document
                .iter()
                .map(|d| dot(q, d).unwrap())
                .fold(f64::NEG_INFINITY, f64::max)
        })
        .sum::<f64>();
    if !score.is_finite() {
        return Err(invalid("MaxSim score overflow"));
    }
    Ok(score)
}
fn text(array: &dyn Array, row: usize) -> Result<Option<String>> {
    if array.is_null(row) {
        return Ok(None);
    }
    if let Some(a) = array.as_any().downcast_ref::<StringArray>() {
        return Ok(Some(a.value(row).to_owned()));
    }
    if let Some(a) = array.as_any().downcast_ref::<LargeStringArray>() {
        return Ok(Some(a.value(row).to_owned()));
    }
    if let Some(a) = array.as_any().downcast_ref::<StringViewArray>() {
        return Ok(Some(a.value(row).to_owned()));
    }
    Err(invalid("BM25 inputs must be strings"))
}
fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}
/// Exact BM25, k1=1.2, b=0.75, ASCII alphanumeric terms, case insensitive.
/// Empty/null documents remain in corpus statistics. Query terms are deduplicated.
pub fn bm25(query: &str, document: &str, corpus: &[Option<String>]) -> f64 {
    let q = tokens(query).into_iter().collect::<BTreeSet<_>>();
    let d = tokens(document);
    let docs = corpus
        .iter()
        .map(|s| tokens(s.as_deref().unwrap_or("")))
        .collect::<Vec<_>>();
    let n = docs.len() as f64;
    let length = docs.iter().map(Vec::len).sum::<usize>() as f64;
    if n == 0.0 || length == 0.0 {
        return 0.0;
    }
    q.iter()
        .map(|term| {
            let tf = d.iter().filter(|t| *t == term).count() as f64;
            let df = docs.iter().filter(|d| d.contains(term)).count() as f64;
            (1.0 + (n - df + 0.5) / (df + 0.5)).ln() * tf * 2.2
                / (tf + 1.2 * (0.25 + 0.75 * d.len() as f64 / (length / n)))
        })
        .sum()
}
impl ScalarUDFImpl for Kernel {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        "native_search_score"
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, args: &[DataType]) -> Result<DataType> {
        fn item(t: &DataType) -> Option<&DataType> {
            match t {
                DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                    Some(f.data_type())
                }
                _ => None,
            }
        }
        let floats = |t: &DataType| matches!(t, DataType::Float32 | DataType::Float64);
        let string = |t: &DataType| {
            matches!(
                t,
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
            )
        };
        let valid = match (self.score, args) {
            (Score::Bm25, [query, doc, corpus]) => {
                string(query)
                    && string(doc)
                    && (*corpus == DataType::Null || item(corpus).is_some_and(string))
            }
            (Score::MaxSim, [a, b]) => [a, b]
                .iter()
                .all(|t| **t == DataType::Null || item(t).and_then(item).is_some_and(floats)),
            (_, [a, b]) => [a, b]
                .iter()
                .all(|t| **t == DataType::Null || item(t).is_some_and(floats)),
            _ => false,
        };
        if !valid {
            return Err(DataFusionError::Plan("search function argument types do not match: vectors require float32/float64 lists, MaxSim requires token matrices, and BM25 requires text, text, list<text>".into()));
        }
        Ok(DataType::Float64)
    }
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let scalar = args
            .args
            .iter()
            .all(|a| matches!(a, ColumnarValue::Scalar(_)));
        let arrays = ColumnarValue::values_to_arrays(&args.args)?;
        let mut scores = Vec::with_capacity(arrays[0].len());
        for i in 0..arrays[0].len() {
            if arrays
                .iter()
                .any(|a| a.data_type() == &DataType::Null || a.is_null(i))
            {
                scores.push(None);
                continue;
            }
            if self.score == Score::Bm25 {
                let corpus = child(arrays[2].as_ref(), i)?;
                let corpus = (0..corpus.len())
                    .map(|j| text(corpus.as_ref(), j))
                    .collect::<Result<Vec<_>>>()?;
                scores.push(Some(bm25(
                    &text(arrays[0].as_ref(), i)?.unwrap(),
                    &text(arrays[1].as_ref(), i)?.unwrap(),
                    &corpus,
                )));
                continue;
            }
            let a = child(arrays[0].as_ref(), i)?;
            let b = child(arrays[1].as_ref(), i)?;
            let score = if self.score == Score::MaxSim {
                let matrix = |a: &dyn Array| {
                    (0..a.len())
                        .map(|j| {
                            if a.is_null(j) {
                                return Err(invalid("token vectors cannot be null"));
                            }
                            vector(child(a, j)?.as_ref())
                        })
                        .collect::<Result<Vec<_>>>()
                };
                maxsim(&matrix(a.as_ref())?, &matrix(b.as_ref())?)?
            } else {
                let a = vector(a.as_ref())?;
                let b = vector(b.as_ref())?;
                let product = dot(&a, &b)?;
                match self.score {
                    Score::Cosine => {
                        let norm = (dot(&a, &a)? * dot(&b, &b)?).sqrt();
                        if norm == 0.0 {
                            scores.push(None);
                            continue;
                        }
                        (product / norm).clamp(-1.0, 1.0)
                    }
                    Score::Dot => product,
                    Score::L2 => a
                        .iter()
                        .zip(&b)
                        .map(|(a, b)| (a - b).powi(2))
                        .sum::<f64>()
                        .sqrt(),
                    _ => unreachable!(),
                }
            };
            if !score.is_finite() {
                return Err(invalid("vector score overflow"));
            }
            scores.push(Some(score));
        }
        let array = Arc::new(Float64Array::from(scores));
        if scalar {
            Ok(ColumnarValue::Scalar(ScalarValue::try_from_array(
                array.as_ref(),
                0,
            )?))
        } else {
            Ok(ColumnarValue::Array(array))
        }
    }
}
fn mapping(value: &str, order: Option<(&str, bool)>) -> SqlFunctionMapping {
    SqlFunctionMapping {
        value: value.into(),
        ordering: order.map(|(e, r)| SqlOrderingMapping {
            expression: e.into(),
            reverse: r,
        }),
    }
}
pub fn functions() -> &'static BTreeMap<String, Arc<ScalarUDF>> {
    static FUNCTIONS: LazyLock<BTreeMap<String, Arc<ScalarUDF>>> = LazyLock::new(|| {
        let pg_a = "CAST(__arg0 AS vector)";
        let pg_b = "CAST(__arg1 AS vector)";
        [
            ("vector.cosine_similarity",Score::Cosine), ("vector.dot",Score::Dot),
            ("vector.l2_distance",Score::L2), ("vector.maxsim",Score::MaxSim),
            ("text.bm25",Score::Bm25),
        ].into_iter().map(|(name,score)| {
            let mut sql:BTreeMap<String,SqlFunctionMapping>=BTreeMap::new();
            match score {
                Score::Cosine=>{
                    let guard=format!("CASE WHEN ({pg_a} <#> {pg_a}) = 0 OR ({pg_b} <#> {pg_b}) = 0 THEN NULL ELSE 1.0 - ({pg_a} <=> {pg_b}) END");
                    let ordering=format!("CASE WHEN ({pg_a} <#> {pg_a}) = 0 OR ({pg_b} <#> {pg_b}) = 0 THEN NULL ELSE ({pg_a} <=> {pg_b}) END");
                    sql.insert("postgres".into(),mapping(&guard,Some((&ordering,true))));
                    sql.insert("duckdb".into(),mapping("CASE WHEN list_inner_product(__arg0, __arg0) = 0 OR list_inner_product(__arg1, __arg1) = 0 THEN NULL ELSE CAST(list_cosine_similarity(__arg0, __arg1) AS DOUBLE) END",None));
                },
                Score::Dot=>{
                    let key=format!("({pg_a} <#> {pg_b})");
                    sql.insert("postgres".into(),mapping(&format!("-({key})"),Some((&key,true))));
                    sql.insert("duckdb".into(),mapping("CAST(list_inner_product(__arg0, __arg1) AS DOUBLE)",None));
                },
                Score::L2=>{
                    sql.insert("postgres".into(),mapping(&format!("({pg_a} <-> {pg_b})"),None));
                    sql.insert("duckdb".into(),mapping("CAST(list_distance(__arg0, __arg1) AS DOUBLE)",None));
                },
                Score::MaxSim=>{
                    // PostgreSQL nested arrays are normalized to JSONB[] by the shared adapter.
                    sql.insert("postgres".into(),mapping("(SELECT SUM(__local1.best) FROM unnest(__arg0) AS __local0(q) CROSS JOIN LATERAL (SELECT MAX(-(CAST(__local0.q::text AS vector) <#> CAST(__local2.d::text AS vector))) AS best FROM unnest(__arg1) AS __local2(d)) AS __local1)",None));
                    sql.insert("duckdb".into(),mapping("(SELECT SUM(__local1.best) FROM unnest(CAST(__arg0 AS DOUBLE[][])) AS __local0(q) CROSS JOIN LATERAL (SELECT MAX(list_inner_product(__local0.q, __local2.d)) AS best FROM unnest(CAST(__arg1 AS DOUBLE[][])) AS __local2(d)) AS __local1)",None));
                },
                Score::Bm25=>{
                    for dialect in ["postgres", "duckdb"] { sql.insert(dialect.into(), mapping(&bm25_sql(dialect), None)); }
                }
            }
            if score!=Score::Bm25 {
                for (dialect, implementation) in &mut sql {
                    implementation.value=validated_vector_sql(score,dialect,&implementation.value);
                }
            }
            let native=Arc::new(ScalarUDF::new_from_impl(Kernel{score,signature:Signature::any(if score==Score::Bm25{3}else{2},Volatility::Immutable)}));
            use crate::ir::rel::search::SearchMetric;
            let metric=match score {Score::Cosine=>Some(SearchMetric::Cosine),Score::Dot=>Some(SearchMetric::Dot),Score::L2=>Some(SearchMetric::L2),Score::Bm25=>Some(SearchMetric::Bm25),Score::MaxSim=>None};
            let mut function=LogicalFunction::new(name,native,sql);
            function.search_metric=metric;
            (name.into(),function.into_udf())
        }).collect()
    });
    &FUNCTIONS
}
pub fn function(name: &str) -> Option<Arc<ScalarUDF>> {
    functions().get(&name.to_ascii_lowercase()).cloned()
}

fn bm25_sql(dialect: &str) -> String {
    if dialect == "duckdb" {
        return bm25_duckdb_sql();
    }
    let tokenize = |expr: &str| {
        format!(
            "regexp_split_to_array(translate(coalesce({expr}, ''), 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '[^a-z0-9]+')"
        )
    };
    let query = tokenize("__arg0");
    let document = tokenize("__arg1");
    let corpus = tokenize("__local6.document");
    let contains = if dialect == "postgres" {
        "array_position(__local0.terms, __local2.term) IS NOT NULL"
    } else {
        "list_contains(__local0.terms, __local2.term)"
    };
    format!(
        "(WITH __local0 AS (SELECT {corpus} AS terms, (SELECT COUNT(*) FROM unnest({corpus}) AS __local7(term) WHERE term <> '') AS dl FROM unnest(__arg2) AS __local6(document)), __local1 AS (SELECT CAST(COUNT(*) AS DOUBLE PRECISION) AS n, AVG(dl) AS avgdl FROM __local0), __local2 AS (SELECT DISTINCT term FROM unnest({query}) AS __local6(term) WHERE term <> ''), __local3 AS (SELECT term FROM unnest({document}) AS __local6(term) WHERE term <> '') SELECT CASE WHEN __arg0 IS NULL OR __arg1 IS NULL OR __arg2 IS NULL THEN NULL ELSE COALESCE(SUM(LN(1.0 + (__local1.n - __local4.df + 0.5) / (__local4.df + 0.5)) * __local5.tf * 2.2 / NULLIF(__local5.tf + 1.2 * (0.25 + 0.75 * (SELECT COUNT(*) FROM __local3) / NULLIF(__local1.avgdl, 0)), 0)), 0.0) END FROM __local2 CROSS JOIN __local1 CROSS JOIN LATERAL (SELECT COUNT(*) AS df FROM __local0 WHERE {contains}) AS __local4 CROSS JOIN LATERAL (SELECT COUNT(*) AS tf FROM __local3 WHERE __local3.term = __local2.term) AS __local5)"
    )
}

fn bm25_duckdb_sql() -> String {
    let tokenize = |expr: &str| {
        format!(
            "list_filter(regexp_split_to_array(translate(coalesce({expr}, ''), 'ABCDEFGHIJKLMNOPQRSTUVWXYZ', 'abcdefghijklmnopqrstuvwxyz'), '[^a-z0-9]+'), __local7 -> __local7 <> '')"
        )
    };
    let q = tokenize("__arg0");
    let d = tokenize("__arg1");
    let c = tokenize("__local0");
    let tf = "len(list_filter(__local1.doc, __local4 -> __local4 = __local3))";
    let df = "len(list_filter(__local1.corpus, __local5 -> list_contains(__local5, __local3)))";
    format!(
        "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL OR __arg2 IS NULL THEN NULL ELSE list_extract(list_transform([struct_pack(query := {q}, doc := {d}, corpus := list_transform(__arg2, __local0 -> {c}))], __local1 -> list_extract(list_transform([struct_pack(n := len(__local1.corpus), avgdl := list_avg(list_transform(__local1.corpus, __local6 -> len(__local6))))], __local2 -> coalesce(list_sum(list_transform(list_distinct(__local1.query), __local3 -> ln(1.0 + (__local2.n - {df} + 0.5) / ({df} + 0.5)) * {tf} * 2.2 / nullif({tf} + 1.2 * (0.25 + 0.75 * len(__local1.doc) / nullif(__local2.avgdl, 0)), 0))), 0.0)), 1)), 1) END"
    )
}

fn validated_vector_sql(score: Score, dialect: &str, value: &str) -> String {
    if dialect == "postgres" {
        if score != Score::MaxSim {
            return value.into();
        }
        return format!(
            "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL WHEN cardinality(__arg0) = 0 OR cardinality(__arg1) = 0 OR EXISTS (SELECT 1 FROM unnest(__arg0) AS __local3(v) WHERE v IS NULL) OR EXISTS (SELECT 1 FROM unnest(__arg1) AS __local4(v) WHERE v IS NULL) THEN CAST('invalid token matrix: ' || CAST(__arg0 AS TEXT) AS DOUBLE PRECISION) ELSE {value} END"
        );
    }
    let invalid = if score == Score::MaxSim {
        "__local3 IS NULL OR len(__local3) = 0 OR len(list_filter(__local3, __local4 -> __local4 IS NULL OR NOT isfinite(__local4))) > 0"
    } else {
        "__local3 IS NULL OR NOT isfinite(__local3)"
    };
    format!(
        "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL WHEN len(__arg0) = 0 OR len(__arg1) = 0 OR len(list_filter(__arg0, __local3 -> {invalid})) > 0 OR len(list_filter(__arg1, __local3 -> {invalid})) > 0 THEN error('search vectors must be nonempty and contain finite, non-null components') ELSE {value} END"
    )
}
