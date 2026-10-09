"""Computed/physical identity regressions on real PostgreSQL and DuckDB.

Set ORCHIDDB_TEST_PG_URL to include PostgreSQL. No persistent tables are used.
"""
import json
import os
import unittest

from orchiddb import Connection, DuckDBEngine, PostgresEngine


class ComputedIdentityTests(unittest.TestCase):
    def check_traversals(self, dialect, composite):
        if dialect == 'postgres':
            import psycopg
            db = psycopg.connect(os.environ['ORCHIDDB_TEST_PG_URL'])
            engine = PostgresEngine(db)
        else:
            import duckdb
            db = duckdb.connect()
            engine = DuckDBEngine(db)
        try:
            citation = [{'source_id': 's', 'quote': 'Use Postgres.'}]
            nested_type = 'JSONB' if dialect == 'postgres' else 'STRUCT(source_id VARCHAR, quote VARCHAR)[]'
            db.execute(f'CREATE TEMP TABLE facts(project_id VARCHAR, record_id VARCHAR, text VARCHAR, supporting_evidence_ids VARCHAR[], evidence {nested_type})')
            db.execute('CREATE TEMP TABLE evidence(project_id VARCHAR, record_id VARCHAR, source_id VARCHAR, source_node_type VARCHAR)')
            db.execute('CREATE TEMP TABLE messages(project_id VARCHAR, record_id VARCHAR, text VARCHAR)')
            db.execute('CREATE TEMP TABLE links(project_id VARCHAR, record_id VARCHAR, src VARCHAR, dst VARCHAR)')
            payload = json.dumps(citation).replace("'", "''")
            db.execute(f"INSERT INTO facts VALUES ('1','f','Uses Postgres',ARRAY['e'],'{payload}'),('other','f','Private',ARRAY['e'],'[]')")
            db.execute("INSERT INTO evidence VALUES ('1','e','s','SlackMessage'),('other','e','s','SlackMessage')")
            db.execute("INSERT INTO messages VALUES ('1','s','Use Postgres.'),('other','s','Private source')")
            db.execute("INSERT INTO links VALUES ('1','l','f','e'),('other','l','f','e')")
            columns = {
                'facts': [('project_id','string'), ('record_id','string'), ('text','string'), ('supporting_evidence_ids','list:string'), ('evidence','list:struct:{"source_id":"string","quote":"string"}')],
                'evidence': [(n,'string') for n in ['project_id','record_id','source_id','source_node_type']],
                'messages': [(n,'string') for n in ['project_id','record_id','text']],
                'links': [(n,'string') for n in ['project_id','record_id','src','dst']],
            }
            key = ['project_id','record_id'] if composite else 'record_id'
            schema = {
                'tables': [dict(name=t, columns=[dict(name=n,data_type=k) for n,k in cs]) for t,cs in columns.items()],
                'nodes': [dict(label=label, table=t, id=key, source_query=f"SELECT * FROM {t} WHERE project_id = '1'", properties={n:n for n,_ in columns[t]}) for label,t in [('AgentFact','facts'),('AgentEvidence','evidence'),('SlackMessage','messages')]],
                'computed_relationships': [
                    dict(name='SUPPORTED_BY', source='AgentFact', target='AgentEvidence', predicate='source.project_id = target.project_id AND list_contains(source.supporting_evidence_ids, target.record_id)'),
                    dict(name='REVISION_OF', source='AgentEvidence', target='SlackMessage', predicate="source.project_id = target.project_id AND source.source_node_type = 'SlackMessage' AND source.source_id = target.record_id"),
                ],
                'edges': [dict(label='PHYSICAL', table='links', id=key, source=['project_id','src'] if composite else 'src', target=['project_id','dst'] if composite else 'dst', source_label='AgentFact', target_label='AgentEvidence', source_query="SELECT * FROM links WHERE project_id = '1'")],
            }
            with Connection(engine, schema) as graph:
                for edge in ['SUPPORTED_BY', 'PHYSICAL']:
                    for hops in [1, 2]:
                        with self.subTest(edge=edge, hops=hops):
                            query = f'MATCH (f:AgentFact)-[:{edge}]->(e:AgentEvidence)'
                            if hops == 2:
                                query += '-[:REVISION_OF]->(s:SlackMessage)'
                            query += ' RETURN f.text AS claim, f.evidence AS evidence'
                            if hops == 2:
                                query += ', s.text AS source'
                            with graph.query(query, language='cypher') as batches:
                                rows = [r for b in batches for r in b.to_pylist()]
                            expected = dict(claim='Uses Postgres', evidence=citation)
                            if hops == 2:
                                expected['source'] = 'Use Postgres.'
                            self.assertEqual(rows, [expected])
        finally:
            db.close()

    def test_nested_struct_transport_types(self):
        from decimal import Decimal
        import pyarrow as pa
        from orchiddb.execution import _logical_arrow_type, _postgres_json_value

        nested = 'struct_fields:' + json.dumps([
            ['z', 'int64'], ['a', 'boolean'], ['amount', 'decimal:20:2'],
        ])
        ty = _logical_arrow_type('list:' + nested, pa)
        self.assertEqual(ty.value_type.names, ['z', 'a', 'amount'])
        value = [{'z': '9007199254740993', 'a': 'false', 'amount': '123.45'}, None]
        decoded = _postgres_json_value(value, ty, pa)
        self.assertEqual(pa.array([decoded], type=ty).to_pylist(), [[
            {'z': 9007199254740993, 'a': False, 'amount': Decimal('123.45')}, None,
        ]])

    @unittest.skipUnless(os.getenv('ORCHIDDB_TEST_PG_URL'), 'Set ORCHIDDB_TEST_PG_URL')
    def test_postgres_text_extrema_ignore_database_locale(self):
        import psycopg
        with psycopg.connect(os.environ['ORCHIDDB_TEST_PG_URL']) as db:
            with Connection(PostgresEngine(db), {'tables': []}) as graph:
                with graph.query("UNWIND ['a', 'b', 'B', null, 'abc', 'abc1'] AS i RETURN min(i) AS lo, max(i) AS hi", language='cypher') as batches:
                    self.assertEqual([r for b in batches for r in b.to_pylist()], [{'lo': 'B', 'hi': 'b'}])

    def test_duckdb_scalar(self):
        self.check_traversals('duckdb', False)

    def test_duckdb_composite(self):
        self.check_traversals('duckdb', True)

    @unittest.skipUnless(os.getenv('ORCHIDDB_TEST_PG_URL'), 'Set ORCHIDDB_TEST_PG_URL')
    def test_postgres_scalar(self):
        self.check_traversals('postgres', False)

    @unittest.skipUnless(os.getenv('ORCHIDDB_TEST_PG_URL'), 'Set ORCHIDDB_TEST_PG_URL')
    def test_postgres_composite(self):
        self.check_traversals('postgres', True)


if __name__ == '__main__':
    unittest.main()
