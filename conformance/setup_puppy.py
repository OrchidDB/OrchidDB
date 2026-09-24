#!/usr/bin/env python3
"""Create the disposable PostgreSQL fixture and map it into PuppyGraph 1.11.1."""
import base64,json,os,urllib.request
from pathlib import Path
import psycopg
with psycopg.connect(os.environ.get('CONFORMANCE_PG','postgresql://conformance:conformance-local-only@127.0.0.1:15433/conformance'),autocommit=True) as c:
 c.execute('CREATE SCHEMA IF NOT EXISTS comparison')
 c.execute('DROP TABLE IF EXISTS comparison.person,comparison.knows')
 c.execute('CREATE TABLE comparison.person(uid BIGINT PRIMARY KEY,name TEXT,age BIGINT,score BIGINT)')
 c.execute("INSERT INTO comparison.person VALUES (1,'Alice',30,NULL),(2,'Bob',20,NULL),(3,'Carol',40,NULL),(4,'Dave',NULL,NULL)")
 c.execute('CREATE TABLE comparison.knows(eid BIGINT PRIMARY KEY,src BIGINT,dst BIGINT,weight BIGINT)')
 c.execute('INSERT INTO comparison.knows VALUES (1,1,2,1),(2,2,3,2),(3,1,3,3),(4,3,1,4)')
def source(table,fields):
 return {'externalDataSource':{'enabled':True,'catalog':'comparison_pg','schema':'comparison','table':table,'mappedField':[{'sourceFieldName':f,'targetFieldName':f} for f in fields]}}
def field(name,kind='Long'):return {'name':name,'type':kind}
schema={'catalog':[{'name':'comparison_pg','type':'postgresql','jdbc':{'jdbcUri':os.environ.get('PUPPY_JDBC','jdbc:postgresql://host.docker.internal:15433/conformance'),'driverClass':'org.postgresql.Driver','username':'conformance','password':'conformance-local-only'}}],
 'node':[{'label':'Person','dataSourceGroup':source('person',['uid','name','age','score']),'id':[field('uid')],'attribute':[field('uid'),field('name','String'),field('age'),field('score')]}],
 'edge':[{'label':'KNOWS','dataSourceGroup':source('knows',['eid','src','dst','weight']),'fromNodeLabel':'Person','toNodeLabel':'Person','id':[field('eid')],'fromKey':[field('src')],'toKey':[field('dst')],'attribute':[field('weight')]}]}
request=urllib.request.Request(os.environ.get('PUPPY_HTTP','http://127.0.0.1:18081')+'/ui-api/uploadSchema',data=json.dumps(schema).encode(),headers={'Content-Type':'application/json','Authorization':'Basic '+base64.b64encode(b'puppygraph:conformance-local-only').decode()})
try:
 with urllib.request.urlopen(request,timeout=90) as r: print(r.read().decode())
except urllib.error.HTTPError as e:print(e.read().decode());raise
