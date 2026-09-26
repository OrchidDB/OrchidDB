//! TinkerPop 3.7 sampling algorithms and java.util.Random's 48-bit LCG.
//!
//! RNGs belong to logical steps (ExecutionContext), not calls, so resets of
//! correlated child traversals do not rewind their random sequence.

use num_traits::ToPrimitive;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::super::expr::eval;
use super::super::{RuntimeError, IrResult, Row};
use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::SampleKind;
use crate::ir::value::{Value, as_gremlin_set};

#[derive(Debug)]
pub(crate) struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5deece66d;
    const MASK: u64 = (1 << 48) - 1;

    pub(crate) fn new(seed: Option<i64>) -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let seed = seed.map(|s| s as u64).unwrap_or_else(|| {
            let time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            time ^ SEQUENCE.fetch_add(0x9e3779b97f4a7c15, Ordering::Relaxed)
        });
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next(&mut self, bits: u32) -> u32 {
        self.state = self.state.wrapping_mul(Self::MULTIPLIER).wrapping_add(11) & Self::MASK;
        (self.state >> (48 - bits)) as u32
    }

    fn next_double(&mut self) -> f64 {
        let high = self.next(26) as u64;
        let low = self.next(27) as u64;
        ((high << 27) + low) as f64 / (1u64 << 53) as f64
    }

    fn next_int(&mut self, bound: usize) -> usize {
        // Java collection sizes (and nextInt bounds) are signed int values.
        assert!(bound > 0 && bound <= i32::MAX as usize);
        if bound.is_power_of_two() {
            return ((bound as u64 * self.next(31) as u64) >> 31) as usize;
        }
        loop {
            let bits = self.next(31);
            let value = bits % bound as u32;
            if bits.wrapping_sub(value).wrapping_add(bound as u32 - 1) <= i32::MAX as u32 {
                return value as usize;
            }
        }
    }
}

pub(crate) fn sample_op(
    kind: SampleKind,
    weight: Option<&IrExpr>,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    random: &mut JavaRandom,
) -> IrResult<Vec<Row>> {
    match kind {
        SampleKind::Coin(probability) => Ok(rows
            .into_iter()
            .filter_map(|mut row| {
                row.bulk = if row.bulk < 100 {
                    (0..row.bulk)
                        .filter(|_| probability >= random.next_double())
                        .count() as u64
                } else {
                    // CoinStep deliberately approximates large bulks, using Math.round.
                    (probability * row.bulk as f64 + 0.5).floor().max(0.0) as u64
                };
                (row.bulk > 0).then_some(row)
            })
            .collect()),
        SampleKind::Local(amount) => rows
            .into_iter()
            .map(|mut row| {
                let sampled = sample_local(row.get("current"), amount, random)?;
                row.bindings.insert("current".into(), sampled);
                Ok(row)
            })
            .collect(),
        SampleKind::Global(amount) => {
            let mut weighted = Vec::with_capacity(rows.len());
            for row in rows {
                let weight = match weight {
                    Some(expr) => number(&eval(expr, &row, graph)?).ok_or_else(|| {
                        RuntimeError::Type("sample().by() must evaluate to a number".into())
                    })?,
                    None => 1.0,
                };
                weighted.push((row, weight));
            }
            sample_global(weighted, amount, random)
        }
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Byte(v) => Some(*v as f64),
        Value::UInt8(v) => Some(*v as f64),
        Value::Short(v) => Some(*v as f64),
        Value::UInt16(v) => Some(*v as f64),
        Value::Int(v) | Value::Long(v) => Some(*v as f64),
        Value::UInt32(v) => Some(*v as f64),
        Value::UInt64(v) => Some(*v as f64),
        Value::Float32(v) => Some(*v as f64),
        Value::Float(v) => Some(*v),
        Value::BigInt(v) | Value::UInt128(v) => v.to_f64(),
        Value::BigDecimal(v) => v.to_f64(),
        _ => None,
    }
}

fn sample_global(
    weighted: Vec<(Row, f64)>,
    amount: u64,
    random: &mut JavaRandom,
) -> IrResult<Vec<Row>> {
    let bulk = weighted
        .iter()
        .fold(0u64, |total, (row, _)| total.saturating_add(row.bulk));
    if bulk <= amount {
        return Ok(weighted.into_iter().map(|(row, _)| row).collect());
    }
    let total_weight: f64 = weighted.iter().map(|(row, w)| *w * row.bulk as f64).sum();
    if amount > 0
        && (!total_weight.is_finite()
            || total_weight <= 0.0
            || weighted.iter().any(|(_, w)| *w < 0.0))
    {
        return Err(RuntimeError::Runtime(
            "sample requires finite nonnegative weights with a positive total".into(),
        ));
    }
    let mut selected = vec![0u64; weighted.len()];
    let mut order = Vec::new();
    for _ in 0..amount {
        // Match SampleGlobalStep: each round starts with the original total,
        // excludes already sampled bulk, and retries if no candidate is chosen.
        'draw: loop {
            let mut running = total_weight;
            let mut positive_remaining = false;
            for (index, (row, weight)) in weighted.iter().enumerate() {
                positive_remaining |= *weight > 0.0 && selected[index] < row.bulk;
                for _ in selected[index]..row.bulk {
                    if random.next_double() <= *weight / running {
                        if selected[index] == 0 {
                            order.push(index);
                        }
                        selected[index] += 1;
                        break 'draw;
                    }
                    running -= *weight;
                }
            }
            if !positive_remaining {
                return Err(RuntimeError::Runtime(
                    "sample exhausted positive-weight traversers".into(),
                ));
            }
        }
    }
    Ok(order
        .into_iter()
        .map(|index| {
            let mut row = weighted[index].0.clone();
            row.bulk = selected[index];
            row
        })
        .collect())
}

