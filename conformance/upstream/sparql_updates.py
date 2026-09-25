"""Read W3C update fixtures and compare the complete resulting RDF dataset."""
from functools import lru_cache
from pathlib import Path
from urllib.parse import urlsplit, unquote
import time
import rdflib
from rdflib import Graph, URIRef, BNode
from rdflib.compare import isomorphic
from fetch import CACHE

MF = rdflib.Namespace('http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#')
UT = rdflib.Namespace('http://www.w3.org/2009/sparql/tests/test-update#')

@lru_cache(maxsize=64)
def manifest(path):
    return Graph().parse(path, format='turtle')

def local(uri):
    parts = urlsplit(str(uri))
    if parts.scheme != 'file':
        raise ValueError('Expected a local upstream fixture: ' + str(uri))
    return Path(unquote(parts.path))

def dataset(graph, root):
    from sparql import term
    files = [(value, None) for value in graph.objects(root, UT.data)]
    for item in graph.objects(root, UT.graphData):
        file = graph.value(item, UT.graph)
        name = graph.value(item, rdflib.RDFS.label) or file
        files.append((file, str(name)))
    quads = []
    names = []
    for file, name in files:
        if name is not None: names.append(name)
        for triple in Graph().parse(local(file)):
            quads.append([term(URIRef(name)) if name else None] + [term(value) for value in triple])
    return quads, names

def encoded(quads):
    rows = []
    for graph, *triple in quads:
        row = [graph['value'] if graph else None]
        for value in triple:
            row.extend([value['value'], {'uri':'IRI','bnode':'BLANK','literal':'LITERAL'}[value['type']], value.get('datatype'), value.get('lang')])
        rows.append(row)
    return rows

def dataset_equal(actual, expected):
    # Reification preserves graph names and one global blank-node bijection.
    from sparql import fromterm
    def reify(quads):
        graph = Graph()
        for quad in quads:
            statement = BNode()
            for index, value in enumerate(quad):
                if value is not None:
                    graph.add((statement, URIRef('urn:crabgraph:assertion:quad:' + str(index)), fromterm(value)))
        return graph
    return isomorphic(reify(actual), reify(expected))

def run_update(adapter, case):
    graph = manifest(str(CACHE/'rdf'/case['path']))
    test = URIRef(case['manifest_test'])
    action, result = graph.value(test, MF.action), graph.value(test, MF.result)
    if action is None or result is None:
        return {'status':'adapter-error', 'reason':'Update manifest has no action/result'}
    path = local(graph.value(action, UT.request))
    quads, names = dataset(graph, action)
    want, expected_names = dataset(graph, result)
    query = path.read_text()
    started = time.monotonic()
    actual = adapter.send({'op':'rdf', 'update':True, 'query':query, 'base':path.as_uri(), 'quads':encoded(quads), 'named_graphs':names})
    expected_error = graph.value(result, UT.result) == UT.failure
    passed = ('error' in actual) if expected_error else ('error' not in actual and dataset_equal(actual.get('quads', []), want) and set(actual.get('named_graphs', [])) == set(expected_names))
    return {'status':'pass' if passed else 'fail', 'query':query, 'expected':{'quads':want, 'named_graphs':expected_names, 'error':expected_error}, 'actual':actual, 'query_ms':round((time.monotonic()-started)*1000,3), 'assertion':'W3C update result dataset, graph names, and global blank-node isomorphism'}
