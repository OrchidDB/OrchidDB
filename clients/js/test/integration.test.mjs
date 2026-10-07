import test from 'node:test';
import assert from 'node:assert/strict';
import { Compiler, batches, authorization, permissionRelation, permissionScope } from '../dist/index.js';
import { openDatabase, arrowEngine, request } from '../examples/duckdb-wasm.mjs';

test('native compile -> caller DuckDB -> Arrow preserves values/nulls and transaction ownership', async () => {
  const db = await openDatabase(); const connection = db.connect();
  try {
    connection.query("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (9007199254740993, 'Orchid'), (2, NULL)");
    connection.query('BEGIN; INSERT INTO people VALUES (3, \'transaction\')');
    const compiler = new Compiler(); const engine = arrowEngine(connection);
    const result = await compiler.query(request, engine);
    const rows = [];
    for await (const batch of batches(result)) {
      assert.equal(batch.schema.fields[0].name, 'id');
      for (let i=0;i<batch.numRows;i++) rows.push([batch.getChild('id').get(i),batch.getChild('name').get(i)]);
    }
    assert.deepEqual(rows, [[2n,null],[3n,'transaction'],[9007199254740993n,'Orchid']]);
    connection.query('ROLLBACK');
    assert.equal(connection.query('SELECT count(*) n FROM people').getChild('n').get(0),2n);
    assert.throws(()=>compiler.compile({...request,query:'this is not a graph query'}));
    assert.throws(()=>compiler.compile({...request,dialect:'clickhouse'}));
    await assert.rejects(compiler.query(request,{...engine,dialect:'postgres'}),/dialects differ/);
  } finally {connection.close();db.reset();}
});

test('generic permission scopes authorize direct and project grants before graph query', async () => {
  const db = await openDatabase(); const connection = db.connect();
  try {
    connection.query("CREATE TABLE documents(id BIGINT, project_id BIGINT, title VARCHAR); INSERT INTO documents VALUES (1,10,'direct'),(2,20,'project'),(3,30,'denied')");
    connection.query("CREATE TABLE effective_grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR); INSERT INTO effective_grants VALUES ('document','view','1','user','','alice'),('project','view','20','user','','alice'),('project','view','30','user','','bob')");
    const compiler = new Compiler();
    const requestWithPermissions = {
      version: 1, dialect: 'duckdb', language: 'cypher',
      query: 'MATCH (d:Document) RETURN d.title AS title ORDER BY title',
      authorization: authorization('user', 'alice'),
      tables: [
        {name:'documents', columns:[{name:'id',data_type:'int64'},{name:'project_id',data_type:'int64'},{name:'title',data_type:'string'}]},
        {name:'effective_grants', columns:[{name:'resource_type',data_type:'string'},{name:'resource_rel',data_type:'string'},{name:'resource_id',data_type:'string'},{name:'subject_type',data_type:'string'},{name:'subject_rel',data_type:'string'},{name:'subject_id',data_type:'string'}]}
      ],
      nodes: [{label:'Document',table:'documents',id:'id',properties:{title:'title',project_id:'project_id'},permission_scopes:[
        permissionScope('id',permissionRelation('effective_grants','document','view')),
        permissionScope('project_id',permissionRelation('effective_grants','project','view'))
      ]}]
    };
    const engine = arrowEngine(connection);
    const result = await compiler.query(requestWithPermissions, engine);
    const rows = [];
    for await (const batch of batches(result)) for (let i=0;i<batch.numRows;i++) rows.push(batch.getChild('title').get(i));
    assert.deepEqual(rows, ['direct','project']);
    assert.throws(() => compiler.compile({...requestWithPermissions, authorization: undefined}), /requires a principal/);
  } finally { connection.close(); db.reset(); }
});

test('early stop releases Arrow lease and permits connection reuse', async()=>{
  const db=await openDatabase(); const connection=db.connect();
  try {
    connection.query('CREATE TABLE people AS SELECT i::BIGINT AS id, NULL::VARCHAR AS name FROM range(10000) t(i)');
    const engine=arrowEngine(connection); const compiler=new Compiler();
    const result=await compiler.query(request,engine);
    await assert.rejects(compiler.query(request,engine),/active result/);
    for await (const batch of batches(result)) {assert.ok(batch.numRows>0);break;}
    await result.close();
    const again=await compiler.query({...request,query:'MATCH (p:Person) RETURN count(p) AS n'},engine);
    for await(const batch of batches(again)) assert.equal(batch.getChild('n').get(0),10000n);
  } finally {connection.close();db.reset();}
});

