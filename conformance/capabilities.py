"""Reviewed product capabilities. Every populated cell links to its evidence."""
import json
from pathlib import Path
D='https://docs.crabgraph.net/'
L='https://docs.ladybugdb.com/'
N='https://neo4j.com/docs/'
P='https://docs.puppygraph.com/'
S='https://sqlg.org/docs/3.1.6/'
SOURCES={
 'crab':D+'concepts.html','mapping':D+'mapping-reference.html','rdf':D+'rdf.html','transactions':D+'transactions.html','execution':D+'execution.html','parameters':D+'parameters.html','rust':D+'rust-api.html',
 'lady':L,'ldiff':L+'cypher/difference/','lext':L+'extensions/','ltrans':L+'cypher/transaction/','lvector':L+'extensions/vector/','lattach':L+'extensions/attach/rdbms/','lpython':L+'client-apis/python/',
 'neo':N+'operations-manual/current/introduction/','nindex':N+'cypher-manual/current/indexes/search-performance-indexes/','nconstraint':N+'cypher-manual/current/constraints/managing-constraints/','ngql':N+'cypher-manual/current/appendix/gql-conformance/',
 'puppy':P+'reference/cypher-query-language/','pgremlin':P+'reference/gremlin-query-language/','pprice':'https://www.puppygraph.com/pricing','sqlg':S,
}
ROWS=[]
def cell(text,source,kind='documented'):return {'text':text,'source':SOURCES[source],'kind':kind,'reviewed':'2026-09-23'}
def add(group,title,**cells):
 cells={k:v for k,v in cells.items() if k in {'crabgraph','sqlg','puppygraph'}}
 if cells:ROWS.append({'category':group,'title':title,'cells':{k:cell(*v) for k,v in cells.items()}})
add('Languages and graph model','Cypher',crabgraph=('Native frontend','crab'),ladybug=('Native dialect','ldiff'),neo4j=('Cypher 5 / 25','neo'),puppygraph=('openCypher 9','puppy'))
add('Languages and graph model','Gremlin',crabgraph=('Native frontend','crab'),puppygraph=('Read traversals','pgremlin'),sqlg=('TinkerPop 3.7.4','sqlg'))
add('Languages and graph model','SPARQL and RDF datasets',crabgraph=('Native frontend / datasets','rdf'))
add('Languages and graph model','ISO GQL feature accounting',neo4j=('Published feature list','ngql'))
add('Languages and graph model','Property graph storage',crabgraph=('Managed or mapped','crab'),ladybug=('Typed node / rel tables','lady'),neo4j=('Native store','neo'),puppygraph=('External tables','pprice'),sqlg=('SQL-backed graph','sqlg'))
add('Languages and graph model','Multiple labels per vertex',neo4j=('Supported','neo'),ladybug=('One node table','ldiff'))
add('Languages and graph model','Schema definition',crabgraph=('Graph mapping','mapping'),ladybug=('Required DDL','ldiff'),sqlg=('Topology schema','sqlg'))
add('Languages and graph model','Default variable path semantics',ladybug=('Walk; default max 30','ldiff'),puppygraph=('Relationship uniqueness','puppy'))
add('Languages and graph model','GraphComputer / OLAP API',sqlg=('Not supported','sqlg','unavailable'))
add('Languages and graph model','Graph variables',sqlg=('Not supported','sqlg','unavailable'))
add('Languages and graph model','Vertex multi-properties',sqlg=('Not supported','sqlg','unavailable'))
add('Languages and graph model','Vertex meta-properties',sqlg=('Not supported','sqlg','unavailable'))
add('Languages and graph model','Threaded transactions',sqlg=('Not supported','sqlg','unavailable'))
add('Data access and integration','Graph over existing tables',crabgraph=('Declarative mapping','mapping'),ladybug=('External RDBMS integration','lattach'),puppygraph=('External source schema','pprice'))
add('Data access and integration','SQL views and query sources',crabgraph=('View / SQL mapping','mapping'),ladybug=('SQL_QUERY via extension','lattach'))
add('Data access and integration','Composite graph identifiers',crabgraph=('Composite keys','mapping'),sqlg=('User-defined identifiers','sqlg'))
add('Data access and integration','Mapped relationship endpoints',crabgraph=('Source / destination key maps','mapping'),puppygraph=('External schema','puppy'))
add('Data access and integration','Ontology mapping',crabgraph=('Classes and predicates','rdf'))
add('Data access and integration','Named RDF graphs',crabgraph=('Dataset API','rdf'))
add('Data access and integration','RDF term identity',crabgraph=('IRI / blank / literal terms','rdf'))
add('Data access and integration','Apache Arrow result batches',crabgraph=('Native result format','rust'),ladybug=('Python Arrow export','lpython'))
for title in ['PostgreSQL','DuckDB']:
 add('Data access and integration',title,crabgraph=('SQL backend','execution'),ladybug=('Extension','lext'),puppygraph=('Developer edition','pprice'),**({'sqlg':('SQL backend','sqlg')} if title=='PostgreSQL' else {}))
