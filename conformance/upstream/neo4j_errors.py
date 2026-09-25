"""Translate observed Neo4j diagnostics to TCK detail names.

No query, scenario, or expected assertion is an input. Neo4j status titles are
retained, including status/phase differences from the pinned openCypher TCK.
Unknown diagnostics remain unclassified. Phase comes from a separate EXPLAIN,
as in Neo4j's own TCK adapter (see neo4j-adapter.md for source references).
"""
import re

# Specific GQL conditions from Neo4j 2026.09 GqlStatusInfoCodes.java.
DETAILS = {
    '42I13': 'InvalidNumberOfArguments', '42I14': 'NoSingleRelationshipType',
    '42I18': 'AmbiguousAggregationExpression', '42N21': 'NoExpressionAlias',
    '42N23': 'InvalidAggregation', '42N29': 'UndefinedVariable',
    '42N32': 'InvalidParameterUse', '42N38': 'ColumnNameConflict',
    '42N39': 'DifferentColumnsInUnion', '42N44': 'UndefinedVariable',
    '42N48': 'UnknownFunction', '42N57': 'InvalidClauseComposition',
    '42N59': 'VariableAlreadyBound', '42N62': 'UndefinedVariable',
    '42N78': 'VariableAlreadyBound', '25N13': 'DeletedEntityAccess',
    'G1001': 'DeleteConnectedNode',
}

def diagnostics(error):
    result = []
    seen = set()
    while error is not None and id(error) not in seen:
        seen.add(id(error))
        result.append({'status': getattr(error, 'gql_status', None),
                       'description': getattr(error, 'gql_status_description', None)})
        error = error.__cause__
    return result

def classify(code, message, phase, gql=()):
    if not code or phase not in ('compile time', 'runtime'):
        return None
    kind = code.rsplit('.', 1)[-1]
    detail = detail_for(message, gql)
    return {'type': kind, 'detail': detail, 'phase': phase} if detail else None

def detail_for(message, gql):
    # More specific observations precede broad type/syntax conditions.
    if 'defined with conflicting type' in message:
        return 'VariableTypeConflict'
    if 'list index must be given as Integer' in message or 'non-integer number index' in message:
        return 'ListElementAccessByNonInteger'
    if 'map key must be given as String' in message or ('Cannot access a map' in message and 'by key' in message):
        return 'MapElementAccessByNonString'
    if 'is not a collection or a map' in message:
        return 'InvalidElementAccess'
    if 'Property values can only be of primitive types' in message:
        return 'InvalidPropertyType'
    if message.startswith('Type mismatch: expected') or 'Coercion of list to boolean' in message:
        return 'InvalidArgumentType'
    if message.startswith('Invalid input for function'):
        return 'InvalidArgumentValue'
    if 'non-negative integer' in message:
        return 'NegativeIntegerArgument' if re.search(r"'-\d+'", message) else 'InvalidArgumentType'
    if 'step argument' in message.lower() and 'cannot be zero' in message:
        return 'NumberOutOfRange'
    if 'must be a number in the range' in message:
        return 'NumberOutOfRange'
    if 'null property value' in message and message.startswith('Cannot merge'):
        return 'MergeReadOwnWrites'
    if 'non-deterministic (random) functions' in message or 'not allowed to refer to variables in' in message:
        return 'NonConstantExpression'
    if 'aggregate functions inside of aggregate functions' in message:
        return 'NestedAggregation'
    if 'integer is too large' in message:
        return 'IntegerOverflow'
    if 'floating point number is too large' in message:
        return 'FloatingPointOverflow'
    if 'invalid literal number' in message:
        return 'InvalidNumberLiteral'
    if 'hexadecimal digits specifying a unicode character' in message:
        return 'InvalidUnicodeLiteral'
    if message.startswith(("Invalid input '—'", "Invalid input '–'")):
        return 'InvalidUnicodeCharacter'
    if 'Only directed relationships are supported' in message:
        return 'RequiresDirectedRelationship'
    if 'Variable length relationships cannot be used' in message:
        return 'CreatingVarLength'
    if "DELETE doesn't support removing labels" in message:
        return 'InvalidDelete'
    if 'Invalid combination of UNION and UNION ALL' in message:
        return 'InvalidClauseComposition'
    if 'Invalid use of aggregating function' in message:
        return 'InvalidAggregation'
    if 'Illegal aggregation expression(s) in order by' in message:
        return 'AmbiguousAggregationExpression'
    if 'RETURN * is not allowed when there are no variables' in message:
        return 'NoVariablesInScope'
    if 'There is no procedure with the name' in message:
        return 'ProcedureNotFound'
    for item in reversed(gql):
        if item.get('status') in DETAILS:
            return DETAILS[item['status']]
    if message.startswith('Invalid input'):
        # These grammar errors concern a relationship length specification.
        if "'WHERE', ']', '{'" in message:
            return 'InvalidRelationshipPattern'
        return 'UnexpectedSyntax'
    if 'A pattern expression should only be used' in message:
        return 'UnexpectedSyntax'
    return None
