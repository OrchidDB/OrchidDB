import assert from 'node:assert/strict';
import {Connection,SourceConnection,batches} from '@orchiddb/client';
import {openDatabase,arrowEngine,schema} from './adapter.mjs';
const db=await openDatabase();const connection=db.connect();
try {
 connection.query("CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,NULL)");
 const graph=new Connection(arrowEngine(connection),schema);
 const rows=[];const result=await graph.query('MATCH (p:Person) WHERE p.id >= $id RETURN p.name AS name ORDER BY p.id',{parameters:{id:1}});
 for await(const batch of batches(result))for(let i=0;i<batch.numRows;i++)rows.push(batch.getChild('name').get(i));
 assert.deepEqual(rows,['Ada',null]);graph.close();assert.equal(connection.query('SELECT 42 AS n').getChild('n').get(0),42);console.log('Packaged JS/native automatic loading and DuckDB Arrow query PASS');
}finally{connection.close();await db.reset();}

const a=await import('apache-arrow');
const localSchema={functions:[{name:'twice',target:'twice',parameters:['int64'],returns:'int64',local:true}]};
for(const engine of ['none','duckdb']) {
 const graph=new SourceConnection(localSchema,engine,new Map(),new Map([['twice',(args)=>a.vectorFromArray([...args[0]].map(v=>v*2n),new a.Int64())]]),{dialect:'duckdb',executeInputs(){assert.fail('DuckDB invoked');}});
 try {
  const result=await graph.query('RETURN twice(21) AS answer');
  const rows=[];for await(const batch of result)rows.push(...batch.getChildAt(0));
  assert.deepEqual(rows,[42n]);await result.close();
 } finally {graph.close();}
}
console.log('Packaged JS source-first native UDF execution PASS');