for title in ['Apache Iceberg','Delta Lake']:
 add('Data access and integration',title,ladybug=('Extension','lext'),puppygraph=('Developer edition','pprice'))
for title in ['Apache Hudi','Elasticsearch','BigQuery','Redshift','Snowflake']:
 add('Data access and integration',title,puppygraph=('Developer edition','pprice'))
for title in ['SQLite','ADBC sources','Azure storage','JSON','Unity Catalog','HTTP file access']:
 add('Data access and integration',title,ladybug=('Extension','lext'))
for title in ['H2','HSQLDB','MySQL','MariaDB']:
 add('Data access and integration',title,sqlg=('SQL backend','sqlg'))
add('Data access and integration','More than two simultaneous sources',puppygraph=('Enterprise','pprice','enterprise'))
add('Indexing and analytics','Vector similarity index',ladybug=('HNSW extension','lvector'),neo4j=('Vector index','neo'),sqlg=('PostgreSQL pgvector','sqlg'))
add('Indexing and analytics','Full-text search',ladybug=('BM25 extension','lext'),neo4j=('Full-text indexes','neo'),sqlg=('PostgreSQL text search','sqlg'))
add('Indexing and analytics','Graph algorithm library',ladybug=('algo extension','lext'),neo4j=('Separate GDS Community plugin','neo','extension'))
add('Indexing and analytics','LLM embeddings',ladybug=('llm extension; provider API','lext'))
add('Indexing and analytics','Range / text / point indexes',neo4j=('Search-performance indexes','nindex'))
add('Indexing and analytics','Property uniqueness constraints',neo4j=('Community','nconstraint'))
for title in ['Property existence constraints','Property type constraints','Node and relationship key constraints']:
 add('Indexing and analytics',title,neo4j=('Enterprise','nconstraint','enterprise'))
add('Indexing and analytics','Primary key constraint',ladybug=('Node table primary key','ldiff'))
add('Execution and transactions','Explicit transaction API',crabgraph=('Begin / commit / rollback','transactions'),ladybug=('Read / write transactions','ltrans'),neo4j=('ACID transactions','neo'),sqlg=('Database transactions','sqlg'))
add('Execution and transactions','Concurrent writers',ladybug=('One active writer','ltrans'))
add('Execution and transactions','SQL pushdown',crabgraph=('Hybrid / SQL-only modes','execution'),sqlg=('Optimized traversal steps','sqlg'))
add('Execution and transactions','Explain and execution plans',crabgraph=('Backend and plan diagnostics','execution'))
add('Execution and transactions','Parameterized queries',crabgraph=('Typed parameter binding','parameters'))
add('Execution and transactions','Gremlin graph mutation',puppygraph=('Read-only Gremlin interface','pgremlin','unavailable'),sqlg=('TinkerPop mutation API','sqlg'))
add('Execution and transactions','Batch / streaming insertion',sqlg=('Batch modes','sqlg'))
add('Execution and transactions','Partitioning',sqlg=('PostgreSQL partition support','sqlg'))
add('Deployment and security','Embedded use',crabgraph=('Rust library','rust'),ladybug=('Embedded engine','lady'),sqlg=('JVM library','sqlg'))
add('Deployment and security','Docker single-node deployment',neo4j=('Community','neo'),puppygraph=('Developer','pprice'))
add('Deployment and security','High-availability cluster',neo4j=('Enterprise','neo','enterprise'),puppygraph=('Enterprise','pprice','enterprise'))
add('Deployment and security','Online backups',neo4j=('Enterprise','neo','enterprise'))
add('Deployment and security','Role-based access control',neo4j=('Enterprise','neo','enterprise'))
add('Deployment and security','LDAP / Active Directory',neo4j=('Enterprise','neo','enterprise'))
add('Deployment and security','Single sign-on',puppygraph=('Enterprise','pprice','enterprise'))
add('Deployment and security','Basic username / password',puppygraph=('Developer','pprice'))
add('Deployment and security','Advanced graph explorer',puppygraph=('Enterprise','pprice','enterprise'))
add('Deployment and security','Basic graph explorer',puppygraph=('Developer','pprice'))
add('Deployment and security','Monitoring sensors / Datadog',puppygraph=('Enterprise','pprice','enterprise'))
add('Deployment and security','AWS AMI distribution',puppygraph=('Enterprise','pprice','enterprise'))
add('Deployment and security','Horizontal sharding',neo4j=('Infinigraph subscription','neo','enterprise'))
if __name__=='__main__':
 Path(__file__).with_name('data').joinpath('capabilities.json').write_text(json.dumps(ROWS,indent=2)+'\n')
 print(len(ROWS),'capability rows')
