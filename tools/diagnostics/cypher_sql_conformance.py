#!/usr/bin/env python3
"""Compile extracted TCK cases against real, typed DuckDB fixture tables.

Input is the internal extracted cases JSON (id/query/setup/parameters/procedures).
The original fixture cardinalities are preserved. Explicit per-scenario schema
annotations cover columns whose types cannot be inferred from an empty fixture.
Output is JSONL evidence, not a claim that SQL binding proves TCK conformance.
Requires the Python duckdb package and a built cypher_corpus_probe executable.
"""
import argparse
import collections
import json
import pathlib
import subprocess
import duckdb


def main():
    root = pathlib.Path(__file__).resolve().parents[2]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cases', required=True, type=pathlib.Path)
    parser.add_argument('--output', required=True, type=pathlib.Path)
    parser.add_argument('--probe', type=pathlib.Path, default=root / 'target/debug/examples/cypher_corpus_probe')
    parser.add_argument('--schemas', type=pathlib.Path, default=root / 'conformance/upstream/cypher_sql_schemas.json')
    parser.add_argument('--no-constant-folding', action='store_true',
                        help='Compile query expressions without constant folding for structural SQL review')
    args = parser.parse_args()
    cases = json.loads(args.cases.read_text())
    schemas = json.loads(args.schemas.read_text())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    counts = collections.Counter()
    with args.output.with_suffix('.stderr').open('w') as stderr, args.output.open('w') as output:
        command = [str(args.probe), '--mapped-sql']
        if args.no_constant_folding:
            command.append('--no-constant-folding')
        process = subprocess.Popen(command, stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, stderr=stderr, text=True, bufsize=1)
        try:
            for case in cases:
                if case.get('expected_error'):
                    continue
                request = {k: case[k] for k in ('query', 'setup', 'parameters', 'procedures') if k in case}
                if case['id'] in schemas:
                    request['schema'] = schemas[case['id']]
                process.stdin.write(json.dumps(request) + '\n')
                process.stdin.flush()
                line = process.stdout.readline()
                if not line:
                    raise RuntimeError(f"probe stopped at {case['id']} (exit={process.poll()})")
                result = json.loads(line)
                result['id'] = case['id']
                result['query'] = case['query']
                result['schema_source'] = 'explicit_case_declaration' if 'schema' in request else 'complete_fixture_types'
                validated_by_probe = 'validation' in result
                result.setdefault('validation', 'not_lowered')
                if 'fixture_sql' in result and not validated_by_probe:
                    connection = duckdb.connect()
                    try:
                        for statement in result['fixture_sql']:
                            connection.execute(statement)
                        result['fixture_loaded'] = True
                        if result.get('sql'):
                            # Bind against the real schema without running the benchmark.
                            connection.execute('EXPLAIN ' + result['sql']).fetchall()
                            result['validation'] = 'bound_to_real_tables'
                    except Exception as error:
                        result['validation'] = 'binding_error' if result.get('fixture_loaded') else 'fixture_error'
                        result['validation_error'] = str(error)
                    finally:
                        connection.close()
                counts[result['validation']] += 1
                output.write(json.dumps(result, ensure_ascii=False) + '\n')
                output.flush()
            process.stdin.close()
            if process.wait() != 0:
                raise RuntimeError(f'probe exited {process.returncode}')
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait()
    summary = dict(counts)
    args.output.with_suffix('.summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
