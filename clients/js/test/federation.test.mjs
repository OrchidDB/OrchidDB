import {test} from 'node:test';
import assert from 'node:assert/strict';
import {queryFederated} from '../dist/index.js';

for (const fail of [false,true]) test(`federation routes read queries and closes results (failure=${fail})`,async()=>{
 const log=[];
 const transfer={source_engine:'pg',source_dialect:'postgres',sql:'SELECT count(*) AS n FROM people WHERE age > 25',target_relation:'exchange',columns:[{name:'n',data_type:'int64',nullable:false}]};
 const plan={version:1,dialect:'duckdb',execution_engine:'dd',sql:'SELECT n FROM exchange',fields:['n'],transfers:[transfer]};
 const compiler={compile:()=>plan,bindArrow:async(p,relation,source)=>{assert.equal(relation,'exchange');log.push('bind');return {...p,transfers:[]};}};
 const engines=new Map([
  ['pg',{dialect:'postgres',execute:async q=>{log.push(q.sql);return {close:()=>log.push('source closed')}}}],
  ['dd',{dialect:'duckdb',execute:async q=>{assert.deepEqual(q.transfers,[]);return {close:()=>log.push('result closed')}}}]
 ]);
 const run=queryFederated(compiler,{},engines,async result=>{if(fail)throw Error('consumer failed');return 7});
 if(fail)await assert.rejects(run,/consumer failed/);else assert.equal(await run,7);
 assert.deepEqual(log,[transfer.sql,'bind','source closed','result closed']);
});
test('routing validation happens before opening any result',async()=>{
 const compiler={compile:()=>({version:1,dialect:'postgres',execution_engine:'p',transfers:[]})};
 await assert.rejects(queryFederated(compiler,{},new Map(),async()=>{}),/Missing engine/);
});

test('dependent requests close inputs and follow rewritten transfer dependencies', async () => {
  const log = [];
  const columns = [{name:'id', data_type:'int64', nullable:false}];
  const first = {source_engine:'db',source_dialect:'duckdb',sql:'INPUT',target_relation:'remote',columns,
    request:{engine:'search',input_columns:columns,template:{adapter:'quickwit'}}};
  const second = {source_engine:'db',source_dialect:'duckdb',sql:'STALE',target_relation:'join',columns};
  const plan = {version:1,dialect:'duckdb',execution_engine:'db',sql:'FINAL',fields:['id'],transfers:[first,second]};
  const compiler = {
    compile:()=>plan,
    bindOperationArrow:async()=>({engine:'search',requests:[{id:9007199254740993n}]}),
    bindArrow:async(p,relation)=>({...p,transfers:relation==='remote'?[{...second,sql:'BOUND'}]:[]}),
  };
  const engines = new Map([
    ['db',{dialect:'duckdb',execute:async q=>{log.push(q.sql);assert.notEqual(q.sql,'STALE');return {close(){log.push('closed '+q.sql);}};}}],
    ['search',{dialect:'quickwit',executeRequests:async requests=>{
      assert.equal(log.at(-1),'closed INPUT'); assert.deepEqual(requests,[{id:9007199254740993n}]);
      return {close(){log.push('closed request');}};
    }}],
  ]);
  await queryFederated(compiler,{},engines,async()=>7);
  assert.deepEqual(log,['INPUT','closed INPUT','closed request','BOUND','closed BOUND','FINAL','closed FINAL']);
});

test('closed requests bind one empty parameter row and propagate failures', async()=>{
  const transfer={source_engine:'search',source_dialect:'quickwit',sql:'',target_relation:'remote',columns:[],
    request:{engine:'search',input_columns:[],template:{adapter:'quickwit'}}};
  const compiler={compile:()=>({version:1,dialect:'duckdb',execution_engine:'db',transfers:[transfer]}),
    operationCommand:command=>{assert.deepEqual(command.rows,[[]]);return {engine:'search',requests:[{}]};}};
  const engines=new Map([['db',{dialect:'duckdb',execute:()=>{throw Error('must not execute');}}],
    ['search',{dialect:'quickwit',executeRequests:()=>{throw Error('remote failed');}}]]);
  await assert.rejects(queryFederated(compiler,{},engines,async()=>{}),/remote failed/);
});
