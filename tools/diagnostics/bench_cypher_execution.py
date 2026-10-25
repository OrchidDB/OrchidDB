#!/usr/bin/env python3
"""Paired warm-query benchmark of two local upstream runner binaries.

Run without concurrent builds/tests. Profiling is disabled. This measures the
whole server request, including compilation and execution, in development builds.
"""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time

QUERIES = {
    'scalar': 'RETURN 42 AS n',
    'aggregate': 'UNWIND range(1,1000) AS n RETURN sum(n) AS total',
    'quantifier': 'UNWIND range(1,500) AS k WITH range(1,k) AS xs RETURN sum(CASE WHEN all(x IN xs WHERE x > 0) THEN 1 ELSE 0 END) AS n',
    'graph_read': 'MATCH (n:Bench) WHERE n.v >= 50 RETURN sum(n.v) AS total',
}


def sample(binary, count, warmup):
    env = dict(os.environ)
    env.pop('ORCHIDDB_PROFILE_SQL', None)
    proc = subprocess.Popen([str(binary.resolve())], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, env=env)
    def request(payload):
        proc.stdin.write(json.dumps(payload) + '\n')
        proc.stdin.flush()
        line = proc.stdout.readline()
        if not line:
            raise RuntimeError(f'runner exited: {proc.poll()}')
        response = json.loads(line)
        if 'error' in response:
            raise RuntimeError(response['error'])
        return response
    try:
        request({'op': 'configure-schema', 'queries': [q for k,q in QUERIES.items() if k != 'graph_read']})
        results = {}
        for name, query in QUERIES.items():
            if name == 'graph_read':
                fixture = 'UNWIND range(1,100) AS v CREATE (:Bench {v:v})'
                request({'op': 'configure-schema', 'queries': [fixture, query]})
                request({'op': 'cypher', 'query': fixture})
            times, walls, rows = [], [], None
            for i in range(warmup + count):
                start = time.perf_counter_ns()
                result = request({'op': 'cypher', 'query': query})
                wall = (time.perf_counter_ns() - start) / 1000
                if rows is not None and rows != result['rows']:
                    raise AssertionError(f'unstable results: {name}')
                rows = result['rows']
                if i >= warmup:
                    times.append(result['query_cost']['request_elapsed_micros'])
                    walls.append(wall)
            results[name] = {'server_micros': times, 'roundtrip_micros': walls, 'rows': rows}
        return results
    finally:
        proc.stdin.close()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
        proc.stdout.close()
        proc.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--iterations', type=int, default=30)
    parser.add_argument('--warmup', type=int, default=10)
    args = parser.parse_args()
    rounds = []
    for i in range(args.rounds):
        run = {}
        for side in (('before', 'after') if i % 2 == 0 else ('after', 'before')):
            run[side] = sample(getattr(args, side), args.iterations, args.warmup)
        for name in QUERIES:
            assert run['before'][name]['rows'] == run['after'][name]['rows'], name
        rounds.append(run)
    summary = {}
    for name in QUERIES:
        summary[name] = {}
        for side in ('before', 'after'):
            values = [v for r in rounds for v in r[side][name]['server_micros']]
            summary[name][side] = {'median_micros': statistics.median(values),
                                  'p95_micros': sorted(values)[int((len(values)-1)*.95)]}
        summary[name]['median_speedup'] = summary[name]['before']['median_micros'] / summary[name]['after']['median_micros']
    report = {'scope': 'warm development builds; server compilation plus execution; profiling off; alternating order',
              'queries': QUERIES, 'warmup': args.warmup, 'iterations': args.iterations,
              'summary': summary, 'rounds': rounds}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
