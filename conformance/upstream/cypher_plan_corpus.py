"""Deterministic, typed Cypher optimizer stress corpus; target queries never execute.

Compile with cypher_corpus_probe --conformance-sql --explain-sql. Fixture setup
runs so EXPLAIN sees real tables and statistics. Findings are review candidates,
not semantic conformance passes. Preserve full requests for reproduction.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
from collections import Counter

FIXTURES = {
    'empty': 'CREATE (:P {num: 1, name: "alpha", flag: true, xs: [1, 2]})',
    'nullable': 'CREATE (a:P {num: 1, name: "alpha", flag: true, xs: [1, 2]}), (b:P {num: 1, name: "beta", flag: false, xs: []}), (c:P), (a)-[:R {weight: 2}]->(b), (a)-[:R {weight: 2}]->(b), (b)-[:R]->(b), (b)-[:S]->(c)',
    'cycle': 'CREATE (a:P {num: 1, name: "alpha", flag: true, xs: [1, 2]}), (b:P {num: 2, name: "beta", flag: false, xs: [2, 2]}), (c:P {num: 3, name: "gamma", flag: true, xs: []}), (a)-[:R {weight: 1}]->(b), (b)-[:R {weight: 2}]->(c), (c)-[:R {weight: 3}]->(a), (a)-[:S]->(c)',
}
# Empty cases keep the declared schema through all scenario statements, while
# deleting rows in fixture setup. Never infer a property type from NULL alone.
def setup(name):
    return [FIXTURES[name]] + (['MATCH (n) DETACH DELETE n'] if name == 'empty' else [])


def templates():
    predicates = ['true', 'n.num IS NULL', 'n.num IS NOT NULL', 'n.num > 1',
                  'n.num = 1 OR n.num IS NULL', 'NOT n.name STARTS WITH "a"',
                  'n.flag', 'coalesce(n.num, 0) >= 0']
    expressions = ['n.num', 'n.name', 'n.flag', 'coalesce(n.num, 0)', 'n.num + 1',
                   'CASE WHEN n.num IS NULL THEN 0 ELSE n.num END']
    for match in ['MATCH (n:P)', 'OPTIONAL MATCH (n:P)', 'MATCH (n)', 'OPTIONAL MATCH (n:Missing)']:
        for pred in predicates:
            for expr in expressions:
                for depth in [0, 1, 3, 6]:
                    chain = f'{match} WHERE {pred}\nWITH {expr} AS x\n'
                    chain += 'WITH x AS x\n' * depth
                    for tail in ['RETURN x', 'RETURN DISTINCT x', 'RETURN x ORDER BY x LIMIT 3',
                                 'RETURN count(x) AS result', 'RETURN collect(DISTINCT x) AS result']:
                        yield 'projection', chain + tail
    for pattern in ['(n)-[r:R]->(m)', '(n)-[r:R]-(m)', '(n)-[r]->(m)',
                    '(n)-[:R]->()-[r:R]->(m)', '(n)-[:R]->()-[:R]->()-[r:R]->(m)',
                    '(n)-[r:R*0..2]->(m)', '(n)-[r:R*1..3]->(m)']:
        for optional in ['MATCH', 'OPTIONAL MATCH']:
            for pred in ['true', 'm.num IS NULL', 'm.num IS NOT NULL', 'm.num > n.num']:
                for ret in ['n.num, m.num', 'count(*) AS result', 'count(DISTINCT m) AS result',
                            'collect(m.num) AS result', 'n.num, sum(m.num) AS result']:
                    yield 'path', f'MATCH (n:P)\n{optional} {pattern}\nWHERE {pred}\nRETURN {ret}'
    for source in ['n.xs', '[n.num, 1, null]', '[]', '[1, 2, 2]', 'null']:
        for pred in ['x = 2', 'x IS NULL', 'x > n.num', 'x IS NOT NULL', 'x > 0 OR x IS NULL']:
            for quantifier in ['all', 'any', 'none', 'single']:
                expr = f'{quantifier}(x IN {source} WHERE {pred})'
                for prefix in ['MATCH (n:P)', 'OPTIONAL MATCH (n:P)']:
                    yield 'quantifier', f'{prefix} RETURN {expr} AS result'
            for expr in [f'size([x IN {source} WHERE {pred}])',
                         f'[x IN {source} WHERE {pred} | x + 1]']:
                yield 'collection', f'MATCH (n:P) RETURN {expr} AS result'
    for agg in ['count(*)', 'count(n.num)', 'count(DISTINCT n.num)', 'sum(n.num)',
                'avg(n.num)', 'min(n.num)', 'max(n.num)', 'collect(n.num)', 'collect(DISTINCT n.num)']:
        for group in ['', 'n.name AS grouping, ', 'n.flag AS grouping, ', 'n.num AS grouping, ']:
            for match in ['MATCH (n:P)', 'OPTIONAL MATCH (n:P)']:
                for tail in ['RETURN value', 'RETURN value ORDER BY value LIMIT 2',
                             'WITH value AS renamed RETURN renamed']:
                    yield 'aggregate', f'{match} WITH {group}{agg} AS value {tail}'
    for hops in range(1, 6):
        pattern = '(n)' + '-[:R]->()' * (hops - 1) + '-[:R]->(m)'
        for expr in [f'size([{pattern} | m.num])', f'size([{pattern} | 1]) > 0',
                     f'[{pattern} | m.num]', f'any(x IN [{pattern} | m.num > 1] WHERE x)']:
            for prefix in ['MATCH (n:P)', 'OPTIONAL MATCH (n:P)']:
                yield 'correlated', f'{prefix} RETURN n.num, {expr} AS result'
    for pred in predicates:
        for limit in [0, 1, 3]:
            for agg in ['count(*)', 'collect(n.num)', 'sum(n.num)']:
                yield 'limit_boundary', f'MATCH (n:P) WITH n ORDER BY n.num LIMIT {limit} WHERE {pred} RETURN {agg} AS result'
    for a in ['n.num', 'n.num + 1', 'coalesce(n.num, 0)']:
        for b in ['n.num', 'n.num - 1', 'coalesce(n.num, 1)']:
            for union in ['UNION', 'UNION ALL']:
                yield 'union', f'MATCH (n:P) RETURN {a} AS x {union} MATCH (n:P) RETURN {b} AS x'


def corpus():
    seen = set()
    for family, query in templates():
        if query in seen:
            continue
        seen.add(query)
        for fixture in FIXTURES:
            request = dict(query=query, setup=setup(fixture))
            request['scenario_queries'] = request['setup'] + [query]
            identity = hashlib.sha256(json.dumps(request, sort_keys=True).encode()).hexdigest()[:20]
            yield dict(id=identity, family=family, fixture=fixture, request=request)


def plan_metrics(plans):
    counts = Counter()
    estimates = []
    def visit(node):
        if isinstance(node, list):
            for item in node: visit(item)
        elif isinstance(node, dict):
            if 'name' in node: counts[node['name'].strip()] += 1
            extra = node.get('extra_info', {})
            value = extra.get('Estimated Cardinality')
            if value is not None:
                try: estimates.append(float(value))
                except (ValueError, TypeError): pass
            visit(node.get('children', []))
    for plan in plans:
        visit(plan.get('plan', []))
    return dict(operators=dict(counts), max_estimated_rows=max(estimates, default=None))


def inspect(case, binary, mode, timeout):
    started = time.monotonic()
    args = [str(binary), '--conformance-sql', '--explain-sql']
    if mode == 'nofold': args.append('--no-constant-folding')
    env = dict(os.environ, TOKIO_WORKER_THREADS='1')
    try:
        process = subprocess.run(args, input=json.dumps(case['request']) + '\n', text=True,
                                 capture_output=True, timeout=timeout, env=env)
        result = json.loads(process.stdout)
        if process.returncode: result['process_error'] = process.stderr[-4000:]
    except subprocess.TimeoutExpired:
        result = dict(error='Plan preparation timeout', timeout=True)
    except (ValueError, OSError) as error:
        result = dict(error=str(error))
    plans = result.get('sql_plans', [])
    metrics = plan_metrics(plans)
    sql = [result['sql']] if result.get('sql') else [r['sql'] for r in result.get('regions', [])]
    metrics.update(sql_chars=sum(map(len, sql)), residual_operators=result.get('residual_operators'))
    operators = [line.strip() for line in result.get('physical_plan', '').splitlines() if line.strip()]
    decode_only = bool(sql and operators) and all(op in {
        'Pruned(DecodeTraversers)Exec', 'CooperativeExec', 'duckdbExec'
    } for op in operators)
    metrics['output_decode_only'] = decode_only
    findings = []
    if result.get('error') or result.get('process_error'): findings.append('preparation_error')
    if any(p.get('error') for p in plans): findings.append('explain_error')
    if not sql: findings.append('no_sql')
    if sql and not plans: findings.append('missing_explain')
    if not result.get('fully_lowered') and not decode_only: findings.append('residual_execution')
    if metrics['operators'].get('CROSS_PRODUCT'): findings.append('cross_product_review')
    if (metrics['max_estimated_rows'] or 0) > 100000: findings.append('estimated_fanout_review')
    if metrics['sql_chars'] > 10000: findings.append('sql_size_review')
    return dict(**case, mode=mode, elapsed_seconds=time.monotonic()-started,
                findings=findings, metrics=metrics, result=result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--probe', type=Path)
    parser.add_argument('--jobs', type=int, default=4)
    parser.add_argument('--timeout', type=float, default=30)
    parser.add_argument('--limit', type=int)
    parser.add_argument('--generate-only', action='store_true')
    args = parser.parse_args()
    if args.jobs < 1 or args.timeout <= 0 or (args.limit is not None and args.limit < 1):
        parser.error('jobs, timeout and limit must be positive')
    if not args.generate_only and not args.probe: parser.error('--probe is required to inspect plans')
    args.output.mkdir(parents=True, exist_ok=True)
    cases = list(corpus())
    # Round-robin families so a smoke run covers all optimizer surfaces.
    groups = {}
    for c in cases: groups.setdefault(c['family'], []).append(c)
    cases = [group[i] for i in range(max(map(len, groups.values())))
             for group in groups.values() if i < len(group)]
    if args.limit: cases = cases[:args.limit]
    manifest = dict(cases=len(cases), families=dict(Counter(c['family'] for c in cases)),
                    query_execution=False, modes=['normal', 'nofold'], generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    (args.output/'corpus.jsonl').write_text(''.join(json.dumps(c)+'\n' for c in cases))
    if args.generate_only:
        (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
        print(json.dumps(manifest)); return
    binary = args.probe.resolve()
    manifest['probe_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    totals = Counter()
    with (args.output/'results.jsonl').open('w') as out, concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        pending = {}
        tasks = iter((case, mode) for case in cases for mode in ['normal', 'nofold'])
        def schedule():
            try: case, mode = next(tasks)
            except StopIteration: return False
            pending[pool.submit(inspect, case, binary, mode, args.timeout)] = (case['id'], mode)
            return True
        for _ in range(args.jobs): schedule()
        completed = 0
        while pending:
            done, _ = concurrent.futures.wait(pending, return_when=concurrent.futures.FIRST_COMPLETED)
            for future in done:
                pending.pop(future)
                row = future.result()
                out.write(json.dumps(row)+'\n'); out.flush()
                totals.update(row['findings']); completed += 1
                if completed % 100 == 0: print(f'{completed}/{len(cases)*2} plans: {dict(totals)}', flush=True)
                schedule()
    manifest.update(inspections=completed, findings=dict(totals))
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
    print(json.dumps(manifest), flush=True)
    if any(totals[k] for k in ['preparation_error', 'explain_error', 'missing_explain']):
        raise SystemExit(1)

if __name__ == '__main__': main()
