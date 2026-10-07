import {test} from 'node:test';
import assert from 'node:assert/strict';
import pg from 'pg';
import {tableFromArrays,vectorFromArray,List,Field,Int64} from 'apache-arrow';
import {Compiler,queryFederated} from '../dist/index.js';
import {openAsyncDatabase} from './async-duckdb.mjs';
const uri=process.env.ORCHIDDB_TEST_PG_URI;
const result=table=>({schema:table.schema,async *[Symbol.asyncIterator](){yield*table.batches;},close(){}});
const quote=name=>'"'+name.replaceAll('"','""')+'"';
test('real PostgreSQL and DuckDB islands in both directions',{skip:!uri},async()=>{
 const client=new pg.Client({connectionString:uri});await client.connect();
 const db=await openAsyncDatabase();const conn=await db.connect();const compiler=new Compiler();
 try {
  for(const execute of [sql=>client.query(sql),sql=>conn.query(sql)]) {
   await execute('CREATE TEMP TABLE people(id BIGINT,name VARCHAR,age BIGINT)');
   await execute("INSERT INTO people VALUES (1,'Ada',30),(2,'Bob',20)");
   await execute('CREATE TEMP TABLE links(id BIGINT,src BIGINT,dst BIGINT)');await execute('INSERT INTO links VALUES (1,1,2)');
  }
  await client.query("CREATE TEMP TABLE nested_values(id BIGINT, items JSONB[])");
  await client.query("INSERT INTO nested_values VALUES (1,ARRAY['[1]'::jsonb,'[2,3]'::jsonb,NULL,'[]'::jsonb])");
  await conn.query("CREATE TEMP TABLE nested_values(id BIGINT, items BIGINT[][])");
  await conn.query("INSERT INTO nested_values VALUES (1,[[1],[2,3],NULL,[]])");
  const readOnly = sql => { assert.match(sql,/^\s*(SELECT|WITH)\b/i); assert.doesNotMatch(sql,/\b(CREATE|DROP|INSERT)\s+(TEMP|TABLE|INTO)/i); };
  const engines=new Map([
   ['d',{id:'d',dialect:'duckdb',execute:async q=>{readOnly(q.sql);return result(await conn.query(q.sql));}}],
   ['p',{id:'p',dialect:'postgres',execute:async q=>{
     readOnly(q.sql);
     const rows=await client.query({text:q.sql,rowMode:'array'});
     return result(tableFromArrays(Object.fromEntries(rows.fields.map((f,i)=>[f.name,f.dataTypeID===3807
       ? vectorFromArray(rows.rows.map(r=>r[i]?.map(child=>child?.map(v=>v===null?null:BigInt(v)) ?? null) ?? null),new List(new Field('item',new List(new Field('item',new Int64(),true)),true)))
       : rows.rows.map(r=>f.dataTypeID===20&&r[i]!==null?BigInt(r[i]):r[i])]))));
   }}]
  ]);
  for(const target of ['d','p']) {
   const request={version:1,dialect:target==='d'?'duckdb':'postgres',execution_engine:target,engines:{d:{dialect:'duckdb'},p:{dialect:'postgres'}},language:'cypher',
    query:'MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.age > 25 RETURN a.name AS a, b.name AS b',
    tables:[{name:'people',engine:'p',columns:[{name:'id',data_type:'int64'},{name:'name',data_type:'string'},{name:'age',data_type:'int64'}]},
     {name:'links',engine:'d',columns:['id','src','dst'].map(name=>({name,data_type:'int64'}))}],
    nodes:[{label:'Person',table:'people',id:'id',properties:{name:'name',age:'age'}}],
    edges:[{label:'KNOWS',table:'links',id:'id',source:'src',target:'dst',source_label:'Person',target_label:'Person'}]};
   const rows=await queryFederated(compiler,request,engines,async stream=>{const rows=[];for await(const batch of stream)for(const row of batch.toArray())rows.push([row.a,row.b]);return rows;});
   assert.deepEqual(rows,[['Ada','Bob']]);
   const nested={...request,query:'MATCH (n:Nested) RETURN ncount(n.items) AS n',
     tables:[{name:'nested_values',engine:target==='p'?'d':'p',columns:[{name:'id',data_type:'int64'},{name:'items',data_type:'list:list:int64'}]}],
     nodes:[{label:'Nested',table:'nested_values',id:'id',properties:{items:'items'}}],edges:[],
     functions:[{name:'ncount',target:target==='p'?'cardinality':'len',parameters:['list:list:int64'],returns:'int64'}]};
   const counts=await queryFederated(compiler,nested,engines,async stream=>{const rows=[];for await(const batch of stream)for(const row of batch.toArray())rows.push(Number(row.n));return rows;});
   assert.deepEqual(counts,[4]);
  }
 } finally {compiler.close();await conn.close();await db.terminate();await client.end();}
});
