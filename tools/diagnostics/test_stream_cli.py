#!/usr/bin/env python3
"""Exercise the CLI with a caller-owned SpiceDB authorization executable."""
import json, pathlib, subprocess, tempfile, os
f=json.loads(pathlib.Path(os.environ['ORCHIDDB_STREAM_FIXTURE']).read_text())
binary=pathlib.Path(__file__).resolve().parents[2]/'target/debug/orchiddb'
def change(key,version): return dict(table='docs',key=key,version=version,data=dict(name=f'{key}-{version}'))
with tempfile.TemporaryDirectory(prefix='orchid-stream-cli-') as tmp:
    tmp=pathlib.Path(tmp)
    def write(name,value):
        path=tmp/name;path.write_text(json.dumps(value));return str(path)
    auth=tmp/'authorize'
    auth.write_text('''#!/usr/bin/env python3
import json,os,urllib.request,sys
f=json.load(open(os.environ['ORCHIDDB_STREAM_FIXTURE']))
r=urllib.request.Request(f['auth_url'],json.dumps(json.load(sys.stdin)).encode(),{'Content-Type':'application/json'})
print(json.dumps(json.load(urllib.request.urlopen(r,timeout=10))))
''');auth.chmod(0o700)
    base=[str(binary),'stream','--schema',write('schema.json',f['schema']),'--config',write('config.json',dict(kafka=f['kafka'],topic=f['topic_prefix']+'-cli',source=f['source'])),'--subject',write('subject.json',f['subject']),'--authorizer',str(auth)]
    def run(*args,deny=False):
        r=subprocess.run(base+list(args),capture_output=True,text=True,timeout=60)
        if deny:
            assert r.returncode and 'denied' in r.stderr,(r.returncode,r.stderr);return
        assert r.returncode==0,r.stderr
        return [json.loads(s) for s in r.stdout.splitlines()]
    run('--insert',write('blocked.json',change('blocked',1)),deny=True)
    run('--insert',write('a.json',change('a',1)))
    run('--insert-batch',write('batch.json',[change('b',1),change('hidden',1),change('a',2)]))
    query=run('--query',f['query'])[0]
    hybrid=run('--query',f['query'],'--snapshot',write('snapshot.json',dict(source=f['source'],offsets=f['offsets'],changes=[change('a',1),change('b',1)])))[0]
    batches=run('--subscribe',f['stream_query'],'--offsets',write('offsets.json',f['offsets']),'--batches','8','--max-records','16','--wait-ms','100')
    frames=[frame for b in batches for frame in b['frames']]
    offsets=batches[-1]['offsets'];assert len(frames)==3 and offsets=={'0':4}
    pathlib.Path('/tmp/orchid-stream-cli-result.json').write_text(json.dumps(dict(query=query,hybrid=hybrid,frames=frames,offsets=offsets,raw_batches=batches)))
    print('CLI Kafka/SpiceDB route passed')