test('integer parameters preserve signed int64 exactly through compiler and DuckDB Arrow', async () => {
  const db = await openDatabase(); const connection = db.connect();
  try {
    const compiler = new Compiler();
    for (const n of [9007199254740993n, -9007199254740993n, 9223372036854775807n, -9223372036854775808n]) {
      const plan = compiler.compile({...request, query: 'RETURN $n AS n', parameters: {n}});
      assert.equal(connection.query(plan.sql).getChild('n').get(0), n);
    }
    const nested = compiler.compile({...request, query: 'RETURN $values[0] AS n', parameters: {values: [9007199254740993n]}});
    assert.equal(connection.query(nested.sql).getChild('n').get(0), 9007199254740993n);
    for (const n of [9007199254740992, -9007199254740992, NaN, Infinity, -Infinity, 9223372036854775808n, -9223372036854775809n]) {
      assert.throws(() => compiler.compile({...request, query: 'RETURN $n AS n', parameters: {n}}), /Unsafe integer|finite|64-bit range/);
    }
    const text = '9007199254740993\"; DROP TABLE people; --';
    const literal = compiler.compile({...request, query: 'RETURN $n AS n', parameters: {n: text}});
    assert.equal(connection.query(literal.sql).getChild('n').get(0), text);
    for (const n of [42, 0.125, Number.MAX_SAFE_INTEGER]) {
      const plan = compiler.compile({...request, query: 'RETURN $n AS n', parameters: {n}});
      assert.equal(connection.query(`SELECT CAST(n AS DOUBLE) AS n FROM (${plan.sql})`).getChild('n').get(0), n);
    }
  } finally { connection.close(); db.reset(); }
});

test('RDF rules query application columns without a type root', async () => {
  const db = await openDatabase(); const connection = db.connect();
  try {
    connection.query("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'Ada')");
    const compiler = new Compiler();
    const plan = compiler.compile({...request, language: 'sparql', query: 'SELECT ?name WHERE {?s <urn:name> ?name}', rdf: [{
      table: 'people', subject: {kind: 'template', prefix: 'urn:person:', columns: ['id']},
      predicate: {kind: 'constant', value: 'urn:name'}, object: {kind: 'literal', column: 'name'}
    }]});
    assert.equal(connection.query(plan.sql).getChild('?name').get(0), 'Ada');
  } finally { connection.close(); db.reset(); }
});

test('generate, retain, save/load and compile statistics with actual async DuckDB', async () => {
  const {openAsyncDatabase} = await import('./async-duckdb.mjs');
  const {asyncDuckDBEngine} = await import('../dist/index.js');
  const {mkdtempSync, rmSync} = await import('node:fs');
  const {tmpdir} = await import('node:os');
  const {join} = await import('node:path');
  const dir = mkdtempSync(join(tmpdir(),'orchiddb-statistics-'));
  const db = await openAsyncDatabase(); const connection = await db.connect();
  const compiler = new Compiler();
  try {
    await connection.query("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace'), (3, NULL)");
    const engine = asyncDuckDBEngine(connection);
    const analysis = await compiler.generateStatistics(request, engine);
    assert.equal(analysis.snapshot.sources.people.sample_rows, 3);
    assert.equal(analysis.snapshot.sources.people.estimated_rows, 3);
    for(const [language, query] of [['cypher', "MATCH (p:Person) WHERE p.name = 'Ada' RETURN p.name"], ['gremlin', "g.V().hasLabel('Person').has('name', 'Ada').values('name')"], ['sparql', 'SELECT (42 AS ?answer) WHERE {}']]) {
      const plan = compiler.compile({...request, language, query});
      assert.equal(typeof plan.logical_plan, 'string');
      const result = await engine.execute(plan);
      let rows = 0; for await(const batch of batches(result)) rows += batch.numRows;
      assert.equal(rows,1);
    }
    compiler.saveStatistics(join(dir,'statistics.json'));
    compiler.clearStatistics();
    compiler.loadStatistics(join(dir,'statistics.json'));
    assert.deepEqual(compiler.statisticsSnapshot, analysis.snapshot);
    assert.deepEqual(compiler.statisticsReport, analysis.report);
    assert.ok(compiler.compile(request).sql);
    await connection.query("CREATE TABLE nested_items AS SELECT 1::BIGINT id, [{'sku':'a','quantity':2::BIGINT},{'sku':'b','quantity':3::BIGINT}] items");
    const nestedRequest = {...request, nodes:[], tables:[{name:'nested_items', columns:[{name:'id',data_type:'int64'},{name:'items',data_type:'list:struct:{"sku":"string","quantity":"int64"}'}]}]};
    const nested = await compiler.generateStatistics(nestedRequest, engine);
    assert.equal(nested.snapshot.sources.nested_items.columns.items.list.observed_elements, 2);
    assert.equal(nested.snapshot.sources.nested_items.columns.items.list.elements.sku.sample_distinct, 2);
  } finally {compiler.close(); await connection.close(); await db.terminate(); rmSync(dir,{recursive:true});}
});
