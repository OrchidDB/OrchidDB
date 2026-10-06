"""Shared compiler frontends executed in the host DuckDB, without runtime fallback."""
import json
import unittest

from test_extension import connect, fixture


class LanguageTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)

    def tearDown(self):
        self.db.close()

    def query(self, language, query, **mapping):
        req = dict(version=1, dialect='duckdb', language=language, query=query, tables=[], **mapping)
        return self.db.execute('SELECT * FROM orchid_query(?)', [json.dumps(req)]).fetchall()

    def test_compiler_owned_constants_and_range(self):
        for query, expected in [
            ('UNWIND [1,2,3] AS x RETURN x ORDER BY x', [(1,), (2,), (3,)]),
            ('UNWIND [] AS x RETURN count(x) AS n', [(0,)]),
            ('UNWIND range(3,1,-1) AS x RETURN x', [(3,), (2,), (1,)]),
            ('MATCH (n:Missing) RETURN count(n) AS n', [(0,)]),
        ]:
            with self.subTest(query=query):
                self.assertEqual(self.db.execute('CYPHER social ' + query).fetchall(), expected)
        self.assertEqual(self.query('cypher', 'UNWIND [1,2,3] AS x RETURN sum(x) AS total'), [(6,)])

    def test_native_gremlin_reuses_graph_and_host_transaction(self):
        self.assertCountEqual(self.db.execute("GREMLIN social g.V().hasLabel('Person').out('FOLLOWS').values('name')").fetchall(), [('Bob',), ('Bob',), ('Cara',)])
        self.db.execute("BEGIN; UPDATE people SET name='Ann' WHERE id=1")
        self.assertEqual(self.db.execute("GREMLIN social g.V(1).values('name')").fetchall(), [('Ann',)])
        self.db.execute('ROLLBACK')
        plan = self.db.execute("EXPLAIN GREMLIN social g.V().values('name')").fetchone()[1]
        self.assertIn('SEQ_SCAN', plan)

    def test_typed_gremlin_bindings_are_data(self):
        value = "x'); DROP TABLE people; //"
        self.assertEqual(self.query('gremlin', 'g.inject(x)', bindings={'x': {'type': 'string', 'value': value}}), [(value,)])
        self.assertEqual(self.db.execute('SELECT count(*) FROM people').fetchone(), (3,))

    def test_sparql_typed_quad_identity_and_named_graphs(self):
        self.db.execute('CREATE TABLE terms(g VARCHAR,s VARCHAR,sk VARCHAR,p VARCHAR,o VARCHAR,ok VARCHAR,dt VARCHAR,lang VARCHAR)')
        xsd = 'http://www.w3.org/2001/XMLSchema#string'
        self.db.executemany('INSERT INTO terms VALUES (?,?,?,?,?,?,?,?)', [
            (None,'urn:s','IRI','urn:p','same','IRI',None,None),
            (None,'urn:s','IRI','urn:p','same','LITERAL',xsd,None),
            ('urn:g','urn:s','IRI','urn:p','bonjour','LITERAL','http://www.w3.org/1999/02/22-rdf-syntax-ns#langString','fr'),
        ])
        source = dict(table='terms',subject_column='s',predicate_column='p',object_column='o',graph_column='g',typed_terms=[
            dict(value='s',kind='sk'), dict(value='p',kind='sk'),dict(value='o',kind='ok',datatype='dt',language='lang')])
        request = dict(version=1, dialect='duckdb',language='sparql',query='SELECT DISTINCT ?o WHERE {?s <urn:p> ?o}',tables=[{'name':'terms'}],rdf_sources=[source])
        cursor = self.db.execute('SELECT * FROM orchid_query(?)', [json.dumps(request)])
        rows = [dict(zip([d[0] for d in cursor.description], row)) for row in cursor.fetchall()]
        self.assertEqual({(r['?o'], r['__rdf:term:kind:?o']) for r in rows}, {('same','IRI'), ('same','LITERAL')})
        request['query'] = 'SELECT ?o WHERE { GRAPH <urn:g> {?s <urn:p> ?o}}'
        cursor = self.db.execute('SELECT * FROM orchid_query(?)', [json.dumps(request)])
        row = dict(zip([d[0] for d in cursor.description], cursor.fetchone()))
        self.assertEqual((row['?o'], row['__rdf:term:language:?o']), ('bonjour','fr'))

    def test_source_schema_and_execution_engine_are_host_owned(self):
        request = dict(version=1,dialect='duckdb',language='cypher',query='MATCH (p:Person) RETURN p.age',
                       tables=[{'name':'people','columns':[{'name':'wrong','data_type':'string'}]}],
                       nodes=[dict(label='Person',table='people',id='id',properties={'age':'age'})])
        self.assertEqual(self.db.execute('SELECT * FROM orchid_query(?)',[json.dumps(request)]).fetchall(),[(30,),(40,),(None,)])
        request['dialect']='postgres'
        with self.assertRaisesRegex(Exception,'host DuckDB'):
            self.db.execute('SELECT * FROM orchid_query(?)',[json.dumps(request)])

    def test_shared_sparql_scalar_kernel(self):
        self.assertEqual(self.query('sparql', '''SELECT (REPLACE("abc", "b", "X") AS ?s) WHERE {}'''), [('aXc',)])
        self.assertEqual(self.query('sparql', '''SELECT (REGEX("ABC", "abc", "i") AS ?ok) WHERE {}'''), [('true',)])
        # Invalid regex and NULL arguments are unbound, not a SQL error.
        self.assertIsNone(self.db.execute("SELECT __orchiddb_sparql_scalar('regex','abc','[','','')").fetchone()[0])
        self.assertIsNone(self.db.execute("SELECT __orchiddb_sparql_scalar('regex',NULL,'a','','')").fetchone()[0])

    def test_compilation_preserves_authoritative_error_diagnostics(self):
        req = dict(version=1,dialect='duckdb',language='cypher',query='RETURN missing',tables=[])
        metadata = json.loads(self.db.execute('SELECT * FROM orchid_compile(?)',[json.dumps(req)]).fetchone()[0])
        self.assertEqual(metadata['classification'], {'type':'SyntaxError','detail':'UndefinedVariable','phase':'compile time'})
        req['query'] = 'CREATE (n) RETURN missing'
        metadata = json.loads(self.db.execute('SELECT * FROM orchid_compile(?)',[json.dumps(req)]).fetchone()[0])
        self.assertIn('error', metadata)
        self.assertEqual(metadata['classification'], {'type':'SyntaxError','detail':'UndefinedVariable','phase':'compile time'})


if __name__ == '__main__': unittest.main(verbosity=2)
