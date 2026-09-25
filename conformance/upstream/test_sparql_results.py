"""Result-file decoding must preserve the upstream assertion order."""
import tempfile
import unittest
from pathlib import Path
from sparql import expected


class ResultFileTests(unittest.TestCase):
    def test_numeric_assertions_preserve_types_values_and_multiplicity(self):
        from sparql import matchterm, rows_equal
        def literal(value, datatype='double'):
            return {'type':'literal','value':value,'datatype':'http://www.w3.org/2001/XMLSchema#'+datatype,'lang':None}
        self.assertTrue(matchterm(literal('2E-1'),literal('0.2'),{}))
        self.assertFalse(matchterm(literal('2E-1'),literal('0.2000000000001'),{}))
        self.assertFalse(matchterm(literal('1','integer'),literal('1','decimal'),{}))
        self.assertFalse(matchterm(literal('01','string'),literal('1','string'),{}))
        self.assertFalse(matchterm(literal('invalid'),literal('0'),{}))
        self.assertFalse(rows_equal([[literal('1')]],[[literal('1')],[literal('1')]],False))
        self.assertTrue(rows_equal([[literal('1')]],[[literal('1')],[literal('1')]],False,lax=True))
        self.assertFalse(rows_equal([[literal('2')]],[[literal('1')],[literal('1')]],False,lax=True))

    def test_turtle_numeric_tokens_are_not_converted_to_python_numbers(self):
        from rdf_fixtures import graph_file
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'data.ttl'
            path.write_text('<urn:s> <urn:p> 0E1, 1.00, "01"^^<http://www.w3.org/2001/XMLSchema#integer>.')
            self.assertEqual({str(value) for value in graph_file(path).objects()},
                             {'0E1', '1.00', '01'})

    def test_turtle_result_indices_override_serialization_order(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'results.ttl'
            path.write_text('''@prefix rs: <http://www.w3.org/2001/sw/DataAccess/tests/result-set#> .
                [] a rs:ResultSet; rs:resultVariable "v";
                rs:solution [rs:index 2; rs:binding [rs:variable "v"; rs:value "second"]],
                            [rs:index 1; rs:binding [rs:variable "v"; rs:value "first"]].''')
            self.assertEqual([row[0]['value'] for row in expected(path)['rows']],
                             ['first', 'second'])
            self.assertTrue(expected(path)['ordered'])


if __name__ == '__main__':
    unittest.main()
