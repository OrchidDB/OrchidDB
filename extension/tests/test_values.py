"""Shared graph scalar and collection kernels inside DuckDB expressions."""
import json
import unittest

from test_extension import connect, fixture


class ValueTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)

    def tearDown(self):
        self.db.close()

    def query(self, query):
        request = dict(version=1,dialect='duckdb',language='cypher',tables=[],query=query)
        result = self.db.execute('SELECT __orchiddb_value_json(r) FROM orchid_query(?) r',[json.dumps(request)]).fetchall()
        return [{k['value']:v for k,v in json.loads(row[0])['value']} for row in result]

    def test_temporal_identity_and_components_cross_with(self):
        row = self.query("WITH datetime('2000-01-01T12:00:00.123456789+01:00') AS d RETURN d, d.nanosecond AS ns, d.year AS y")[0]
        self.assertEqual(row['d'],dict(type='cypher_temporal',value=dict(kind='datetime',text='2000-01-01T12:00:00.123456789+01:00')))
        self.assertEqual(row['ns']['value'],123456789)
        self.assertEqual(row['y']['value'],2000)

    def test_nested_values_and_dynamic_unwind_preserve_types(self):
        rows = self.query('WITH {items:[1,"one",null,{ok:true}]} AS m UNWIND m.items AS x RETURN x')
        self.assertEqual([row['x']['type'] for row in rows],['int','string','null','map'])
        self.assertEqual(rows[3]['x']['value'],[[dict(type='string',value='ok'),dict(type='boolean',value=True)]])
        self.assertEqual(self.query('UNWIND [] AS x RETURN x'),[])
        self.assertEqual(self.query('UNWIND null AS x RETURN x'),[])

    def test_heterogeneous_aggregate_uses_shared_numeric_semantics(self):
        row=self.query('UNWIND [1,2.0,5,null,0.1] AS x RETURN min(x) AS lo,max(x) AS hi,collect(x) AS xs')[0]
        self.assertEqual(row['lo'],dict(type='double',value='0.1'))
        self.assertEqual(row['hi'],dict(type='int',value=5))
        self.assertEqual(row['xs']['value'],[dict(type='int',value=1),dict(type='double',value='2'),dict(type='int',value=5),dict(type='double',value='0.1')])

    def test_sort_uses_shared_comparator_and_keeps_row_association(self):
        rows=self.query('UNWIND [{n:2,x:time("12:00+05:00")},{n:1,x:time("10:00-08:00")}] AS m RETURN m.n AS n,m.x AS x ORDER BY x')
        self.assertEqual([row['n']['value'] for row in rows],[2,1])
        rows=self.query('UNWIND [[1,"a"],[],[null,1],[2]] AS x RETURN x ORDER BY x')
        self.assertEqual([row['x']['value'] for row in rows],[[],[dict(type='int',value=1),dict(type='string',value='a')],[dict(type='int',value=2)],[dict(type='null',value=None),dict(type='int',value=1)]])

    def test_query_clock_is_shared_between_scalar_calls(self):
        self.assertEqual(self.query('RETURN duration.inSeconds(datetime(),datetime()) AS d')[0]['d']['value']['text'],'PT0S')
        self.assertEqual(self.query('RETURN date.transaction(null) AS d')[0]['d']['type'],'null')

    def test_scalar_kernel_retains_host_source_scan_and_transaction(self):
        self.db.execute('BEGIN; UPDATE people SET age=2001 WHERE id=1')
        row=self.db.execute("SELECT __orchiddb_value_json(r) FROM orchid_cypher('social',?) r", ["MATCH (p:Person) WHERE p.name='Alice' RETURN date({year:p.age,month:2,day:3}) AS d"]).fetchone()
        self.assertEqual(json.loads(row[0])['value'][0][1]['value']['text'],'2001-02-03')
        self.db.execute('ROLLBACK')

    def gremlin(self, query):
        rows=self.db.execute("SELECT __orchiddb_value_json(q.current) FROM orchid_gremlin('social',?) q",[query]).fetchall()
        return [json.loads(row[0]) for row in rows]

    def test_graph_elements_survive_collections_with_identity_and_properties(self):
        nodes=self.gremlin('g.V().fold().unfold()')
        self.assertEqual(sorted(v['id']['value'] for v in nodes),[1,2,3])
        self.assertEqual({v['properties']['name']['value'] for v in nodes},{'Alice','Bob','Cara'})
        edges=self.gremlin('g.E().fold().unfold()')
        self.assertEqual({v['id']['value'] for v in edges},{10,11,12})
        self.assertTrue(all(v['outVLabel']=='Person' and v['inVLabel']=='Person' for v in edges))
        self.assertEqual({v['value'] for v in self.gremlin('g.V().fold().unfold().values("name")')},{'Alice','Bob','Cara'})

    def test_group_maps_remain_typed_and_support_downstream_steps(self):
        result=self.gremlin('g.V().groupCount().by(label)')[0]
        self.assertEqual(result,dict(type='map',value=[[dict(type='string',value='Person'),dict(type='long',value=3)]]))
        self.assertEqual(self.gremlin('g.V().groupCount().by(label).unfold().select(values)'),[dict(type='long',value=3)])
        values=self.gremlin('g.V().has("name","Alice").valueMap("age")')[0]
        self.assertEqual(values,dict(type='map',value=[[dict(type='string',value='age'),dict(type='list',value=[dict(type='int',value=30)])]]))

    def test_paths_use_shared_graph_value_semantics(self):
        paths=self.gremlin('g.V().has("name","Alice").out().path().by("name")')
        expected=dict(type='path',value=[dict(type='string',value='Alice'),dict(type='string',value='Bob')])
        self.assertEqual(paths,[expected,expected])
        groups=self.gremlin('g.V().out().groupCount().by(label)')
        self.assertEqual(groups,[dict(type='map',value=[[dict(type='string',value='Person'),dict(type='long',value=3)]])])

    def test_composite_element_keys_survive_typed_transport(self):
        self.db.execute("CREATE TABLE accounts(tenant BIGINT,id BIGINT,name VARCHAR); INSERT INTO accounts VALUES(2,7,'Ada')")
        self.db.execute('CREATE PROPERTY GRAPH accounts_graph VERTEX TABLES(accounts KEY(tenant,id) LABEL Account PROPERTIES(name))')
        row=self.db.execute("SELECT __orchiddb_value_json(q.n) FROM orchid_cypher('accounts_graph','MATCH (n) RETURN n') q").fetchone()
        node=json.loads(row[0])
        self.assertEqual(node['id'],dict(type='list',value=[dict(type='long',value=2),dict(type='long',value=7)]))
        self.assertEqual(node['properties']['name'],dict(type='string',value='Ada'))

    def test_registered_procedure_uses_shared_argument_and_result_types(self):
        procedure=dict(inputs=[dict(name='number',type='INTEGER',nullable=False)],outputs=[dict(name='word',type='STRING',nullable=False)],rows=[[1,'one'],[2,'two']])
        request=dict(version=1,dialect='duckdb',language='cypher',tables=[],procedures={'test.words':procedure},query='UNWIND [2,1] AS n CALL test.words(n) YIELD word RETURN n,word')
        rows=self.db.execute('SELECT __orchiddb_value_json(q.word) FROM orchid_query(?) q',[json.dumps(request)]).fetchall()
        self.assertEqual([json.loads(row[0]) for row in rows],[dict(type='string',value='two'),dict(type='string',value='one')])


if __name__ == '__main__': unittest.main(verbosity=2)
