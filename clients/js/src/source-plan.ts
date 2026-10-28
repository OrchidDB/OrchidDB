import type { ArrowResult, CompiledQuery, Dialect } from './internal.js';
import type { Vector } from 'apache-arrow';
export type RelationalEngine = 'none' | 'duckdb';
export interface SqlSource {
  readonly dialect: string;
  readonly concurrencyKey?: string;
  readonly supportsConcurrentStatements?: boolean;
  execute?(query: CompiledQuery): Promise<ArrowResult>;
  executeRequests?(requests:readonly unknown[],columns:readonly unknown[]):Promise<ArrowResult>;
}
export interface CompletionEngine extends SqlSource {
  executeInputs(query: CompiledQuery, inputs: ReadonlyMap<string, ArrowResult>): Promise<ArrowResult>;
}
export type LocalFunction = (arguments_: readonly Vector[], rows: number) => Vector;
type Expr = {kind:'column';index:number} | {kind:'literal';ipc:number[]} |
  {kind:'cast';input:Expr;data_type:string} |
  {kind:'graph_element';input:Expr;data_type:string} |
  {kind:'call';function:string;arguments:Expr[];returns:string};
type Node = {kind:string; columns:{name:string;data_type:string;nullable:boolean}[]; schema_ipc:number[];
  source?:string;dialect?:Dialect;sql?:string;input?:number;expressions?:Expr[];one_row?:boolean;
  inputs?:{name:string;node:number}[]; template?:{adapter:string;parameters:number;request:any;bindings:{pointer:string;parameter:number;encoding?:string}[]}};
export interface SourcePlan {version:number;engine:RelationalEngine;nodes:Node[];result:number}

