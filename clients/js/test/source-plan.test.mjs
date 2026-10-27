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

for(const one_row of [true,false]) test(`column-free input preserves cardinality ${one_row}`,async()=>{
 const empty=new a.Schema([]);
 const emptyIPC=Array.from(a.tableToIPC(new a.Table(empty),'stream'));
 const node={kind:'input',columns:[],schema_ipc:emptyIPC,one_row};
 const bare=await executeSourcePlan({version:1,engine:'none',nodes:[node],result:0},new Map(),new Map());
 let count=0;for await(const batch of bare){assert.equal(batch.numCols,0);count+=batch.numRows;}
 assert.equal(count,one_row?1:0);
 const local={kind:'local_project',input:0,columns,schema_ipc,expressions:[{kind:'call',function:'answer',arguments:[],returns:'int64'}]};
 const functions=new Map([['answer',(_,rows)=>a.vectorFromArray(Array(rows).fill(42n),new a.Int64())]]);
 const result=await executeSourcePlan({version:1,engine:'none',nodes:[node,local],result:1},new Map(),functions);
 const got=[];for await(const batch of result)got.push(...batch.getChildAt(0));
 assert.deepEqual(got,one_row?[42n]:[]);
});
