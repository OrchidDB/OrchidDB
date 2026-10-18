#!/usr/bin/env python3
"""Create real, pinned-service search fixtures for local language-client tests.

Start tests/remote-engines.compose.yml first. Run with --output FILE, then set
ORCHIDDB_REMOTE_FIXTURE=FILE for each client's local test suite. --delete FILE
removes only the indexes recorded in that fixture.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import time
import urllib.request


def request(endpoint, method, path, data=None, ndjson=False):
    body = data.encode() if ndjson else (json.dumps(data).encode() if data is not None else None)
    req = urllib.request.Request(endpoint + path, data=body, method=method,
                                 headers={'Content-Type': 'application/x-ndjson' if ndjson else 'application/json'})
    with urllib.request.urlopen(req, timeout=60) as response:
        raw = response.read()
        return json.loads(raw) if raw else None


def create(output):
    cases, indexes = [], []
    for adapter, endpoint in [('quickwit', os.environ.get('QUICKWIT_URL', 'http://127.0.0.1:17280')),
                              ('elasticsearch', os.environ.get('ELASTICSEARCH_URL', 'http://127.0.0.1:19200'))]:
        index = f'orchiddb_clients_{os.getpid()}_{time.time_ns()}'
        if adapter == 'quickwit':
            mappings = [{'name': 'id', 'type': 'i64', 'fast': True},
                        {'name': 'body', 'type': 'text', 'tokenizer': 'default', 'record': 'position', 'fieldnorms': True},
                        {'name': 'tenant', 'type': 'text', 'tokenizer': 'raw', 'fast': True},
                        {'name': 'title', 'type': 'text', 'tokenizer': 'raw'},
                        {'name': 'payload', 'type': 'json'}]
            request(endpoint, 'POST', '/api/v1/indexes', {'version': '0.8', 'index_id': index,
                    'doc_mapping': {'mode': 'strict', 'field_mappings': mappings}})
        else:
            properties = {'id': {'type': 'long'}, 'body': {'type': 'text'}, 'tenant': {'type': 'keyword'},
                          'title': {'type': 'keyword'}, 'payload': {'properties': {'tag': {'type': 'keyword'}}}}
            request(endpoint, 'PUT', '/' + index, {'settings': {'number_of_shards': 1, 'number_of_replicas': 0},
                    'mappings': {'properties': properties}})
        indexes.append({'adapter': adapter, 'endpoint': endpoint, 'index': index})
        docs = [{'id': 1, 'body': 'graph storage database', 'tenant': 'a', 'title': 'One', 'payload': {'tag': 'alpha'}},
                {'id': 2, 'body': 'graph graph graph', 'tenant': 'a', 'title': 'Two'},
                {'id': 3, 'body': 'graph graph graph graph', 'tenant': 'b', 'title': 'Other tenant'},
                {'id': 4, 'body': 'database storage', 'tenant': 'a', 'title': 'Four'},
                {'id': 5, 'body': 'graph', 'title': 'No tenant'}]
        data = ''.join((json.dumps({'index': {'_id': str(d['id'])}}) + '\n' if adapter == 'elasticsearch' else '')
                       + json.dumps(d) + '\n' for d in docs)
        path = f'/api/v1/{index}/ingest?commit=force' if adapter == 'quickwit' else f'/{index}/_bulk?refresh=true'
        result = request(endpoint, 'POST', path, data, True)
        assert not result.get('errors'), result
        columns = lambda pairs: [{'name': n, 'data_type': t} for n, t in pairs]
        base = {'version': 1, 'dialect': 'duckdb', 'language': 'cypher',
                'engines': {'local': {'dialect': 'duckdb'}, 'text': {'dialect': adapter}}, 'execution_engine': 'local',
                'tables': [{'name': 'queries', 'engine': 'local', 'columns': columns([('id', 'int64'), ('body', 'string'), ('tenant', 'string')])},
                           {'name': index, 'engine': 'text', 'columns': columns([('id', 'int64'), ('body', 'string'), ('tenant', 'string'), ('title', 'string'), ('payload', 'json')])}],
                'nodes': [{'label': 'Question', 'table': 'queries', 'id': 'id', 'properties': {'id': 'id', 'body': 'body', 'tenant': 'tenant'}},
                          {'label': 'Document', 'table': index, 'id': 'id', 'properties': {'id': 'id', 'body': 'body', 'tenant': 'tenant', 'title': 'title', 'payload': 'payload'}}],
                'source_metadata': [{'table': index, 'format': adapter, 'options': {'index': index}, 'indexes': [{'column': 'body', 'metric': 'bm25'}]}],
                'computed_relationships': [{'name': 'SIMILAR_TO', 'source': 'Question', 'target': 'Document',
                    'predicate': 'source.tenant = target.tenant', 'properties': {'score': 'text.bm25(source.body,target.body)'},
                    'order_by': [{'expression': 'score', 'direction': 'desc'}], 'limit_per_source': 2}]}
        setup = ['CREATE TABLE queries(id BIGINT, body VARCHAR, tenant VARCHAR)',
                 "INSERT INTO queries VALUES (10,'graph','a'),(20,'database','a'),(30,NULL,'a'),(40,'graph',NULL)"]

        def add(name, query, expected, external=False, extra_setup=None):
            compiled = copy.deepcopy(base)
            compiled['query'] = query
            sql = setup.copy()
            if external:
                compiled['tables'][1]['engine'] = 'local'
                compiled['source_metadata'][0]['options']['engine'] = 'text'
                sql += [f'CREATE TABLE "{index}"(id BIGINT,body VARCHAR,tenant VARCHAR,title VARCHAR,payload JSON)',
                        f'''INSERT INTO "{index}" VALUES (1,'graph storage database','a','Authoritative one',NULL),(2,'graph graph graph','a','Authoritative two',NULL),(3,'graph graph graph graph','b','Authoritative other',NULL),(4,'database storage','a','Authoritative four',NULL),(5,'graph',NULL,'Authoritative null',NULL)''']
            sql += extra_setup or []
            cases.append({'name': adapter + '-' + name, 'adapter': adapter, 'endpoint': endpoint,
                          'request': compiled, 'setup_sql': sql, 'expected_rows': expected})

        add('paged-scan', 'MATCH (d:Document) RETURN d.id,d.title ORDER BY d.id', [[d['id'], d['title']] for d in docs])
        query = 'MATCH (q:Question)-[m:SIMILAR_TO]->(d:Document) RETURN q.id,d.id,d.title,m.score > 0 AS scored ORDER BY q.id,d.id'
        add('correlated-bm25', query, [[10,1,'One',True],[10,2,'Two',True],[20,1,'One',True],[20,4,'Four',True]])
        add('authoritative-sql', query, [[10,1,'Authoritative one',True],[10,2,'Authoritative two',True],[20,1,'Authoritative one',True],[20,4,'Authoritative four',True]], True)
        add('null-query', 'MATCH (q:Question {id:30})-[:SIMILAR_TO]->(d:Document) RETURN d.id', [])
        add('empty-source', query, [], extra_setup=['DELETE FROM queries'])
        add('json-payload', "MATCH (d:Document {id:1}) RETURN json.value(d.payload, '$.tag') AS tag", [['alpha']])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps({'indexes': indexes, 'cases': cases}, indent=2) + '\n')
    print(f'Created {len(cases)} live client cases in {output}')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path)
    p.add_argument('--delete', type=Path)
    args = p.parse_args()
    if args.delete:
        for index in json.loads(args.delete.read_text())['indexes']:
            path = '/api/v1/indexes/' if index['adapter'] == 'quickwit' else '/'
            request(index['endpoint'], 'DELETE', path + index['index'])
    elif args.output:
        create(args.output)
    else:
        p.error('provide --output or --delete')
