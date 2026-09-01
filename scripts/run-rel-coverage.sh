#!/usr/bin/env bash
set -euo pipefail

language="${1:-all}"
shards="${2:-4}"

if [[ ! "$shards" =~ ^[1-9][0-9]*$ ]]; then
  echo "shard count must be a positive integer" >&2
  exit 2
fi

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
report_root="$repo_dir/target/coverage-reports/${language}-duckdb-sharded"
previous_root="${report_root}-previous"

if [[ -d "$report_root" ]]; then
  if [[ -d "$previous_root" ]]; then
    rm -rf "$previous_root"
  fi
  mv "$report_root" "$previous_root"
fi
mkdir -p "$report_root"

cd "$repo_dir"
if [[ "${GRAPH_REL_SKIP_BUILD:-0}" != "1" ]]; then
  cargo test --release --test graph_rel_backend_cases --no-run
fi
test_binary="$({
  find "$repo_dir/target/release/deps" -type f -perm -111 \
    -name 'graph_rel_backend_cases-*' -print0
} | xargs -0 ls -1t | sed -n '1p')"
if [[ -z "$test_binary" ]]; then
  echo "compiled graph_rel_backend_cases binary was not found" >&2
  exit 1
fi

pids=()
for ((index = 0; index < shards; index++)); do
  shard_dir="$report_root/shard-$index"
  mkdir -p "$shard_dir"
  env \
    GRAPH_REL_LANG="$language" \
    GRAPH_REL_EXEC=duckdb \
    GRAPH_REL_SHARD_COUNT="$shards" \
    GRAPH_REL_SHARD_INDEX="$index" \
    GRAPH_REL_SHARD_CHUNK_SIZE="${GRAPH_REL_SHARD_CHUNK_SIZE:-8}" \
    GRAPH_REL_OUTPUT_DIR="$shard_dir" \
    GRAPH_REL_TIMEOUT_MS="${GRAPH_REL_TIMEOUT_MS:-1000}" \
    GRAPH_REL_SETUP_TIMEOUT_MS="${GRAPH_REL_SETUP_TIMEOUT_MS:-1000}" \
    GRAPH_REL_CASE_TIMEOUT_MS="${GRAPH_REL_CASE_TIMEOUT_MS:-60000}" \
    GRAPH_REL_SLOW_MS="${GRAPH_REL_SLOW_MS:-500}" \
    "$test_binary" --ignored --nocapture \
      >"$report_root/shard-$index.log" 2>&1 &
  pids+=("$!")
done

failed=0
for pid in "${pids[@]}"; do
  if ! wait "$pid"; then
    failed=1
  fi
done
if ((failed)); then
  echo "one or more coverage shards failed; see $report_root/shard-*.log" >&2
  exit 1
fi

awk -F '\t' '
  BEGIN { OFS="\t" }
  FNR == 1 { next }
  {
    language=$1
    for (column=2; column<=10; column++) totals[language, column]+=$column
    languages[language]=1
  }
  END {
    print "language", "total", "runnable", "matched", "parsed", "planned", "lowered", "executed", "mismatches", "skipped"
    for (language in languages) {
      printf "%s", language
      for (column=2; column<=10; column++) printf "\t%d", totals[language, column]
      printf "\n"
    }
  }
' "$report_root"/shard-*/metrics.tsv >"$report_root/metrics.tsv"

awk -F '\t' '
  BEGIN { OFS="\t" }
  FNR == 1 { next }
  { totals[$2]+=$1 }
  END {
    for (reason in totals) print totals[reason], reason
  }
' "$report_root"/shard-*/blockers.tsv \
  | sort -t $'\t' -k1,1nr >"$report_root/blockers.tsv"

{
  printf 'language\tpath\toutcome\treason\n'
  for cases_file in "$report_root"/shard-*/cases.tsv; do
    tail -n +2 "$cases_file"
  done | sort -t $'\t' -k1,1 -k2,2
} >"$report_root/cases.tsv"

if [[ -f "$previous_root/cases.tsv" ]]; then
  awk -F '\t' '
    BEGIN { OFS="\t" }
    NR == FNR {
      if (FNR > 1) previous[$1 SUBSEP $2]=$3
      next
    }
    FNR > 1 {
      key=$1 SUBSEP $2
      before=(key in previous ? previous[key] : "missing")
      transitions[before SUBSEP $3]++
      if (before == "matched" && $3 != "matched") {
        regressions[$1 SUBSEP $2]=before OFS $3 OFS $4
      }
    }
    END {
      print "before", "after", "count"
      for (key in transitions) {
        split(key, parts, SUBSEP)
        print parts[1], parts[2], transitions[key]
      }
    }
  ' "$previous_root/cases.tsv" "$report_root/cases.tsv" \
    | { IFS= read -r header; printf '%s\n' "$header"; sort -t $'\t' -k1,1 -k2,2; } \
    >"$report_root/transitions.tsv"

  awk -F '\t' '
    BEGIN { OFS="\t" }
    NR == FNR {
      if (FNR > 1) previous[$1 SUBSEP $2]=$3
      next
    }
    FNR > 1 {
      key=$1 SUBSEP $2
      if (previous[key] == "matched" && $3 != "matched") {
        print $1, $2, previous[key], $3, $4
      }
    }
  ' "$previous_root/cases.tsv" "$report_root/cases.tsv" \
    | { printf 'language\tpath\tbefore\tafter\treason\n'; sort -t $'\t' -k1,1 -k2,2; } \
    >"$report_root/regressions.tsv"
fi

awk -F '\t' '
  NR == 1 { next }
  {
    pct = $2 == 0 ? 0 : 100 * $4 / $2
    printf "%s: total=%d runnable=%d matched=%d (%.1f%%) parsed=%d planned=%d lowered=%d executed=%d mismatches=%d skipped=%d\n", $1, $2, $3, $4, pct, $5, $6, $7, $8, $9, $10
  }
' "$report_root/metrics.tsv"
echo "reports: $report_root"
