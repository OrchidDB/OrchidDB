#!/usr/bin/env python3
"""JSON transport for the original upstream assertions, using only host DuckDB.

No GraphEngine, source-language evaluator, or alternative query executor is used.
Fixture inserts and result decoding are transport; all queries use orchid_query.
"""
from session import execute as execute_query, describe as describe_query, prepare as prepare_query
import json
import hashlib
import math
from decimal import Decimal
import os
from pathlib import Path
import sys
import time

import duckdb

ROOT = Path(__file__).resolve().parents[2]
EXTENSION = Path(os.environ.get('ORCHID_EXTENSION', ROOT / 'extension/build/orchid.duckdb_extension')).resolve()


def ident(s):
    return '"' + s.replace('"', '""') + '"'


def literal(s):
    return "'" + s.replace("'", "''") + "'"


def sql_type(value, declared=None):
    types = {'Integer': 'INTEGER', 'Long': 'BIGINT', 'Byte': 'TINYINT', 'Short': 'SMALLINT',
             'Float': 'FLOAT', 'Double': 'DOUBLE', 'String': 'VARCHAR', 'Boolean': 'BOOLEAN'}
    if declared:
        if declared not in types:
            raise ValueError('Unsupported fixture type: ' + declared)
        return types[declared]
    if isinstance(value, bool): return 'BOOLEAN'
    if isinstance(value, int): return 'BIGINT'
    if isinstance(value, float): return 'DOUBLE'
    if isinstance(value, str): return 'VARCHAR'
    raise ValueError('Unsupported fixture value type: ' + type(value).__name__)


def typed(value, dtype):
    if value is None: return None
    names = {'TINYINT': 'Byte', 'SMALLINT': 'Short', 'INTEGER': 'Integer', 'BIGINT': 'Long',
             'FLOAT': 'Float', 'DOUBLE': 'Double'}
    if str(dtype) in names: return {'$type': names[str(dtype)], 'value': value}
    if isinstance(value, (bool, str)): return value
    if isinstance(value, list) and dtype.id == 'list':
        return [typed(v, dtype.children[0][1]) for v in value]
    raise ValueError('Unsupported typed Gremlin result: ' + str(dtype))


