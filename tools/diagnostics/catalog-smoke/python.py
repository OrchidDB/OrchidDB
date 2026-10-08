import os
import time
import duckdb
from orchiddb import Catalog, CatalogAuth, Credential, Connection, DuckDBEngine, CypherEdge, QueryError

e = os.environ
url, secret = e['AUTH_SMOKE_URL'], e['AUTH_SMOKE_SECRET']
auths = [
    CatalogAuth.bearer(e['AUTH_SMOKE_TOKEN']),
    CatalogAuth.bearer(Credential.env('AUTH_SMOKE_TOKEN')),
    CatalogAuth.bearer(Credential.file(e['AUTH_SMOKE_TOKEN_FILE'])),
    CatalogAuth.client_credentials('admin', secret),
    CatalogAuth.client_credentials('admin', Credential.env('AUTH_SMOKE_SECRET')),
    CatalogAuth.client_credentials('admin', Credential.file(e['AUTH_SMOKE_SECRET_FILE'])),
    CatalogAuth.client_credentials('external-client', e['AUTH_SMOKE_IDP_SECRET'], issuer=e['AUTH_SMOKE_ISSUER']),
    CatalogAuth.client_credentials('external-client', e['AUTH_SMOKE_IDP_SECRET'], token_endpoint=e['AUTH_SMOKE_ISSUER']+'/token'),
    CatalogAuth.token_exchange(e['AUTH_SMOKE_TOKEN']),
]
db = duckdb.connect()
db.execute("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')")
for auth in auths:
    catalog = Catalog(url, scope='smoke', graph='smoke', auth=auth)
    assert catalog.discover()['description'] == 'Auth smoke graph'
    connection = Connection(DuckDBEngine(db), catalog=catalog)
    with connection.query('MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name') as result:
        assert result.read_all().column('name').to_pylist() == ['Ada', 'Grace']
    connection.close()
admin = Catalog(url, scope='smoke', graph='smoke', auth=auths[3])
issued = admin.register_principal('python-workload', roles=['reader'])
assert admin.principal('python-workload')['version'] == 1
worker = Catalog(url, scope='smoke', graph='smoke', auth=CatalogAuth.client_credentials('python-workload', issued['client_secret']))
assert worker.discover()['revision'] == 1
admin.register_edge('python_edge', CypherEdge('OTHER', 'person', 'person', 'WITH source MATCH (target:Person) WHERE target.id <> source.id RETURN target', 'Other people'))
assert admin.object('python_edge')['version'] == 1
admin.register_graph(['person', 'peer', 'python_edge'], description='Auth smoke graph', expected_version=1)
admin.publish(expected_revision=1, graph_version=2, object_versions={'person':1, 'people':1, 'peer':1, 'python_edge':1})
with Connection(DuckDBEngine(db), catalog=admin).query('MATCH (p:Person)-[:OTHER]->(q:Person) RETURN q.name AS name ORDER BY name') as result:
    assert result.read_all().column('name').to_pylist() == ['Ada', 'Grace']
db.execute("INSERT INTO people VALUES (3,'Linus')")
nearest = CypherEdge('NEAREST', 'person', 'person',
    'WITH source MATCH (target:Person) WHERE target.id <> source.id RETURN target, target.id AS score ORDER BY score DESC LIMIT $k',
    'Highest ranked peers', properties={'score': {'type': 'integer'}},
    parameters=({'name': 'k', 'schema': {'type': 'integer', 'minimum': 1}, 'default': 1},))
admin.register_edge('nearest', nearest)
admin.register_graph(['person', 'peer', 'python_edge', 'nearest'], description='Auth smoke graph', expected_version=2)
versions = {'person':1, 'people':1, 'peer':1, 'python_edge':1, 'nearest':1}
admin.publish(expected_revision=2, graph_version=3, object_versions=versions)
graph = Connection(DuckDBEngine(db), catalog=admin)
pinned = Connection(DuckDBEngine(db), catalog=admin.at_revision(3))

def rows(query, parameters=None, connection=graph):
    with connection.query(query, parameters=parameters) as result:
        return result.read_all().to_pylist()

forward = 'MATCH (p:Person)-[r:NEAREST]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source, target'
expected = [{'source': 1, 'target': 3, 'score': 3}, {'source': 2, 'target': 3, 'score': 3}, {'source': 3, 'target': 2, 'score': 2}]
assert rows(forward) == expected
assert rows('MATCH (q:Person)<-[r:NEAREST]-(p:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source, target') == expected
assert rows('MATCH (p:Person) CALL NEAREST(p) YIELD target, score RETURN p.id AS source, target.id AS target, score ORDER BY source, target') == expected
assert rows('MATCH (p:Person) CALL NEAREST(p, $k) YIELD target, score RETURN p.id AS source, target.id AS target, score ORDER BY source, target', {'k':2}) == [dict(source=a, target=b, score=b) for a in (1,2,3) for b in (1,2,3) if a!=b]
assert rows('MATCH (p:Person)-[r:NEAREST {score:3}]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source, target') == expected[:2]
assert rows('MATCH (p:Person) WHERE p.id=1 OPTIONAL MATCH (p)-[r:NEAREST {score:99}]->(q:Person) RETURN q.id AS target') == [{'target':None}]
for invalid in (0, 'two'):
    try:
        rows('MATCH (p:Person) CALL NEAREST(p, $k) YIELD target RETURN target.id', {'k':invalid})
    except QueryError as error:
        assert "parameter k" in str(error)
        pass
    else:
        raise AssertionError('Invalid derived edge parameter accepted')
