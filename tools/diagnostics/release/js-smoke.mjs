import assert from 'node:assert/strict';
import {Connection,batches} from '@orchiddb/client';
import {openDatabase,arrowEngine,schema} from './adapter.mjs';
const db=await openDatabase();const connection=db.connect();
try {
 connection.query("CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,NULL)");
 const graph=new Connection(arrowEngine(connection),schema);
 const rows=[];const result=await graph.query('MATCH (p:Person) WHERE p.id >= $id RETURN p.name AS name ORDER BY p.id',{parameters:{id:1}});
 for await(const batch of batches(result))for(let i=0;i<batch.numRows;i++)rows.push(batch.getChild('name').get(i));
 assert.deepEqual(rows,['Ada',null]);graph.close();assert.equal(connection.query('SELECT 42 AS n').getChild('n').get(0),42);console.log('Packaged JS/native automatic loading and DuckDB Arrow query PASS');
}finally{connection.close();await db.reset();}
