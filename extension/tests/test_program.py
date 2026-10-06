"""Shared compiled graph programs execute inside the caller's DuckDB transaction."""
import json
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest

from test_extension import connect


class ProgramTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        self.graph = 'main.program_graph'
        self.db.execute('CALL orchid_graph_create(?)', [self.graph])

    def tearDown(self):
        self.db.close()

    def request(self, language, query):
        return json.dumps(dict(version=1, dialect='duckdb', language=language,
                               query=query, tables=[], managed_table=self.graph))

    def query(self, language, query):
        request = self.request(language, query)
        metadata = json.loads(self.db.execute('SELECT * FROM orchid_compile(?)', [request]).fetchone()[0])
        if 'error' in metadata:
            raise AssertionError(metadata['error'])
        if not metadata['fields']:
            self.db.execute('SELECT * FROM orchid_query(?)', [request]).fetchall()
            return []
        quote = lambda value: '"' + value.replace('"', '""') + '"'
        packed = ','.join(quote(name) + ':=result.' + quote(name) for name in metadata['fields'])
        rows = self.db.execute('SELECT __orchiddb_value_json(struct_pack(' + packed +
                               ')) FROM orchid_query(?) AS result', [request]).fetchall()
        return [dict((key['value'], value) for key, value in json.loads(row[0])['value']) for row in rows]

    def cypher(self, query):
        return self.query('cypher', query)

    def gremlin(self, query):
        return [row['current'] for row in self.query('gremlin', query)]

    def snapshot(self):
        return json.loads(self.db.execute('SELECT * FROM orchid_graph_snapshot(?)', [self.graph]).fetchone()[0])

    def import_fixture(self):
        nodes = []
        for identity, label, name in [(1, 'Person', 'Ada'), (2, 'Person', 'Bob'), (3, 'Software', 'Code')]:
            records = [dict(id=identity * 10, id_type='Long', key='name', value=name,
                            type='String', meta={}, meta_types={})]
            if identity == 1:
                records += [dict(id=11 + index, id_type='Long', key='location', value=place,
                                 type='String', meta={'since': since}, meta_types={'since': 'Integer'})
                            for index, (place, since) in enumerate([('Paris', 2000), ('London', 2010)])]
            nodes.append(dict(id=identity, id_type='Integer', label=label, properties={'name': name},
                              property_types={'name': 'String'}, property_records=records))
        edges = [dict(id=identity, id_type='Integer', label='KNOWS', src=src, dst=dst,
                      properties={}, property_types={}) for identity, src, dst in [(101, 1, 2), (102, 2, 3)]]
        fixture = dict(op='fixture', name='program-fixture', nodes=nodes, edges=edges,
                       allow_null_property_values=False)
        self.db.execute('CALL orchid_graph_import(?, ?)', [self.graph, json.dumps(fixture)])

    def test_cypher_writes_merge_and_same_transaction_rollback(self):
        self.cypher("CREATE (:Person {name:'Ada'})")
        before = self.snapshot()
        self.db.execute('BEGIN')
        self.cypher("MERGE (p:Person {name:'Ada'}) ON MATCH SET p.age=40")
        self.cypher("MERGE (p:Person {name:'Bob'}) ON CREATE SET p.age=30")
        rows = self.cypher('MATCH (p:Person) RETURN p.name AS name,p.age AS age ORDER BY name')
        self.assertEqual([(r['name']['value'], r['age']['value']) for r in rows], [('Ada', 40), ('Bob', 30)])
        self.cypher("MATCH (p:Person {name:'Bob'}) DELETE p")
        self.assertEqual(self.cypher('MATCH (p:Person) RETURN count(p) AS n')[0]['n']['value'], 1)
        self.db.execute('ROLLBACK')
        self.assertEqual(self.snapshot(), before)

    def test_explain_prepare_have_no_effect_and_execute_uses_fresh_state(self):
        request = self.request('cypher', 'CREATE (:Run)')
        self.db.execute('EXPLAIN SELECT * FROM orchid_query(?)', [request]).fetchall()
        escaped = request.replace("'", "''")
        self.db.execute("PREPARE create_run AS SELECT * FROM orchid_query('" + escaped + "')")
        self.assertEqual(self.cypher('MATCH (n:Run) RETURN count(n) AS n')[0]['n']['value'], 0)
        self.db.execute('EXECUTE create_run').fetchall()
        self.db.execute('EXECUTE create_run').fetchall()
        self.assertEqual(self.cypher('MATCH (n:Run) RETURN count(n) AS n')[0]['n']['value'], 2)

    def test_failed_statement_is_atomic_and_connection_recovers(self):
        before = self.snapshot()
        failing = 'CREATE (n:Attempt {zero:0}) WITH n RETURN 1/n.zero AS invalid'
        metadata = json.loads(self.db.execute('SELECT * FROM orchid_compile(?)',
                             [self.request('cypher', failing)]).fetchone()[0])
        self.assertNotIn('error', metadata)
        with self.assertRaises(Exception):
            self.cypher(failing)
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(self.cypher('RETURN 7 AS n')[0]['n']['value'], 7)
        self.cypher('CREATE (:Recovered)')
        self.assertEqual(self.cypher('MATCH (n:Recovered) RETURN count(n) AS n')[0]['n']['value'], 1)

    def test_nested_gremlin_subplans_and_side_effect_state(self):
        self.import_fixture()
        values = self.gremlin('g.V(1).coalesce(out("MISSING").fold().unfold(),out("KNOWS")).values("name")')
        self.assertEqual(values, [dict(type='string', value='Bob')])
        paths = self.gremlin('g.V(1).repeat(out("KNOWS")).times(2).path().by("name")')
        self.assertEqual(paths, [dict(type='path', value=[dict(type='string', value=v) for v in ['Ada', 'Bob', 'Code']])])
        result = self.gremlin('g.V().group("a").by(label).by(count()).cap("a")')[0]
        self.assertEqual({key['value']: value['value'] for key, value in result['value']}, {'Person': 2, 'Software': 1})
        self.assertCountEqual(self.gremlin('g.V().hasLabel("Person").aggregate("a").out().cap("a").unfold().values("name")'),
                         [dict(type='string', value='Ada'), dict(type='string', value='Bob')])

    def test_deep_value_round_trip_on_host_stack(self):
        row = self.cypher('RETURN ' + '[' * 40 + ']' * 40 + ' AS value')[0]['value']
        for _ in range(39):
            self.assertEqual(row['type'], 'list')
            self.assertEqual(len(row['value']), 1)
            row = row['value'][0]
        self.assertEqual(row, dict(type='list', value=[]))

    def test_union_preserves_result_contract_and_ordered_branch_effects(self):
        rows = self.cypher('RETURN 1 AS x UNION RETURN 2 AS x UNION RETURN 1 AS x')
        self.assertEqual({r['x']['value'] for r in rows}, {1, 2})
        self.cypher("CREATE (:Person {id:7,name:'Ada'})")
        rows = self.cypher('MATCH (n:Person) RETURN n UNION ALL MATCH (n:Person) RETURN n')
        self.assertEqual(len(rows), 2)
        self.assertEqual(rows[0], rows[1])
        self.db.execute('SET threads=4')
        # A later branch must see the earlier branch's mutation, including when
        # each branch has its own blocking aggregate pipeline.
        values = self.gremlin('g.union(__.addV("Run").count(),__.V().hasLabel("Run").count())')
        self.assertEqual([v['value'] for v in values], [1, 1])
        self.assertEqual(self.gremlin('g.V().hasLabel("Run").count()')[0]['value'], 1)

    def test_multi_properties_meta_properties_and_heterogeneous_endpoints(self):
        self.import_fixture()
        locations = self.gremlin('g.V(1).properties("location").value()')
        self.assertEqual({value['value'] for value in locations}, {'Paris', 'London'})
        years = self.gremlin('g.V(1).properties("location").properties("since").value()')
        self.assertEqual({value['value'] for value in years}, {2000, 2010})
        self.assertEqual(self.gremlin('g.E().count()'), [dict(type='long', value=2)])
        self.assertEqual({value['value'] for value in self.gremlin('g.E().inV().label()')}, {'Person', 'Software'})

    def test_managed_fixture_reset_restores_mutated_data(self):
        self.import_fixture()
        before = self.snapshot()
        self.gremlin('g.V(1).properties("location").drop()')
        self.assertEqual(self.gremlin('g.V(1).properties("location").count()'), [dict(type='long', value=0)])
        self.db.execute('CALL orchid_graph_reset(?)', [self.graph])
        self.import_fixture()
        self.assertEqual(self.snapshot(), before)

    def test_interrupt_stops_native_repeat_and_connection_recovers(self):
        # A subprocess bounds a cancellation regression even if the native
        # kernel stops observing interrupts; it uses the same loaded artifact.
        script = textwrap.dedent('''
            import json
            import threading
            from test_extension import connect
            db = connect()
            db.execute("CALL orchid_graph_create('cancel_graph')")
            def query(language, text):
                request = json.dumps(dict(version=1, dialect='duckdb', language=language,
                    query=text, tables=[], managed_table='cancel_graph'))
                return db.execute('SELECT * FROM orchid_program(?)', [request]).fetchall()
            before = db.execute("SELECT * FROM orchid_graph_snapshot('cancel_graph')").fetchone()
            stop = threading.Event()
            def interrupt():
                while not stop.wait(0.05):
                    db.interrupt()
            worker = threading.Thread(target=interrupt)
            worker.start()
            try:
                query('gremlin', "g.addV('Attempt').repeat(__.identity()).times(1000000000)")
            except Exception as error:
                assert 'interrupt' in str(error).lower() or 'cancel' in str(error).lower(), error
            else:
                raise AssertionError('Native repeat unexpectedly completed')
            finally:
                stop.set()
                worker.join()
            assert db.execute("SELECT * FROM orchid_graph_snapshot('cancel_graph')").fetchone() == before
            assert len(query('gremlin', 'g.inject(7)')) == 1
            query('cypher', 'CREATE (:Recovered)')
            assert db.execute('SELECT 7').fetchone() == (7,)
            db.close()
        ''')
        result = subprocess.run([sys.executable, '-c', script],
                                cwd=Path(__file__).resolve().parent,
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main(verbosity=2)
