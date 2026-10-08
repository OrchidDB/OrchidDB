import assert from 'node:assert/strict';
import {Catalog, CatalogAuth, Credential, Connection, asyncDuckDBEngine} from '../../../clients/js/dist/index.js';
import {openAsyncDatabase} from '../../../clients/js/test/async-duckdb.mjs';
const e=process.env;
const auths=[CatalogAuth.bearer(e.AUTH_SMOKE_TOKEN),CatalogAuth.bearer(Credential.env('AUTH_SMOKE_TOKEN')),
  CatalogAuth.bearer(Credential.file(e.AUTH_SMOKE_TOKEN_FILE)),CatalogAuth.clientCredentials('admin',e.AUTH_SMOKE_SECRET),
  CatalogAuth.clientCredentials('admin',Credential.env('AUTH_SMOKE_SECRET')),
  CatalogAuth.clientCredentials('admin',Credential.file(e.AUTH_SMOKE_SECRET_FILE)),
  CatalogAuth.clientCredentials('external-client',e.AUTH_SMOKE_IDP_SECRET,{issuer:e.AUTH_SMOKE_ISSUER}),
  CatalogAuth.tokenExchange(e.AUTH_SMOKE_TOKEN)];
const database=await openAsyncDatabase();
const session=await database.connect();
try {
  await session.query("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')");
  for(const auth of auths) {
    const catalog=new Catalog(e.AUTH_SMOKE_URL,{scope:'smoke',graph:'smoke',auth});
    assert.equal(catalog.discover().description,'Auth smoke graph');
    const graph=new Connection(asyncDuckDBEngine(session),catalog);
    const rows=await graph.query('MATCH (p:Person)-[:PEER {score:1, limit:1}]->(q:Person) RETURN q.name AS name ORDER BY name');
    const names=[];
    for await (const batch of rows) for (const row of batch.toArray()) names.push(row.name);
    assert.deepEqual(names,['Ada','Grace']);
    rows.close();graph.close();
  }
  const catalog=new Catalog(e.AUTH_SMOKE_URL,{scope:'smoke',graph:'smoke',auth:auths[3]});
  assert.equal(catalog.registerPrincipal('javascript-workload',{roles:['reader']}).version,1);
  assert.equal(catalog.principal('javascript-workload').version,1);
  console.log('PASS JS/TS: credential sources, OAuth/OIDC, exchange, principal management, DuckDB execution');
} finally { await session.close(); await database.terminate(); }
