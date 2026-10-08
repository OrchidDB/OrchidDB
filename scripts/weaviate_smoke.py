from contextlib import contextmanager
import argparse
import copy
import json
import math
import os
from pathlib import Path
import secrets
import subprocess
import sys
import time
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'clients/python/src'))
os.environ.setdefault('ORCHIDDB_NATIVE_LIBRARY', str(ROOT / ('target/debug/liborchiddb_compiler.dylib' if sys.platform == 'darwin' else 'target/debug/liborchiddb_compiler.so')))
import duckdb
from orchiddb import Connection, DuckDBEngine, QueryError, RemoteEngine


class ObservedRemote(RemoteEngine):
    calls = 0
    @contextmanager
    def execute_requests(self, requests, columns):
        self.calls += len(requests)
        with super().execute_requests(requests, columns) as result:
            yield result


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--no-build',action='store_true')
    args=parser.parse_args()
    if not args.no_build:
        from catalog_smoke import build_native_clients
        build_native_clients()
    container = 'orchid-weaviate-smoke-' + str(os.getpid())
    token = secrets.token_urlsafe(32)
    environment = os.environ | dict(AUTHENTICATION_APIKEY_ALLOWED_KEYS=token)
    command = ['docker','run','-d','--name',container,'-p','127.0.0.1::8080',
               '-e','AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=false','-e','AUTHENTICATION_APIKEY_ENABLED=true',
               '-e','AUTHENTICATION_APIKEY_ALLOWED_KEYS','-e','AUTHENTICATION_APIKEY_USERS=orchid-smoke',
               '-e','AUTHORIZATION_ADMINLIST_ENABLED=true','-e','AUTHORIZATION_ADMINLIST_USERS=orchid-smoke',
               '-e','DEFAULT_VECTORIZER_MODULE=none','-e','DISABLE_TELEMETRY=true','-e','CLUSTER_HOSTNAME=orchid-smoke',
               'cr.weaviate.io/semitechnologies/weaviate:1.34.0']
    subprocess.run(command, env=environment, check=True, stdout=subprocess.DEVNULL)
    try:
        for _ in range(30):
            published = subprocess.run(['docker','port',container,'8080/tcp'],text=True,capture_output=True)
            if published.returncode == 0 and published.stdout.strip(): break
            state = subprocess.check_output(['docker','inspect','-f','{{.State.Status}}',container],text=True).strip()
            if state == 'exited':
                logs = subprocess.run(['docker','logs',container],text=True,capture_output=True)
                details = subprocess.check_output(['docker','inspect','-f','{{json .State}}',container],text=True)
                raise RuntimeError((details+logs.stdout+logs.stderr).replace(token,'[redacted]'))
            time.sleep(0.2)
        else: raise RuntimeError('Weaviate container did not publish its port')
        port = published.stdout.strip().rsplit(':',1)[1]
        endpoint = 'http://127.0.0.1:' + port
        def request(path, value=None, method=None):
            data = None if value is None else json.dumps(value).encode()
            with urlopen(Request(endpoint+path,data=data,method=method,headers={'Content-Type':'application/json','Authorization':'Bearer '+token}),timeout=10) as response:
                raw=response.read()
                return json.loads(raw) if raw else None
        for attempt in range(120):
            try:
                request('/v1/.well-known/ready')
                break
            except Exception:
                if attempt == 119: raise
                time.sleep(0.5)
        db = duckdb.connect()
        db.execute("CREATE TABLE questions(id BIGINT, tenant_id VARCHAR, embedding DOUBLE[]); INSERT INTO questions VALUES (10,'a',[1,0]),(20,'a',[0,1]); CREATE TABLE documents(id BIGINT, tenant_id VARCHAR, embedding DOUBLE[], title VARCHAR); INSERT INTO documents VALUES (1,'a',[1,0],'Graph storage'),(2,'a',[0.8,0.6],'Graph retrieval'),(3,'b',[1,0],'Other tenant'); CREATE TABLE authors(id BIGINT,name VARCHAR); INSERT INTO authors VALUES (100,'Ada'),(200,'Grace'); CREATE TABLE written_by(id BIGINT,document_id BIGINT,author_id BIGINT); INSERT INTO written_by VALUES(1,1,100),(2,2,200)")
        table = lambda name, columns:dict(name=name,engine='local',columns=[dict(name=n,data_type=t) for n,t in columns])
        schema = dict(engines={'local':{'dialect':'duckdb'},'vectors':{'dialect':'weaviate'}},execution_engine='local',
            tables=[table('questions',[('id','int64'),('tenant_id','string'),('embedding','list:float64')]),table('documents',[('id','int64'),('tenant_id','string'),('embedding','list:float64'),('title','string')]),table('authors',[('id','int64'),('name','string')]),table('written_by',[('id','int64'),('document_id','int64'),('author_id','int64')])],
            nodes=[dict(label='Question',table='questions',id='id',properties={'id':'id','tenant_id':'tenant_id','embedding':'embedding'}),dict(label='Document',table='documents',id='id',properties={'id':'id','tenant_id':'tenant_id','embedding':'embedding','title':'title'}),dict(label='Author',table='authors',id='id',properties={'name':'name'})],
            edges=[dict(label='WRITTEN_BY',table='written_by',id='id',source='document_id',target='author_id',source_label='Document',target_label='Author')])
        local = DuckDBEngine(db)
        with ObservedRemote('weaviate',endpoint,authentication={'type':'api_key','token':token}) as remote:
            for metric,function,distance,named in [('cosine','vector.cosine_similarity','cosine',False),('dot','vector.dot','dot',False),('l2','vector.l2_distance','l2-squared',False),('cosine','vector.cosine_similarity','cosine',True)]:
                collection = 'Orchid'+metric.title()+('Named' if named else '')
                definition=dict(**{'class':collection},vectorizer='none',vectorIndexType='hnsw',vectorIndexConfig={'distance':distance},invertedIndexConfig={'indexNullState':True},properties=[dict(name='doc_id',dataType=['int']),dict(name='tenant_id',dataType=['text'],tokenization='field')])
                if named:
                    definition.pop('vectorizer')
                    definition.pop('vectorIndexType')
                    definition.pop('vectorIndexConfig')
                    definition['vectorConfig']={'content':{'vectorizer':{'none':{}},'vectorIndexType':'hnsw','vectorIndexConfig':{'distance':distance}}}
                request('/v1/schema',definition)
                for id,tenant,vector in [(1,'a',[1,0]),(2,'a',[0.8,0.6]),(3,'b',[1,0])]:
                    request('/v1/objects',{'class':collection,'properties':{'doc_id':id,'tenant_id':tenant},**({'vectors':{'content':vector}} if named else {'vector':vector})})
                config=copy.deepcopy(schema)
                config['source_metadata']=[dict(table='documents',format='weaviate',options={'engine':'vectors','collection':collection,'key_field':'doc_id','retrieval':'approximate_allowed'},indexes=[dict(column='embedding',metric=metric)])]
                if named: config['source_metadata'][0]['options']['target_vector']='content'
                direction='ASC' if metric=='l2' else 'DESC'
                comparison='<=' if metric=='l2' else '>='
                config['cypher_relationships']=[dict(name='RELEVANT_TO',source='Question',target='Document',parameters=[dict(name='score',schema={'type':'number'},default=1 if metric=='l2' else 0.7),dict(name='limit',schema={'type':'integer','minimum':1},default=5)],cypher=f'WITH source MATCH (target:Document) WHERE target.tenant_id = source.tenant_id WITH target, {function}(source.embedding,target.embedding) AS score WHERE score {comparison} $score RETURN target, score ORDER BY score {direction} LIMIT $limit',returns={'target':'target','properties':{'score':{'type':'number'}}})]
                with Connection(local,schema=config,engines={'local':local,'vectors':remote}) as graph:
                    def rows(query,parameters=None):
                        with graph.query(query,parameters=parameters) as result:return result.read_all().to_pylist()
                    query='MATCH (q:Question)-[r:RELEVANT_TO {score:$score,limit:$limit}]->(d:Document)-[:WRITTEN_BY]->(a:Author) WHERE q.id=10 RETURN d.id AS id, d.title AS title, a.name AS author, r.score AS score ORDER BY id'
                    before = remote.calls
                    result=rows(query,{'score':1 if metric=='l2' else .7,'limit':2})
                    assert remote.calls > before, 'Weaviate retrieval was not executed'
                    assert [row['id'] for row in result]==[1,2],result
                    expected=[0,math.sqrt(.4)] if metric=='l2' else [1,.8]
                    assert all(math.isclose(row['score'],value,abs_tol=1e-5) for row,value in zip(result,expected)),result
                    assert [row['author'] for row in result]==['Ada','Grace'],result
                    limited=rows(query,{'score':2 if metric=='l2' else 0,'limit':1})
                    assert [row['id'] for row in limited]==[1],limited
                    per_source=rows('MATCH (q:Question)-[r:RELEVANT_TO {score:$score,limit:1}]->(d:Document) RETURN q.id AS question,d.id AS document ORDER BY question',{'score':2 if metric=='l2' else 0})
                    assert per_source==[{'question':10,'document':1},{'question':20,'document':2}],per_source
                    rejected=rows(query,{'score':-.1 if metric=='l2' else 1.1,'limit':2})
                    assert rejected==[],rejected
                    db.execute("UPDATE documents SET tenant_id='b' WHERE id=2")
                    stale=rows(query,{'score':1 if metric=='l2' else .7,'limit':2})
                    assert [row['id'] for row in stale]==[1],stale
                    db.execute("UPDATE documents SET tenant_id='a' WHERE id=2")
                    print('PASS Weaviate '+metric+(' named vector' if named else '')+': parameterized derived edge, score conversion, tenant filtering, per-source limit, SQL graph joins',flush=True)
                exact=copy.deepcopy(config)
                exact['source_metadata'][0]['options']['retrieval']='exact'
                try:
                    with Connection(local,schema=exact,engines={'local':local,'vectors':remote}) as graph:
                        with graph.query('MATCH (q:Question)-[:RELEVANT_TO]->(d:Document) RETURN d.id') as result: result.read_all()
                except QueryError as error:
                    assert 'exact' in str(error),str(error)
                else:raise AssertionError('Exact retrieval silently used approximate engine')
            mismatch=copy.deepcopy(config)
            mismatch['source_metadata'][0]['options']['collection']='OrchidDot'
            mismatch['source_metadata'][0]['options'].pop('target_vector',None)
            try:
                with Connection(local,schema=mismatch,engines={'local':local,'vectors':remote}) as graph:
                    with graph.query('MATCH (q:Question)-[:RELEVANT_TO]->(d:Document) RETURN d.id') as result: result.read_all()
            except QueryError as error:
                assert 'distance does not match' in str(error),str(error)
            else:raise AssertionError('Mismatched index distance was accepted')
            with RemoteEngine('weaviate',endpoint,authentication={'type':'api_key','token':'wrong-credential'}) as denied:
                try:
                    with Connection(local,schema=config,engines={'local':local,'vectors':denied}) as graph:
                        with graph.query('MATCH (q:Question)-[:RELEVANT_TO]->(d:Document) RETURN d.id') as result: result.read_all()
                except QueryError as error:
                    assert 'weaviate HTTP 401' in str(error),str(error)
                    assert token not in str(error) and 'wrong-credential' not in str(error),str(error)
                else:raise AssertionError('Invalid credentials were accepted')
            print('PASS Weaviate: exact-retrieval rejection, metric validation, authentication rejection, authoritative tenant recheck',flush=True)
        db.close()
    finally:
        subprocess.run(['docker','rm','-f',container],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)


if __name__=='__main__':
    main()
