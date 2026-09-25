"""Result-file decoding must preserve the upstream assertion order."""
import tempfile
import unittest
from pathlib import Path
from sparql import expected


class ResultFileTests(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
