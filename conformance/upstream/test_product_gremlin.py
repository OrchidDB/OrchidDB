import unittest
from product_gremlin import execution_path, ProductGremlin

class ProductGremlinTests(unittest.TestCase):
    def case(self, text='the traversal of', doc='g.V()', tags=()):
        return {'tags': tags, 'steps': [{'text': text, 'doc': doc}]}

    def test_required_interface_is_selected_from_inputs(self):
        self.assertEqual(execution_path(self.case()), 'crabgraph')
        self.assertEqual(execution_path(self.case(tags=['@GraphComputerOnly'])), 'crabgraph-computer')
        self.assertEqual(execution_path(self.case(text='an unsupported test')), 'crabgraph-jvm')
        for value in ['c[it.get()]', 'e[a-knows->b]', 'l[e[a-knows->b],e[b-knows->c]]', 's[]']:
            self.assertEqual(execution_path(self.case(text='using the parameter x defined as "'+value+'"')), 'crabgraph-jvm')
        self.assertEqual(execution_path(self.case(doc='g.V().map(Lambda.function("it.get()"))')), 'crabgraph-jvm')
        self.assertEqual(execution_path(self.case(doc='g.V().has("x", "Lambda.function(foo)")')), 'crabgraph')
        self.assertEqual(execution_path(self.case(text='using the parameter x defined as "e[a-knows->b].id"')), 'crabgraph')

    def test_failed_scenario_executes_once_and_is_not_retried(self):
        calls=[]
        class Adapter:
            classpath='classpath'
            def __init__(self, name): self.name=name
            def run(self, case): calls.append(self.name); return {'status':'fail','error':'assertion mismatch'}
            def close(self): pass
        adapter=ProductGremlin(Adapter)
        result=adapter.run(self.case())
        self.assertEqual(calls, ['crabgraph'])
        self.assertEqual(result['status'], 'fail')
        self.assertEqual(result['error'], 'assertion mismatch')