nearest_ascending = CypherEdge('NEAREST', 'person', 'person', nearest.cypher.replace('DESC', 'ASC'), nearest.description,
    properties=nearest.properties, parameters=nearest.parameters)
admin.register_edge('nearest', nearest_ascending, expected_version=1)
assert rows(forward) == expected
versions['nearest'] = 2
admin.publish(expected_revision=3, graph_version=3, object_versions=versions)
assert rows(forward) == [{'source':1,'target':2,'score':2}, {'source':2,'target':1,'score':1}, {'source':3,'target':1,'score':1}]
assert rows(forward, connection=pinned) == expected
assert admin.at_revision(3).at_revision(4).discover()['revision'] == 4
rag = CypherEdge('RELEVANT_TO', 'person', 'person',
    'WITH source MATCH (target:Person) WHERE target.id <> source.id AND target.id >= $score RETURN target, target.id AS score, "peer" AS kind ORDER BY score DESC LIMIT $limit',
    'Ranked peer fixture', properties={'score': {'type':'integer'}, 'kind': {'type':'string'}},
    parameters=({'name':'score','schema':{'type':'integer'},'default':2}, {'name':'limit','schema':{'type':'integer','minimum':1},'default':1}))
admin.register_edge('relevant', rag)
admin.register_graph(['person', 'peer', 'python_edge', 'nearest', 'relevant'], description='Auth smoke graph', expected_version=3)
versions['relevant'] = 1
admin.publish(expected_revision=4, graph_version=4, object_versions=versions)
assert rows('MATCH (p:Person)-[r:RELEVANT_TO {score:$score, limit:$limit}]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source, target', {'score':2,'limit':2}) == [dict(source=1,target=2,score=2),dict(source=1,target=3,score=3),dict(source=2,target=3,score=3),dict(source=3,target=2,score=2)]
assert rows('MATCH (p:Person)-[r:RELEVANT_TO {score:2, limit:1, kind:"peer"}]->(q:Person) WHERE r.score > 2 RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source') == expected[:2]
assert rows('MATCH (p:Person)-[r:RELEVANT_TO {score:2}]->(q:Person) RETURN DISTINCT label(r) AS kind') == [{'kind':'RELEVANT_TO'}]
assert rows('MATCH (p:Person)-[r:RELEVANT_TO {score:2, kind:"missing"}]->(q:Person) RETURN q.id AS target') == []
assert rows('MATCH (p:Person) WHERE p.id=1 MATCH (p)-[a:RELEVANT_TO {score:2,limit:1}]->(x:Person) MATCH (p)-[b:RELEVANT_TO {score:1,limit:2}]->(y:Person) RETURN x.id AS x, y.id AS y ORDER BY y') == [{'x':3,'y':2},{'x':3,'y':3}]
assert rows('MATCH (q:Person)<-[r:RELEVANT_TO {score:3,limit:1}]-(p:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source') == expected[:2]
assert rows('MATCH (p:Person) WHERE p.id=3 OPTIONAL MATCH (p)-[r:RELEVANT_TO {score:3,limit:1}]->(q:Person) RETURN q.id AS target') == [{'target':None}]
assert rows('MATCH (p:Person)-[r:PEER {rank:2}]->(q:Person) RETURN p.id AS source ORDER BY source') == [{'source':1},{'source':3}]
assert rows('MATCH (p:Person) WHERE EXISTS { MATCH (p)-[:RELEVANT_TO {score:3,limit:1}]->(q:Person) } RETURN p.id AS source ORDER BY source') == [{'source':1},{'source':2}]
assert sorted(rows('MATCH (p:Person)-[:RELEVANT_TO {score:3,limit:1}]->(q:Person) RETURN q.id AS target UNION MATCH (p:Person)-[:RELEVANT_TO {score:2,limit:1}]->(q:Person) RETURN q.id AS target ORDER BY target'), key=lambda row:row['target']) == [{'target':2},{'target':3}]
for invalid in (0, 'two'):
    try:
        rows('MATCH (p:Person)-[:RELEVANT_TO {limit:$limit}]->(q:Person) RETURN q.id', {'limit':invalid})
    except QueryError as error:
        assert 'parameter limit' in str(error)
    else:
        raise AssertionError('Invalid relationship-map argument accepted')
print('PASS relationship-map arguments: score/limit, defaults, per-occurrence binding, property collisions/filters, canonical label, reverse/optional traversal, validation', flush=True)
graph.close()
pinned.close()
print('PASS derived edges: traversal, per-source limits, defaults/arguments, properties, reverse/optional traversal, invalid arguments, draft isolation, active/pinned revisions', flush=True)
renew = Catalog(url, scope='renew', graph='smoke', auth=auths[3])
renew.discover()
time.sleep(5)
renew.discover()
time.sleep(4)
renew.discover()
print('PASS Python: all credential sources, internal/external OAuth, exchange, renewal, principal/edge publication, DuckDB execution', flush=True)