fn choose_local<T>(mut original: Vec<T>, amount: u64, random: &mut JavaRandom) -> IrResult<Vec<T>> {
    if original.len() as u64 <= amount {
        return Ok(original);
    }
    if original.len() > i32::MAX as usize {
        return Err(RuntimeError::Runtime(
            "local sample collection exceeds Java's maximum size".into(),
        ));
    }
    let mut target = Vec::with_capacity(amount as usize);
    while target.len() < amount as usize {
        let index = random.next_int(original.len());
        target.push(original.remove(index));
    }
    Ok(target)
}

fn sample_local(value: Value, amount: u64, random: &mut JavaRandom) -> IrResult<Value> {
    if let Some(items) = as_gremlin_set(&value) {
        if items.len() as u64 <= amount {
            return Ok(value);
        }
        return Ok(Value::List(choose_local(items.to_vec(), amount, random)?));
    }
    Ok(match value {
        Value::List(items) | Value::BulkSet(items) => {
            Value::List(choose_local(items, amount, random)?)
        }
        Value::Map(items) if items.len() as u64 <= amount => Value::Map(items),
        Value::Map(items) => Value::TypedMap(choose_local(
            items
                .into_iter()
                .map(|(key, value)| (Value::String(key), value))
                .collect(),
            amount,
            random,
        )?),
        Value::TypedMap(items) => Value::TypedMap(choose_local(items, amount, random)?),
        other => other,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_random_matches_known_java_outputs() {
        let mut random = JavaRandom::new(Some(0));
        assert_eq!(random.next_double(), 0.730967787376657);
        assert_eq!(random.next_double(), 0.24053641567148587);
        let mut random = JavaRandom::new(Some(0));
        assert_eq!(
            (0..5).map(|_| random.next_int(100)).collect::<Vec<_>>(),
            vec![60, 48, 29, 47, 15]
        );
    }

    #[test]
    fn local_sample_preserves_scalars_and_consumes_rng_between_calls() {
        let mut random = JavaRandom::new(Some(999999));
        let values = vec![0.2, 0.4, 0.4, 0.5, 0.5, 1.0, 1.0, 1.0]
            .into_iter()
            .map(Value::Float)
            .collect();
        assert_eq!(
            sample_local(Value::List(values), 5, &mut random).unwrap(),
            Value::List(
                vec![0.5, 1.0, 0.4, 0.2, 1.0]
                    .into_iter()
                    .map(Value::Float)
                    .collect()
            )
        );
        assert_eq!(
            sample_local(Value::Int(42), 0, &mut random).unwrap(),
            Value::Int(42)
        );
        let first = choose_local((0..20).collect(), 5, &mut random).unwrap();
        let second = choose_local((0..20).collect(), 5, &mut random).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn coin_uses_java_bulk_threshold_and_does_not_draw_for_large_bulks() {
        let graph = PropertyGraph::new();
        let rows = vec![
            Row {
                bindings: Default::default(),
                bulk: 4,
            },
            Row {
                bindings: Default::default(),
                bulk: 100,
            },
        ];
        let mut random = JavaRandom::new(Some(0));
        let result = sample_op(SampleKind::Coin(0.5), None, rows, &graph, &mut random).unwrap();
        assert_eq!(
            result.iter().map(|row| row.bulk).collect::<Vec<_>>(),
            vec![1, 50]
        );
        let mut reference = JavaRandom::new(Some(0));
        for _ in 0..4 {
            reference.next_double();
        }
        assert_eq!(random.next_double(), reference.next_double());
    }

    #[test]
    fn global_sample_is_weighted_bulk_aware_and_seeded() {
        let rows = vec![
            (
                Row {
                    bindings: [("current".into(), Value::Int(1))].into(),
                    bulk: 5,
                },
                2.0,
            ),
            (Row::new().with("current", Value::Int(2)), 0.0),
            (
                Row {
                    bindings: [("current".into(), Value::Int(3))].into(),
                    bulk: 7,
                },
                3.0,
            ),
        ];
        let sample = sample_global(rows.clone(), 6, &mut JavaRandom::new(Some(42))).unwrap();
        assert_eq!(sample.iter().map(|r| r.bulk).sum::<u64>(), 6);
        assert!(sample.iter().all(|r| r.get("current") != Value::Int(2)));
        let again = sample_global(rows, 6, &mut JavaRandom::new(Some(42))).unwrap();
        assert_eq!(
            sample
                .iter()
                .map(|r| (&r.bindings, r.bulk))
                .collect::<Vec<_>>(),
            again
                .iter()
                .map(|r| (&r.bindings, r.bulk))
                .collect::<Vec<_>>()
        );
        let population = || {
            (0..30)
                .map(|i| (Row::new().with("current", Value::Int(i)), 1.0))
                .collect()
        };
        let a = sample_global(population(), 5, &mut JavaRandom::new(Some(1))).unwrap();
        let b = sample_global(population(), 5, &mut JavaRandom::new(Some(2))).unwrap();
        assert_ne!(
            a.iter().map(|r| r.get("current")).collect::<Vec<_>>(),
            b.iter().map(|r| r.get("current")).collect::<Vec<_>>()
        );
    }
}