/** Interpret the shared plan with application-owned adapters. No SQL driver. */
export async function executeSourcePlan(plan:SourcePlan, sources:ReadonlyMap<string,SqlSource>,
    functions:ReadonlyMap<string,LocalFunction>, duckdb?:CompletionEngine, decode?:(command:unknown)=>any):Promise<ArrowResult> {
  const a = await import('apache-arrow');
  // Arrow JS 17 infers RecordBatch length from its children, losing the one
  // logical row of a column-free input. Preserve the explicit struct data.
  class InputBatch extends a.RecordBatch {
    override readonly data: import('apache-arrow').Data<import('apache-arrow').Struct>;
    constructor(schema: import('apache-arrow').Schema, rows: number) {
      const data = a.makeData({type:new a.Struct(schema.fields),length:rows,nullCount:0,children:[]});
      super(schema,data);
      this.data = data;
    }
  }
  if(plan.version!==1 || !plan.nodes[plan.result]) throw new Error('Invalid source plan');
  const schemas=plan.nodes.map(n=>a.tableFromIPC(Uint8Array.from(n.schema_ipc)).schema);
  const logicalType=(name:string):import('apache-arrow').DataType|undefined=>{
    const scalar:Record<string,import('apache-arrow').DataType>={int64:new a.Int64(),int32:new a.Int32(),float64:new a.Float64(),float32:new a.Float32(),boolean:new a.Bool(),string:new a.Utf8(),binary:new a.Binary()};
    if(scalar[name])return scalar[name];
    if(name.startsWith('list<')&&name.endsWith('>')){const item=logicalType(name.slice(5,-1));if(item)return new a.List(new a.Field('item',item,true));}
    return undefined;
  };

  function check(e:Expr,width:number):void {
    if(e.kind==='column' && (e.index<0 || e.index>=width)) throw new Error('Column out of bounds');
    if(e.kind==='call') {
      if(!functions.has(e.function)) throw new Error(`Missing local function ${e.function}`);
      e.arguments.forEach(x=>check(x,width));
    } else if(e.kind==='cast'||e.kind==='graph_element') {
      if(e.kind==='graph_element'&&!decode)throw new Error('Graph representation decoder is required');
      check(e.input,width);
    }
    else if(e.kind==='literal') {
      const t=a.tableFromIPC(Uint8Array.from(e.ipc));
      if(t.numRows!==1 || t.numCols!==1) throw new Error('Invalid literal');
    }
  }
  for(const [i,n] of plan.nodes.entries()) {
    const deps=n.input==null?[]:[n.input];
    if(n.kind==='source_sql') {
      if(sources.get(n.source!)?.dialect!==n.dialect || !sources.get(n.source!)?.execute) throw new Error(`Missing source ${n.source}`);
    } else if(n.kind==='source_request') {
      if(sources.get(n.source!)?.dialect!==n.template!.adapter || !sources.get(n.source!)?.executeRequests) throw new Error(`Missing request source ${n.source}`);
    } else if(n.kind==='local_project') {
      n.expressions!.forEach(e=>check(e,plan.nodes[n.input!]?.columns.length??0));
    } else if(n.kind==='duckdb') {
      if(plan.engine!=='duckdb' || duckdb?.dialect!=='duckdb' || !duckdb.executeInputs) throw new Error('DuckDB completion binding required');
      deps.push(...n.inputs!.map(x=>x.node));
    } else if(n.kind!=='input') throw new Error(`Unsupported source-plan operation ${n.kind}`);
    if(deps.some(d=>d<0||d>=i)) throw new Error('Plan dependency out of order');
  }
  const live:ArrowResult[]=[];
  const outputs=new Map<number,ArrowResult>();
  const close=async()=>{
    const errors:unknown[]=[];
    for(const r of live.splice(0).reverse()) try {await r.close();} catch(e) {errors.push(e);}
    if(errors.length) throw new AggregateError(errors,'Closing source plan');
  };
  async function spool(result:ArrowResult):Promise<ArrowResult> {
    const fs=await import('node:fs/promises'); const {tmpdir}=await import('node:os'); const {join}=await import('node:path');
    const dir=await fs.mkdtemp(join(tmpdir(),'orchid-plan-')); let count=0;
    try {
      for await(const batch of result) await fs.writeFile(join(dir,String(count++)),a.tableToIPC(new a.Table(batch),'stream'));
      await result.close();
    } catch(e) {await fs.rm(dir,{recursive:true,force:true});throw e;}
    let closed=false;
    return {schema:result.schema,async close(){if(!closed){closed=true;await fs.rm(dir,{recursive:true,force:true});}},async *[Symbol.asyncIterator](){
      for(let i=0;i<count&&!closed;i++) yield* a.tableFromIPC(await fs.readFile(join(dir,String(i)))).batches;
    }};
  }
  const work=(n:Node):CompiledQuery=>({version:1,dialect:n.dialect??'duckdb',sql:n.sql!,fields:n.columns.map(c=>c.name),field_types:n.columns.map(c=>c.data_type)});
  const evaluate=(e:Expr,b:import('apache-arrow').RecordBatch):Vector=>{
    if(e.kind==='column') return b.getChildAt(e.index)!;
    if(e.kind==='literal') {
      const value=a.tableFromIPC(Uint8Array.from(e.ipc)).getChildAt(0)!;
      return a.vectorFromArray(Array.from({length:b.numRows},()=>value.get(0)),value.type);
    }
    // Declared-argument casts are type checks: compiler overload binding already
    // requires exact argument types. No implicit host-language numeric coercion.
    if(e.kind==='cast') {const value=evaluate(e.input,b);const type=logicalType(e.data_type);if(value.type instanceof a.Null && type)return a.vectorFromArray(Array(b.numRows).fill(null),type);return value;}
    if(e.kind==='graph_element') {
      const descriptor=evaluate(e.input,b);
      const ipc=Array.from(a.tableToIPC(new a.Table({descriptor}),'stream'));
      const result=decode!({op:'decode_graph_values',data_type:e.data_type,ipc});
      if(!Array.isArray(result)||result.length!==b.numRows)throw new Error('Invalid graph decoder row count');
      return a.vectorFromArray(result,new a.Struct([new a.Field('__orchiddb_value_v1',new a.Utf8(),true)]));
    }
    const value=functions.get(e.function)!(e.arguments.map(x=>evaluate(x,b)),b.numRows);
    const expected=logicalType(e.returns);
    if(value.length!==b.numRows || (expected&&expected.toString()!==value.type.toString())) throw new Error(`Invalid type or row count from ${e.function}`);
    return value;
  };
  try {
    for(const [i,n] of plan.nodes.entries()) {
      const schema=schemas[i]; let result:ArrowResult;
      if(n.kind==='source_sql') result=await sources.get(n.source!)!.execute!(work(n));
      else if(n.kind==='duckdb') result=await duckdb!.executeInputs(work(n),new Map(n.inputs!.map(r=>[r.name,outputs.get(r.node)!])));
      else if(n.kind==='source_request') {
        const input=n.input==null?undefined:outputs.get(n.input)!;
        let active:ArrowResult|undefined;
        result={schema,async close(){await active?.close();active=undefined;},async *[Symbol.asyncIterator](){
          const bind=(values:any[])=>{
            if(values.length!==n.template!.parameters) throw new Error('Request parameter count mismatch');
            let payload=structuredClone(n.template!.request);
            for(const b of n.template!.bindings){
              const value=b.encoding==='sql_null'?values[b.parameter]==null:values[b.parameter];
              if(b.pointer===''){payload=value;continue;}
              const keys=b.pointer.slice(1).split('/').map(k=>k.replaceAll('~1','/').replaceAll('~0','~'));
              let parent=payload;for(const key of keys.slice(0,-1))parent=parent[key];
              if(!(keys.at(-1)! in parent))throw new Error('Invalid request binding pointer');parent[keys.at(-1)!]=value;
            }
            return payload;
          };
          async function* send(requests:unknown[]){
            active=await sources.get(n.source!)!.executeRequests!(requests,n.columns);
            try{yield* active;}finally{await active.close();active=undefined;}
          }
          if(!input){yield* send([bind([])]);return;}
          for await(const batch of input){
            for(let start=0;start<batch.numRows;start+=64){
              const requests=[];
              for(let row=start;row<Math.min(start+64,batch.numRows);row++)requests.push(bind(batch.schema.fields.map((_,i)=>batch.getChildAt(i)!.get(row))));
              yield* send(requests);
            }
          }
        }};
      }
      else {
        const input=n.input===undefined?undefined:outputs.get(n.input)!;
        result={schema,close(){},async *[Symbol.asyncIterator](){
          if(n.kind==='input') {
            yield new InputBatch(schema,n.one_row?1:0);
          } else {
            for await(const batch of input!) {
              const vectors=n.expressions!.map(e=>evaluate(e,batch));
              for(let k=0;k<vectors.length;k++) if(vectors[k].type.toString()!==schema.fields[k].type.toString()) throw new Error('Local UDF output type mismatch');
              yield new a.RecordBatch(schema,a.makeData({type:new a.Struct(schema.fields),length:batch.numRows,children:vectors.map(v=>v.data[0])}));
            }
          }
        }};
      }
      const source=n.source===undefined?undefined:sources.get(n.source);
      const reentry=n.kind==='duckdb' ? plan.nodes.slice(i+1).some(x=>x.kind==='duckdb') :
        source && !source.supportsConcurrentStatements && plan.nodes.slice(i+1).some(x=>x.source!==undefined &&
          (x.source===n.source || (source.concurrencyKey!==undefined && sources.get(x.source)?.concurrencyKey===source.concurrencyKey)));
      if(reentry) result=await spool(result);
      live.push(result); outputs.set(i,result);
    }
    const result=outputs.get(plan.result)!; let closed=false;
    return {schema:result.schema,async close(){if(!closed){closed=true;await close();}},async *[Symbol.asyncIterator](){
      if(closed) throw new Error('Result closed');
      try { yield* result; } finally {closed=true;await close();}
    }};
  } catch(e) {await close();throw e;}
}
