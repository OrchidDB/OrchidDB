"""Lossless RDF fixture parsing; no query or update evaluation.

RDFLib's Turtle parser converts bare double tokens through Python float,
even with NORMALIZE_LITERALS=False. Preserve the original lexical forms
using the Oxigraph RDF parser, then retain RDFLib's graph comparison tools.
"""
from pathlib import Path
from pyoxigraph import parse, RdfFormat, NamedNode, BlankNode
from rdflib import Graph, URIRef, BNode, Literal


def graph_file(path):
    path = Path(path)
    def term(value):
        if isinstance(value, NamedNode):
            return URIRef(value.value)
        if isinstance(value, BlankNode):
            return BNode(value.value)
        return Literal(value.value, lang=value.language,
                       datatype=None if value.language else URIRef(value.datatype.value),
                       normalize=False)
    graph = Graph()
    for quad in parse(path=path, format=RdfFormat.from_extension(path.suffix[1:]),
                      base_iri=path.absolute().as_uri(), rename_blank_nodes=True):
        graph.add(tuple(term(value) for value in (quad.subject, quad.predicate, quad.object)))
    return graph