class Extension:
    def __init__(self):
        self.artifact_sha256 = hashlib.sha256(EXTENSION.read_bytes()).hexdigest()
        self.db = duckdb.connect(config={'allow_unsigned_extensions': 'true'})
        self.db.execute('LOAD ' + literal(str(EXTENSION)))
        self.db.execute('SET errors_as_json=true')
        self.fixtures = {}
        self.reset()

    def reset(self):
        self.db.execute('DROP SCHEMA IF EXISTS orchid_fixture CASCADE')
        self.db.execute('CREATE SCHEMA orchid_fixture')
        self.managed_table = 'orchid_fixture.graph'
        self.db.execute('CALL orchid_graph_create(?)', [self.managed_table])
        self.mapping = {'tables': [], 'nodes': [], 'edges': []}
        self.current_fixture = None
        self.procedures = {}

    def table(self, name, columns, rows):
        name = 'orchid_fixture.' + name
        self.db.execute('CREATE TABLE ' + name + '(' + ','.join(ident(k) + ' ' + t for k, t in columns.items()) + ')')
        if rows:
            self.db.executemany('INSERT INTO ' + name + ' VALUES (' + ','.join('?' for _ in columns) + ')', rows)
        self.mapping['tables'].append({'name': name})
        return name

    def fixture(self, req):
        # Fixture records are passed unchanged to the shared production importer.
        # Reset restores storage even when the same fixture follows mutations.
        self.reset()
        self.db.execute('CALL orchid_graph_import(?, ?)', [self.managed_table, json.dumps(req)])
        if req.get('fixture_key'):
            self.fixtures[req['fixture_key']] = req
        self.current_fixture = req
        return {'ok': True}

    def query(self, req):
        language = req.get('op', 'cypher')
        request = dict(version=1, dialect='duckdb', language=language, query=req['query'], procedures=self.procedures, managed_table=self.managed_table, **self.mapping)
        if language == 'cypher': request['parameters'] = req.get('params', {})
        if language == 'gremlin': request['bindings'] = req.get('bindings', {})
        encoded = json.dumps(request)
        if language in ('cypher', 'gremlin'):
            metadata = describe_query(self.db, encoded)
            if 'error' in metadata: return metadata
            if not metadata['fields']:
                execute_query(self.db, 'SELECT * FROM orchid_query(?)', encoded).fetchall()
                return {'columns': [], 'native_columns': [], 'native_rows': [], 'rows': [], 'backend': 'duckdb-extension'}
            if metadata['fields']:
                packed = ','.join(ident(name) + ':=__orchid_row.' + ident(name) for name in metadata['fields'])
                rows = execute_query(self.db, 'SELECT __orchiddb_value_json(struct_pack(' + packed + ')) FROM orchid_query(?) AS __orchid_row', encoded).fetchall()
                decoded = [{key['value']: value for key, value in json.loads(row[0])['value']} for row in rows]
                names = metadata['fields']
                return {'columns': names, 'native_columns': names, 'native_rows': [[row[name] for name in names] for row in decoded],
                        'rows': [], 'backend': 'duckdb-extension'}
        cursor = execute_query(self.db, 'SELECT * FROM orchid_query(?)', encoded)
        columns = [d[0] for d in cursor.description]
        types = [d[1] for d in cursor.description]
        rows = cursor.fetchall()
        def transport(value):
            if isinstance(value, Decimal):
                # Cypher's public numeric types are integer and double. DuckDB
                # may use DECIMAL for exact arithmetic on compiled literals.
                return int(value) if value == value.to_integral_value() else float(value)
            if isinstance(value, float) and not math.isfinite(value):
                return {'$float': 'NaN' if math.isnan(value) else 'Infinity' if value > 0 else '-Infinity'}
            if isinstance(value, (list, tuple)): return [transport(v) for v in value]
            if isinstance(value, dict): return {k: transport(v) for k, v in value.items()}
            return value
        result = {'columns': columns, 'rows': transport(rows), 'backend': 'duckdb-extension'}
        if language == 'gremlin':
            result['typed_rows'] = [[typed(v, t) for v, t in zip(row, types)] for row in rows]
        return result

    def rdf(self, req):
        self.reset()
        columns = ['g'] + [p + suffix for p in ('s', 'p', 'o') for suffix in ('', '_kind', '_dt', '_lang')]
        table = self.table('terms', dict.fromkeys(columns, 'VARCHAR'), req.get('quads', []))
        graph_table = self.table('graph_names', {'iri': 'VARCHAR'}, [[g] for g in req.get('named_graphs', [])])
        source = dict(writable=bool(req.get('update')), table=table, subject_column='s', predicate_column='p', object_column='o', graph_column='g',
                      typed_terms=[dict(value=p, kind=p + '_kind', datatype=p + '_dt', language=p + '_lang') for p in ('s', 'p', 'o')])
        request = dict(version=1, dialect='duckdb', language='sparql', query=req['query'], tables=self.mapping['tables'],
                       rdf_sources=[source], rdf_graph_names=dict(table=graph_table, column='iri', writable=bool(req.get('update'))))
        if req.get('update'):
            function, values = prepare_query(self.db, request, update=True)
            self.db.execute('CALL ' + function[:-1] + ', base := ?)', values + [req.get('base')]).fetchall()
            request['query'] = 'SELECT ?g ?s ?p ?o WHERE { { ?s ?p ?o } UNION { GRAPH ?g { ?s ?p ?o } } }'
            quads = self.rdf_query(request)['rows']
            request['query'] = 'SELECT ?g WHERE { GRAPH ?g {} }'
            names = [row[0]['value'] for row in self.rdf_query(request)['rows']]
            return {'quads': quads, 'named_graphs': names}
        return self.rdf_query(request)

    def rdf_query(self, request):
        encoded = json.dumps(request)
        metadata = describe_query(self.db, encoded)
        if 'error' in metadata: return metadata
        cursor = execute_query(self.db, 'SELECT * FROM orchid_query(?)', encoded)
        names = [d[0] for d in cursor.description]
        rows = cursor.fetchall()
        if metadata['result_form'] == 'Boolean': return {'boolean': bool(rows and rows[0][0])}
        fields = metadata['fields']
        def term(row, field):
            val = row[names.index(field)]
            kind, datatype, lang = [row[names.index('__rdf:term:' + part + ':' + field)] for part in ('kind', 'datatype', 'language')]
            if kind is None: return None
            if kind == 'IRI': return {'type': 'uri', 'value': val}
            if kind == 'BLANK': return {'type': 'bnode', 'value': val}
            if kind == 'LITERAL':
                return {'type': 'literal', 'value': val, 'datatype': datatype or 'http://www.w3.org/2001/XMLSchema#string', 'lang': lang}
            raise ValueError('Unknown RDF term kind: ' + kind)
        decoded = [[term(row, f) for f in fields] for row in rows]
        if metadata['result_form'] == 'RdfGraph': return {'graph': decoded}
        return {'variables': [f.removeprefix('?') for f in fields], 'rows': decoded}

    def send(self, req):
        op = req.get('op', 'cypher')
        if op == 'provider-info':
            return {'backend': 'duckdb-extension', 'duckdb_version': duckdb.__version__,
                    'extension_artifact_sha256': self.artifact_sha256,
                    'fixture_transport': 'original-upstream-property-records',
                    'query_transport': 'original-upstream-remote-bytecode'}
        if op == 'reset': self.reset(); return {'ok': True}
        if op == 'register-procedure':
            procedures = dict(self.procedures)
            procedures[req['name']] = {key:req[key] for key in ('inputs','outputs','rows')}
            request = dict(version=1,dialect='duckdb',language='cypher',query='RETURN 1',procedures=procedures,managed_table=self.managed_table,**self.mapping)
            metadata = describe_query(self.db, json.dumps(request))
            if 'error' in metadata: return metadata
            self.procedures = procedures
            return {'ok':True}
        if op == 'fixture': return self.fixture(req)
        if op == 'fixture-reset': return self.fixture(self.fixtures[req['fixture_key']])
        if op == 'cypher-snapshot':
            result = self.db.execute('SELECT * FROM orchid_graph_snapshot(?)', [self.managed_table]).fetchone()
            if result is None:
                raise ValueError('Managed graph snapshot returned no result')
            return {'native_snapshot': json.loads(result[0])}
        if op == 'sparql-syntax':
            self.db.execute('SELECT * FROM orchid_sparql_syntax(?, ?, ?)', [req['query'], req.get('base'), req.get('update', False)]).fetchall()
            return {'parsed': True}
        if op == 'rdf': return self.rdf(req)
        if op in ('cypher', 'gremlin'): return self.query(req)
        raise ValueError('Extension operation is not implemented: ' + op)


def main():
    adapter = Extension()
    try:
        for line in sys.stdin:
            start = time.monotonic()
            try: result = adapter.send(json.loads(line))
            except Exception as error:
                result = {'error': str(error)}
                try:
                    detail, _ = json.JSONDecoder().raw_decode(str(error)[str(error).index('{'):])
                    if 'orchid_classification' in detail:
                        result['classification'] = json.loads(detail['orchid_classification'])
                except (ValueError, TypeError): pass
            result.setdefault('backend', 'duckdb-extension')
            result['extension_artifact_sha256'] = adapter.artifact_sha256
            result['engine_instance'] = str(os.getpid())
            result['query_cost'] = {'metric_version': 1, 'coverage': 'elapsed_only', 'work_units': None,
                                    'request_elapsed_micros': int((time.monotonic() - start) * 1e6),
                                    'reason': 'duckdb_extension'}
            try: print(json.dumps(result, allow_nan=False), flush=True)
            except (ValueError, TypeError) as error:
                print(json.dumps({'error': 'Result transport: ' + str(error), 'engine_instance': str(os.getpid())}), flush=True)
    finally: adapter.db.close()


if __name__ == '__main__': main()
