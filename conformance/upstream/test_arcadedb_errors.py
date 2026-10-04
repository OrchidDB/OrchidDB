import unittest
from arcadedb_errors import classify


class ArcadeDBDiagnosticTests(unittest.TestCase):
    def diagnostic(self, name, message, phase='compile time'):
        return classify([{'class': 'com.arcadedb.exception.' + name, 'message': message}], phase)

    def test_semantic_and_syntax_errors_stay_distinct(self):
        for cls, kind in [('CommandSemanticException', 'SemanticError'), ('CommandParsingException', 'SyntaxError')]:
            result = self.diagnostic(cls, 'UndefinedVariable: Variable x not defined')
            self.assertEqual(result['type'], kind)
            self.assertEqual(result['detail'], 'UndefinedVariable')

    def test_native_arity_diagnostic_is_not_assigned_a_compile_phase(self):
        result = self.diagnostic('CommandSemanticException', "Procedure 'test.proc' expects 2 arguments but got 1", 'runtime')
        self.assertEqual((result['type'], result['detail'], result['phase']), ('SemanticError', 'InvalidNumberOfArguments', 'runtime'))

    def test_explicit_type_error_and_nested_cause(self):
        result = classify([
            {'class': 'com.arcadedb.exception.CommandExecutionException', 'message': 'Error executing command'},
            {'class': 'java.lang.IllegalArgumentException', 'message': 'TypeError: InvalidArgumentType - invalid value'},
        ], 'runtime')
        self.assertEqual((result['type'], result['detail']), ('TypeError', 'InvalidArgumentType'))

    def test_source_prefixed_pattern_errors(self):
        for detail in ('RequiresDirectedRelationship', 'RelationshipUniquenessViolation'):
            self.assertEqual(self.diagnostic('CommandParsingException', detail + ': pattern rejected')['detail'], detail)

    def test_index_error_and_unknown_diagnostic(self):
        result = classify([{'class': 'java.lang.IllegalArgumentException', 'message': 'List index must be a number, got: String'}], 'runtime')
        self.assertEqual(result['detail'], 'ListElementAccessByNonInteger')
        self.assertIsNone(self.diagnostic('CommandExecutionException', 'Unrecognized internal failure', 'runtime'))
        self.assertIsNone(self.diagnostic('CommandParsingException', 'UndefinedVariable: x', 'unknown'))
