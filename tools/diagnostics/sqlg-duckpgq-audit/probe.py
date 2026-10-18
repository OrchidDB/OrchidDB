import duckdb, json
from pathlib import Path
root=Path(__file__).resolve().parents[3]
results=[]
def case(name, statements):
 c=duckdb.connect(config={'allow_unsigned_extensions':'true'}); c.execute(f"LOAD '{root}/extension/build/orchid.duckdb_extension'")
 result={'name':name,'statements':statements,'results':[]}
 for s in statements:
  try: result['results'].append({'rows':c.execute(s).fetchall()})
  except Exception as e: result['results'].append({'error':str(e)}); break
 results.append(result); c.close()
base=['CREATE TABLE p(id BIGINT PRIMARY KEY, name VARCHAR)', "INSERT INTO p VALUES (1,'a'),(2,'b'),(3,'c')", 'CREATE TABLE e(id BIGINT PRIMARY KEY, src BIGINT, dst BIGINT)', 'INSERT INTO e VALUES(1,1,2),(2,2,3)', 'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) LABEL P) EDGE TABLES(e KEY(id) SOURCE KEY(src) REFERENCES p(id) DESTINATION KEY(dst) REFERENCES p(id) LABEL R)']
case('graph_table',base+['SELECT * FROM GRAPH_TABLE(g MATCH (n:P) COLUMNS(n.name))'])
case('cypher_shortest',base+["CYPHER g MATCH p=shortestPath((a:P {id:1})-[:R*]->(b:P {id:3})) RETURN length(p)"])
case('fail_unreachable',base+["GREMLIN g g.V().has('id',999).fail('should not run')"])
case('inferred_keys',['CREATE TABLE p(id BIGINT PRIMARY KEY)', 'CREATE PROPERTY GRAPH g VERTEX TABLES(p)'])
case('same_label',['CREATE TABLE a(id BIGINT)', 'CREATE TABLE b(id BIGINT)', 'CREATE PROPERTY GRAPH g VERTEX TABLES(a KEY(id) LABEL P, b KEY(id) LABEL P)'])
case('exclude_properties',base[:4]+['CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) PROPERTIES ALL COLUMNS EXCEPT(name))'])
case('inheritance',['CREATE TABLE p(id BIGINT, typemask BIGINT)', 'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) LABEL P IN typemask(A,B))'])
for typ in ['UUID','TIMESTAMPTZ','MAP(VARCHAR,INTEGER)']:
 case('type_'+typ,[f'CREATE TABLE p(id BIGINT, v {typ})', 'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id))'])
case('null_key',['CREATE TABLE p(id BIGINT, name VARCHAR)', "INSERT INTO p VALUES(NULL,'a')", 'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) LABEL P)', 'CYPHER g MATCH(n:P) RETURN count(n), count(*), n IS NULL'])
case('dynamic_default',['CREATE SEQUENCE seq', "CREATE TABLE p(id BIGINT PRIMARY KEY DEFAULT nextval('seq'), name VARCHAR)",'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) LABEL P)', "CYPHER g CREATE(n:P {name:'a'}) RETURN n.id"])
case('nonkey_default',["CREATE TABLE p(id BIGINT PRIMARY KEY, name VARCHAR DEFAULT upper('alice'))", 'CREATE PROPERTY GRAPH g VERTEX TABLES(p KEY(id) LABEL P)', "CYPHER g CREATE(n:P {id:1}) RETURN n.name"])
print(json.dumps(results,indent=2,default=str))
