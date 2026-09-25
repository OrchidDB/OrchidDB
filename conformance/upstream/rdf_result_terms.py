"""Numeric result normalization using the pinned Oxigraph implementation.

Oxigraph v0.5.11 testsuite/src/sparql_evaluator.rs::StaticQueryResults::from_graph
normalizes expected and actual literals by round-tripping through Store. Apply
that rule to numeric solution values, without changing fixtures or raw reports.
Datatype equality remains mandatory; no epsilon or cross-type promotion is used.
"""
from functools import lru_cache
from pyoxigraph import Store, Quad, NamedNode, Literal

XSD = 'http://www.w3.org/2001/XMLSchema#'
NUMERIC = {XSD + name for name in (
    'integer', 'decimal', 'float', 'double', 'long', 'int', 'short', 'byte',
    'nonNegativeInteger', 'positiveInteger', 'nonPositiveInteger', 'negativeInteger',
    'unsignedLong', 'unsignedInt', 'unsignedShort', 'unsignedByte')}

@lru_cache(maxsize=4096)
def numeric_value(lexical, datatype):
    store = Store()
    store.add(Quad(NamedNode('urn:result:s'), NamedNode('urn:result:p'),
                   Literal(lexical, datatype=NamedNode(datatype))))
    return next(iter(store)).object.value

def same_numeric_value(left, right):
    datatype = left.get('datatype')
    return (left.get('type') == right.get('type') == 'literal'
            and datatype == right.get('datatype') and datatype in NUMERIC
            and left.get('lang') == right.get('lang')
            and numeric_value(left['value'], datatype) == numeric_value(right['value'], datatype))
