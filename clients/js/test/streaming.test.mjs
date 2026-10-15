import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {tableFromIPC} from 'apache-arrow';
import {StreamConnection} from '../dist/index.js';
test('Kafka insertion, hybrid and streaming use the host SpiceDB route', {skip:!process.env.ORCHIDDB_STREAM_FIXTURE}, async () => {
 const f=JSON.parse(fs.readFileSync(process.env.ORCHIDDB_STREAM_FIXTURE,'utf8'));
 const authorizer=async(subject,checks)=>{const r=await fetch(f.auth_url,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({subject,checks})});assert.equal(r.status,200);return r.json();};
 const change=(key,version)=>({table:'docs',key,version,data:{name:`${key}-${version}`}});
 const c=new StreamConnection(f.schema,{...f,topic:f.topic_prefix+'-js',authorizer});
 try {
  await assert.rejects(c.insert(change('blocked',1),f.subject),/denied/);
  await c.insert(change('a',1),f.subject);
  await c.insertBatch([change('b',1),change('hidden',1),change('a',2)],f.subject);
  const query=await c.query(f.query,{subject:f.subject});
  const hybrid=await c.query(f.query,{subject:f.subject,snapshot:{source:f.source,offsets:f.offsets,changes:[change('a',1),change('b',1)]}});
  for(const r of [query,hybrid]) assert.deepEqual(tableFromIPC(Buffer.from(r.ipc,'base64')).toArray().map(r=>r.name),['a-2','b-1']);
  await c.subscribe(f.stream_query,f.offsets,{subject:f.subject});
  const frames=[];let offsets;
  for(let i=0;i<20;i++){const b=await c.nextBatch(16,100);frames.push(...b.frames);offsets=b.offsets;if(offsets['0']===4)break;}
  assert.deepEqual(offsets,{'0':4});assert.deepEqual(frames.map(f=>[f.key,f.version]).sort(),[['a','1'],['a','2'],['b','1']]);
  await c.unsubscribe();fs.writeFileSync('/tmp/orchid-stream-js-result.json',JSON.stringify({query,hybrid,frames,offsets}));
 } finally {c.close();}
 const bad=new StreamConnection(f.schema,{...f,topic:f.topic_prefix+'-js',authorizer:()=>[]});
 await assert.rejects(bad.insert(change('a',3),f.subject),/one boolean/);await assert.rejects(bad.query(f.query,{subject:f.subject}),/closed/);bad.close();
});
