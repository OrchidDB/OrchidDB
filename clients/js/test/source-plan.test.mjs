import test from 'node:test';
import assert from 'node:assert/strict';
import * as a from 'apache-arrow';
import {executeSourcePlan} from '../dist/source-plan.js';
const table=a.tableFromArrays({id:new BigInt64Array([1n,2n])});
const columns=[{name:'id',data_type:'int64',nullable:true}];
const schema_ipc=Array.from(a.tableToIPC(table.slice(0,0),'stream'));
function plan(engine='none', local=false){
 const nodes=[{kind:'source_sql',source:'db',dialect:'postgres',sql:'SELECT id FROM people',columns,schema_ipc}];
 if(local)nodes.push({kind:'local_project',input:0,columns,schema_ipc,expressions:[{kind:'call',function:'double',arguments:[{kind:'column',index:0}],returns:'int64'}]});
 return {version:1,engine,nodes,result:nodes.length-1};
}
function source(){let calls=0,closed=0;return {get calls(){return calls;},get closed(){return closed;},dialect:'postgres',async execute(work){
 calls++;assert.equal(work.sql,'SELECT id FROM people');return {schema:table.schema,close(){closed++;},async *[Symbol.asyncIterator](){yield* table.batches;}};
}};}
for(const engine of ['none','duckdb'])test(`direct source skips engine ${engine}`,async()=>{
 const db=source();const result=await executeSourcePlan(plan(engine),new Map([['db',db]]),new Map(),{dialect:'duckdb',executeInputs(){assert.fail('engine called');}});
 const rows=[];for await(const b of result)rows.push(...b.getChildAt(0));assert.deepEqual(rows,[1n,2n]);assert.equal(db.calls,1);assert.equal(db.closed,1);
});
test('local UDF preflight and result',async()=>{
 const db=source();await assert.rejects(executeSourcePlan(plan('none',true),new Map([['db',db]]),new Map()),/Missing local/);assert.equal(db.calls,0);
 const functions=new Map([['double',(args,rows)=>{assert.equal(rows,2);return a.vectorFromArray([...args[0]].map(v=>v*2n),new a.Int64());}]]);
 const result=await executeSourcePlan(plan('none',true),new Map([['db',db]]),functions);const rows=[];for await(const b of result)rows.push(...b.getChildAt(0));assert.deepEqual(rows,[2n,4n]);
});
