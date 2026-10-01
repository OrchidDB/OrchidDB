#!/usr/bin/env python3
"""Execute original TCK empty-graph read assertions through the shared SQL compiler.

This is a SQL compiler profile, separate from the managed GraphEngine report.
Unsupported compilation is reported, never re-run on a different engine.
The dynamic three-language and mixed-engine matrix lives in the Python binding's
``tests/test_postgres_federation.py`` and runs against both real databases.
"""
import argparse, json, os, re, sys
from pathlib import Path
from decimal import Decimal
from cypher import value, normalize, rows_equal
from orchiddb import Compiler, DuckDBEngine, PostgresEngine

ROOT=Path(__file__).resolve().parent

def cases():
    catalog=json.loads((ROOT/'catalog.json').read_text())
    for case in catalog['cases']:
        if case['suite']!='opencypher': continue
        steps=case['steps']
        queries=[s['doc'] for s in steps if s['text']=='executing query:']
        results=[s for s in steps if s['text'].startswith('the result should be') and 'table' in s]
        allowed=lambda s: s['text'] in ('an empty graph','executing query:','parameters are:','no side effects') or s['text'].startswith('the result should be')
        if len(queries)!=1 or len(results)!=1 or not all(map(allowed,steps)):continue
        if not queries[0].lstrip().upper().startswith(('RETURN','WITH','UNWIND')):continue
        if re.search(r'\b(CREATE|MERGE|DELETE|SET|REMOVE|CALL)\b',queries[0],re.I):continue
        yield case, queries[0], results[0]

def normalize_sql(v):
    if isinstance(v,Decimal):return int(v) if v==v.to_integral_value() else float(v)
    if isinstance(v,list):return [normalize_sql(x) for x in v]
    if isinstance(v,dict):return {k:normalize_sql(x) for k,x in v.items()}
    return normalize(v)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine',choices=['postgres','duckdb'],required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--case',action='append',default=[])
    parser.add_argument('--baseline',type=Path,help='Compare the same pinned cases against another SQL engine report')
    args=parser.parse_args()
    if args.engine=='postgres':
        import psycopg
        connection=psycopg.connect(os.environ['ORCHIDDB_TEST_PG_URL'],autocommit=True)
        engine=PostgresEngine(connection)
    else:
        import duckdb
        connection=duckdb.connect();engine=DuckDBEngine(connection)
    compiler=Compiler();results=[]
    try:
        for case,query,expected in cases():
            if args.case and case['id'] not in args.case:continue
            params=next(({r[0]:value(r[1]) for r in s['table']} for s in case['steps'] if s['text']=='parameters are:'),{})
            record={k:case[k] for k in ['id','name','source','source_sha256']}
            request=dict(version=1,dialect=args.engine,language='cypher',query=query,parameters=params,tables=[])
            try:
                plan=compiler.compile(request)
                record['sql']=plan.sql
                with engine.query_arrow(plan) as reader:
                    actual=[[normalize_sql(v) for v in row.values()] for row in reader.read_all().to_pylist()]
                table=expected['table'];want=[[value(v) for v in row] for row in table[1:]]
                ordered='in order' in expected['text'];unordered_lists='ignoring list order' in expected['text']
                passed=list(plan.fields)==table[0] and rows_equal(actual,want,ordered,unordered_lists)
                record.update(status='passed' if passed else 'failed',sql=plan.sql)
                if not passed:record.update(actual=actual,expected=want)
            except Exception as e:record.update(status='failed',error=str(e))
            results.append(record)
    finally:connection.close()
    report=dict(engine=args.engine,profile='upstream-empty-graph-sql-reads',core_revision=compiler.core_revision,
                total=len(results),passed=sum(r['status']=='passed' for r in results),results=results)
    regressions = None
    if args.baseline:
        baseline = json.loads(args.baseline.read_text())
        previous = {r['id']: r for r in baseline['results']}
        if set(previous) != {r['id'] for r in results} or any(previous[r['id']]['source_sha256'] != r['source_sha256'] for r in results):
            raise ValueError('Baseline must contain exactly the same pinned cases')
        regressions = [r['id'] for r in results if previous[r['id']]['status'] == 'passed' and r['status'] != 'passed']
        report['comparison'] = dict(baseline_engine=baseline['engine'], regressions=regressions,
            shared_failures=[r['id'] for r in results if previous[r['id']]['status'] != 'passed' and r['status'] != 'passed'])
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2,default=str)+'\n')
    print(f"{args.engine}: {report['passed']}/{report['total']} original TCK assertions passed")
    if regressions is not None:
        print(f"{len(regressions)} regressions against {baseline['engine']}; {len(report['comparison']['shared_failures'])} shared failures remain reported")
    return 0 if report['passed']==report['total'] else 1
if __name__=='__main__':sys.exit(main())
