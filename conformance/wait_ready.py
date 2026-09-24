#!/usr/bin/env python3
import time,urllib.request
import psycopg
from neo4j import GraphDatabase

def wait(label,check):
 deadline=time.monotonic()+180
 while True:
  try:check();print(label,'ready',flush=True);return
  except Exception:
   if time.monotonic()>deadline:raise
   time.sleep(2)
def postgres():
 with psycopg.connect('postgresql://conformance:conformance-local-only@127.0.0.1:15433/conformance',connect_timeout=3) as c:c.execute('SELECT 1')
def neo():
 with GraphDatabase.driver('bolt://127.0.0.1:17687',auth=('neo4j','conformance-local-only'),connection_timeout=3) as d:d.verify_connectivity()
wait('PostgreSQL',postgres);wait('Neo4j',neo)
wait('PuppyGraph HTTP',lambda:urllib.request.urlopen('http://127.0.0.1:18081',timeout=3).close())
