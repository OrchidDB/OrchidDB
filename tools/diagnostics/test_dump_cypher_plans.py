import unittest
from dump_cypher_plans import classify, statements


class ExecutionEvidenceTests(unittest.TestCase):
    def test_missing_and_compile_only_are_not_execution(self):
        self.assertEqual(classify(None), 'no_plan_captured')
        self.assertEqual(classify({'capture': 'compiled', 'fully_lowered_to_sql': True}), 'no_plan_captured')

    def test_orchid_initiated_source_sql_is_not_runtime_only(self):
        self.assertEqual(classify({'capture':'executed','native_source_queries':['SELECT 1']}), 'sql_with_orchid_orchestration')
        self.assertEqual(classify({'capture':'executed','physical_plan':'Procedure'}), 'orchid_runtime')

    def test_decode_only_is_distinguished_from_procedure_work(self):
        plan = {'capture':'executed','sql_queries':['SELECT 1'], 'physical_plan':'Pruned(DecodeTraversers)\n  DuckDbExec\n'}
        self.assertEqual(classify(plan), 'sql_with_result_decoding')
        plan['physical_plan'] += '    Procedure\n'
        self.assertEqual(classify(plan), 'sql_with_orchid_orchestration')

    def test_write_sql_is_exported_as_sql_without_losing_original_record(self):
        record = {'sql':'INSERT INTO t SELECT * FROM bound', 'inputs':[['bound',1]]}
        self.assertEqual(list(statements({'write_queries':[record]})), [('write_queries',record['sql'])])
        self.assertEqual(record['inputs'], [['bound',1]])


if __name__ == '__main__': unittest.main()
