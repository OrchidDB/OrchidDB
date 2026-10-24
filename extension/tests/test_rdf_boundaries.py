"""RDF result identity and zero-column cardinality through host SQL."""
import unittest
from test_extension import connect  # Adds the shared test transport to sys.path.
from adapter import Extension


class RdfBoundaryTests(unittest.TestCase):
    def test_empty_solution_and_template_blank_identity(self):
        for policy in ('hybrid', 'sql_required'):
            with self.subTest(policy=policy):
                adapter = Extension()
                try:
                    adapter.db.execute("SET orchid_lowering_policy = '" + policy + "'")
                    quads = [[None, 'urn:s', 'IRI', None, None,
                              'urn:p', 'IRI', None, None, 'urn:o', 'IRI', None, None]]
                    def query(text):
                        return adapter.rdf(dict(query=text, quads=quads, named_graphs=['urn:g']))
                    self.assertEqual(query('SELECT * { GRAPH <urn:g> {} }'),
                                     {'variables': [], 'rows': [[]]})
                    self.assertEqual(query('SELECT * { GRAPH <urn:missing> {} }'),
                                     {'variables': [], 'rows': []})
                    self.assertEqual(query('SELECT * { <urn:s> <urn:p>* <urn:o> }'),
                                     {'variables': [], 'rows': [[]]})
                    self.assertEqual(query('SELECT ?o { <urn:s> <urn:p> ?o . { FILTER(?o = <urn:o>) } }'),
                                     {'variables': ['o'], 'rows': []})
                    result = query('CONSTRUCT { _:b <urn:subject> ?s; <urn:object> ?o } WHERE { ?s ?p ?o }')['graph']
                    self.assertEqual(len(result), 2)
                    self.assertEqual(result[0][0], result[1][0])
                    self.assertEqual(result[0][0]['type'], 'bnode')
                finally:
                    adapter.db.close()
