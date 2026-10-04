"""Map ArcadeDB 26.9.1 diagnostics without consulting queries or expectations.

Phase is observed independently through EXPLAIN. Native semantic/syntax exception
classes remain distinct, so engine differences from the TCK stay failures.
"""
import re

DETAILS = set('''AmbiguousAggregationExpression ColumnNameConflict CreatingVarLength
DeleteConnectedNode DeletedEntityAccess RequiresDirectedRelationship RelationshipUniquenessViolation DifferentColumnsInUnion FloatingPointOverflow
IntegerOverflow InvalidAggregation InvalidArgumentType InvalidArgumentValue
InvalidClauseComposition InvalidDelete InvalidElementAccess InvalidNumberLiteral
InvalidParameterUse InvalidPropertyType InvalidRelationshipPattern InvalidUnicodeCharacter
InvalidUnicodeLiteral ListElementAccessByNonInteger MapElementAccessByNonString
MergeReadOwnWrites NegativeIntegerArgument NestedAggregation NoExpressionAlias
NoSingleRelationshipType NoVariablesInScope NonConstantExpression NumberOutOfRange
ProcedureNotFound UndefinedVariable UnexpectedSyntax UnknownFunction VariableAlreadyBound
VariableTypeConflict InvalidNumberOfArguments'''.split())


def classify(diagnostics, phase):
    if phase not in ('compile time', 'runtime'):
        return None
    for diagnostic in reversed(diagnostics):
        message = diagnostic.get('message') or ''
        cls = diagnostic.get('class', '').rsplit('.', 1)[-1]
        explicit = re.match(r'(TypeError|SyntaxError|SemanticError|ProcedureError):\s*', message)
        kind = explicit[1] if explicit else {
            'CommandSemanticException': 'SemanticError',
            'CommandParsingException': 'SyntaxError',
            'IllegalArgumentException': 'TypeError',
            'CommandExecutionException': 'RuntimeError',
        }.get(cls)
        body = message[explicit.end():] if explicit else message
        token = re.match(r'([A-Za-z]+)(?::|\s+-)', body)
        detail = token[1] if token and token[1] in DETAILS else None
        if detail is None:
            if message.startswith(('Unexpected input', 'Syntax error')):
                detail = 'UnexpectedSyntax'
            elif message.startswith('Unknown procedure/function:'):
                kind, detail = 'ProcedureError', 'ProcedureNotFound'
            elif message.startswith(('Unknown function:', 'Unknown SQL function:')):
                detail = 'UnknownFunction'
            elif re.match(r"(?:Procedure|Function) '[^']+' expects .* arguments? but got \d+$", message):
                detail = 'InvalidNumberOfArguments'
            elif 'list index must be an integer' in message.lower() or 'list index must be a number' in message.lower():
                detail = 'ListElementAccessByNonInteger'
            elif 'map key must be a string' in message.lower():
                detail = 'MapElementAccessByNonString'
            elif message.startswith(('Arithmetic operations require numeric operands', 'Type mismatch:')):
                detail = 'InvalidArgumentType'
            elif message.startswith('range() step cannot be zero'):
                detail = 'NumberOutOfRange'
            elif message.startswith('TypeError:'):
                detail = 'InvalidArgumentType'
        if kind and detail:
            return {'type': kind, 'detail': detail, 'phase': phase,
                    'origin': 'ArcadeDB native exception class and diagnostic'}
    return None
