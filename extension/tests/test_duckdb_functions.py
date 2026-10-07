"""The caller's DuckDB catalog owns ordinary scalar and aggregate functions."""
import json
import unittest

from test_extension import execute_query, describe_query, prepare_query, connect, fixture


class DuckDBFunctionTests(unittest.TestCase):
    def setUp(self):
        self.db = connect()
        fixture(self.db)
        self.db.execute('CREATE PROPERTY GRAPH managed_functions')

    def tearDown(self):
        self.db.close()

    def managed(self, query):
        request = json.dumps(dict(version=1, dialect='duckdb', language='cypher',
                                  query=query, tables=[], managed_table='main.managed_functions'))
        rows = execute_query(self.db, 'SELECT __orchiddb_value_json(v) FROM orchid_query(?)', request).fetchall()
        values = [json.loads(row[0]) for row in rows]
        return [float(v['value']) if v['type'] in ('double','float') else v.get('value') for v in values]

    def test_portable_functions_coexist_with_caller_catalog(self):
        self.db.execute('CREATE MACRO twice(x) AS x*2')
        self.assertEqual(self.db.execute(
            'CYPHER social MATCH (p:Person) RETURN fn.coalesce(p.name, \'unknown\'), twice(p.age) ORDER BY p.name'
        ).fetchall(), [('Alice', 60), ('Bob', 80), ('Cara', None)])
        self.assertEqual(self.managed("RETURN fn.lower('ALICE') AS v"), ['alice'])

    def test_portable_conditionals_keep_residual_branches_lazy(self):
        self.db.execute("CREATE MACRO explode() AS error('unused branch was executed')")
        for expression in [
            "fn.coalesce('safe', explode())",
            "fn.nvl('safe', explode())",
            "fn.ifnull('safe', explode())",
            "fn.nvl2(null, explode(), 'safe')",
            "fn.nvl2(1, 'safe', explode())",
        ]:
            self.assertEqual(self.managed(f'RETURN {expression} AS v'), ['safe'], expression)

    def test_catalog_builtins_without_signatures(self):
        expected = self.db.execute("SELECT version(), unicode('A'), bar(5,0,10,10)").fetchone()
        self.assertEqual(self.db.execute("CYPHER social RETURN version(), unicode('A'), bar(5,0,10,10)").fetchone(), expected)
        self.assertEqual(self.managed("RETURN unicode('A') AS v"), [65])

    def test_macros_mapped_and_residual(self):
        self.db.execute('CREATE MACRO twice(x) AS x*2')
        self.assertEqual(self.db.execute('CYPHER social MATCH (p:Person) RETURN twice(p.age) ORDER BY p.age').fetchall(), [(60,), (80,), (None,)])
        self.assertEqual(self.managed('UNWIND [1,2,3] AS x RETURN twice(x) AS v'), [2,4,6])
        self.assertEqual(self.managed('UNWIND [[1,2],[3,4]] AS x RETURN list_dot_product(x,x) AS v'), [5.0,25.0])

    def test_catalog_changes_and_search_path(self):
        self.db.execute('CREATE SCHEMA custom; CREATE MACRO custom.offset_by(x) AS x+7')
        self.db.execute("SET search_path='custom,main'")
        self.assertEqual(self.db.execute('CYPHER social RETURN offset_by(3)').fetchone(), (10,))
        self.assertEqual(self.managed('RETURN custom.offset_by(3) AS v'), [10])
        self.db.execute('CREATE OR REPLACE MACRO custom.offset_by(x) AS x+9')
        self.assertEqual(self.managed('RETURN custom.offset_by(3) AS v'), [12])
        self.db.execute('DROP MACRO custom.offset_by')
        with self.assertRaises(Exception):
            self.managed('RETURN custom.offset_by(3) AS v')

    def test_native_aggregates_in_sql_and_residual_groups(self):
        self.assertEqual(self.db.execute('CYPHER social MATCH (p:Person) RETURN product(p.age)').fetchone(), (1200.0,))
        self.assertEqual(self.managed('UNWIND [2,3,4] AS x RETURN product(x) AS v'), [24.0])
        self.assertEqual(self.managed('UNWIND [2,2,3] AS x RETURN product(DISTINCT x) AS v'), [6.0])
        self.assertEqual(self.managed('UNWIND [1,2,3,4] AS x RETURN quantile_cont(x,0.5) AS v'), [2.5])
        self.assertEqual(self.managed('UNWIND [] AS x RETURN product(x) AS v'), [None])

    def test_binding_never_evaluates_volatile_calls(self):
        self.db.execute('CREATE SEQUENCE fn_sequence START 1; CREATE MACRO ticket() AS nextval(\'fn_sequence\')')
        self.db.execute('EXPLAIN CYPHER social RETURN ticket()').fetchall()
        self.assertEqual(self.db.execute("SELECT nextval('fn_sequence')").fetchone(), (1,))
        self.assertEqual(self.managed('UNWIND [1,2,3] AS x RETURN ticket() AS v'), [2,3,4])
        self.db.execute('CREATE MACRO explode() AS error(\'function was executed\')')
        self.db.execute('EXPLAIN CYPHER social RETURN explode()').fetchall()
        with self.assertRaisesRegex(Exception, 'function was executed'):
            self.managed('RETURN explode() AS v')

    def test_nulls_and_nested_results(self):
        self.db.execute("CREATE MACRO null_default(x) AS coalesce(x,99); CREATE MACRO payload(x) AS struct_pack(number:=x,items:=[x,x+1])")
        self.assertEqual(self.managed('RETURN null_default(null) AS v'), [99])
        self.assertEqual(self.db.execute('CYPHER social RETURN payload(4)').fetchone(), ({'number':4,'items':[4,5]},))
        self.assertIsNotNone(self.managed('RETURN payload(4) AS v')[0])
        self.db.execute('CREATE MACRO payload_number(p) AS p.number')
        self.assertEqual(self.managed('RETURN payload_number(payload(4)) AS v'), [4])

    def test_loaded_extension_functions(self):
        self.db.execute('LOAD icu')
        expected=self.db.execute("SELECT icu_sort_key('hello','en')").fetchone()[0]
        self.assertEqual(self.db.execute("CYPHER social RETURN icu_sort_key('hello','en')").fetchone(), (expected,))
        self.assertEqual(self.managed("RETURN icu_sort_key('hello','en') AS v"), [expected])

    def test_computed_edges_reuse_immutable_macros(self):
        self.db.execute('CREATE MACRO age_gap(a,b) AS abs(a-b)')
        ddl = """CREATE PROPERTY GRAPH peers VERTEX TABLES
          (people KEY(id) LABEL Person PROPERTIES(name,age))
          COMPUTED EDGES (PEER SOURCE Person DESTINATION Person
          WHERE (source.name <> target.name)
          PROPERTIES (age_gap(source.age,target.age) AS gap)
          ORDER BY (gap) LIMIT PER SOURCE 1)"""
        self.db.execute(ddl)
        self.assertEqual(self.db.execute("CYPHER peers MATCH (a:Person {name:'Alice'})-[e:PEER]->(b) RETURN b.name,e.gap").fetchall(), [('Bob',10)])
        self.db.execute('CREATE OR REPLACE MACRO age_gap(a,b) AS random()')
        with self.assertRaisesRegex(Exception, 'immutable'):
            self.db.execute(ddl.replace('CREATE PROPERTY','CREATE OR REPLACE PROPERTY'))

    def test_caller_registered_udf(self):
        self.db.create_function('host_add', lambda value: value + 4, ['BIGINT'], 'BIGINT')
        self.assertEqual(self.db.execute('CYPHER social RETURN host_add(8)').fetchone(), (12,))
        self.assertEqual(self.managed('UNWIND [1,2] AS x RETURN host_add(x) AS v'), [5,6])

    def test_table_functions_remain_relations(self):
        with self.assertRaises(Exception):
            self.db.execute('CYPHER social RETURN duckdb_tables()')
        self.db.execute('CREATE VIEW generated AS SELECT * FROM range(3) AS r(id)')
        self.db.execute('CREATE PROPERTY GRAPH generated_graph VERTEX TABLES (generated KEY(id) LABEL Number PROPERTIES(id))')
        self.assertEqual(self.db.execute('CYPHER generated_graph MATCH (n:Number) RETURN n.id ORDER BY n.id').fetchall(), [(0,),(1,),(2,)])


if __name__ == '__main__':
    unittest.main()
