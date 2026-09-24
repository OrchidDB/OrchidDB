#!/usr/bin/env python3
"""Validate recorded evidence, adapter coverage and reference expectations."""
import hashlib,json
from pathlib import Path
from catalog import TESTS
ROOT=Path(__file__).resolve().parent
expected={t['id']:t for t in TESTS}
assert len(expected)==len(TESTS)
for product,languages in {'crabgraph':{'cypher','gremlin','sparql'},'ladybug':{'cypher'},'neo4j':{'cypher'},'puppygraph':{'cypher','gremlin'},'sqlg':{'gremlin'},'reference':{'gremlin'}}.items():
 d=json.loads((ROOT/f'results/{product}.json').read_text());rows=d['results']
 assert len({r['id'] for r in rows})==len(rows),(product,'duplicate results')
 assert {r['id'] for r in rows}=={t['id'] for t in TESTS if t['language'] in languages},(product,'missing results')
 for r in rows:
  assert r['status'] in {'pass','mismatch','query-error','timeout','harness-error'}
  assert r.get('probe_sha256')==hashlib.sha256(json.dumps(expected[r['id']],sort_keys=True).encode()).hexdigest(),(product,r['id'],'stale probe')
  assert r['status'] not in {'harness-error','timeout'},(product,r['id'],'infrastructure failure')
 if product=='reference':assert all(r['status']=='pass' for r in rows),'Fix Gremlin expectations before publishing'
print('Validated all six result files, probe hashes, adapter coverage and reference expectations')
