"""Shared SPARQL effects executed in the calling DuckDB transaction."""
import json
import unittest

from test_extension import connect


class UpdateTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.db.execute('CREATE TABLE terms(g VARCHAR,s VARCHAR,sk VARCHAR,sd VARCHAR,sl VARCHAR,p VARCHAR,pk VARCHAR,pd VARCHAR,pl VARCHAR,o VARCHAR,ok VARCHAR,od VARCHAR,ol VARCHAR)')
        self.db.execute('CREATE TABLE graph_names(iri VARCHAR PRIMARY KEY)')
        self.request = dict(version=1, dialect='duckdb', language='sparql', tables=[{'name':'terms'},{'name':'graph_names'}],
            rdf_sources=[dict(table='terms',subject_column='s',predicate_column='p',object_column='o',graph_column='g',writable=True,
                typed_terms=[dict(value=p,kind=p+'k',datatype=p+'d',language=p+'l') for p in ('s','p','o')])],
            rdf_graph_names=dict(table='graph_names',column='iri',writable=True))

    def tearDown(self):
        self.db.close()

    def encoded(self, query):
        return json.dumps(dict(self.request, query=query))

    def update(self, query):
        return self.db.execute('CALL orchid_sparql_update(?)', [self.encoded(query)]).fetchall()

    def test_update_reads_own_writes_and_rolls_back_with_caller(self):
        self.db.execute('BEGIN')
        self.update('INSERT DATA { <urn:s> <urn:p> "first" }')
        self.update('DELETE {?s ?p ?o} INSERT {?s ?p "second"} WHERE {?s ?p ?o}')
        self.assertEqual(self.db.execute('SELECT o FROM terms').fetchall(), [('second',)])
        self.db.execute('ROLLBACK')
        self.assertEqual(self.db.execute('SELECT count(*) FROM terms').fetchone(), (0,))

    def test_update_error_is_atomic_and_connection_recovers(self):
        with self.assertRaisesRegex(Exception, 'already exists'):
            self.update('INSERT DATA { <urn:s> <urn:p> "rollback" }; CREATE GRAPH <urn:g>; CREATE GRAPH <urn:g>')
        self.assertEqual(self.db.execute('SELECT count(*) FROM terms').fetchone(), (0,))
        self.assertEqual(self.db.execute('SELECT count(*) FROM graph_names').fetchone(), (0,))
        self.update('INSERT DATA { <urn:s> <urn:p> "kept" }')
        self.assertEqual(self.db.execute('SELECT o FROM terms').fetchall(), [('kept',)])

    def test_explain_and_prepare_do_not_apply_effects(self):
        request = self.encoded('INSERT DATA { _:b <urn:p> "hello" }')
        self.db.execute('EXPLAIN SELECT * FROM orchid_sparql_update(?)', [request]).fetchall()
        quoted = "'" + request.replace("'", "''") + "'"
        self.db.execute('PREPARE upd AS SELECT * FROM orchid_sparql_update(' + quoted + ')')
        self.assertEqual(self.db.execute('SELECT count(*) FROM terms').fetchone(), (0,))
        for _ in range(2):
            self.db.execute('EXECUTE upd').fetchall()
        self.assertEqual(self.db.execute('SELECT count(DISTINCT s) FROM terms').fetchone(), (2,))

    def test_with_preserves_explicit_named_graph_and_typed_aggregate(self):
        self.update('INSERT DATA { GRAPH <urn:g> { <urn:s> <urn:p> "x" } }')
        self.update('WITH <urn:other> DELETE {GRAPH <urn:g> {?s ?p ?o}} WHERE {GRAPH <urn:g> {?s ?p ?o}}')
        self.assertEqual(self.db.execute('SELECT count(*) FROM terms').fetchone(), (0,))
        self.update('INSERT {<urn:s> <urn:count> ?n} WHERE {SELECT (COUNT(*) AS ?n) WHERE {?s ?p ?o}}')
        self.assertEqual(self.db.execute('SELECT o,od FROM terms').fetchone(), ('0','http://www.w3.org/2001/XMLSchema#integer'))

    def test_writes_require_explicit_writable_source(self):
        self.request['rdf_sources'][0]['writable'] = False
        with self.assertRaisesRegex(Exception, 'read-only'):
            self.update('INSERT DATA { <urn:s> <urn:p> "x" }')
        self.assertEqual(self.db.execute('SELECT count(*) FROM terms').fetchone(), (0,))


if __name__ == '__main__': unittest.main(verbosity=2)
