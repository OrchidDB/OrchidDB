import base64
import json
import os
import urllib.request
from pathlib import Path
import pytest
from orchiddb import StreamConnection

@pytest.mark.skipif(not os.environ.get('ORCHIDDB_STREAM_FIXTURE'), reason='live Kafka/SpiceDB fixture required')
def test_streaming_spicedb_route():
    import pyarrow as pa
    f = json.loads(Path(os.environ['ORCHIDDB_STREAM_FIXTURE']).read_text())
    checks = []
    def auth(subject, batch):
        checks.extend(batch)
        request = urllib.request.Request(f['auth_url'], json.dumps(dict(subject=subject,checks=batch)).encode(), {'Content-Type':'application/json'})
        return json.load(urllib.request.urlopen(request,timeout=10))
    def change(key, version):
        return dict(table='docs',key=key,version=version,data=dict(name=f'{key}-{version}'))
    def rows(result):
        return pa.ipc.open_stream(base64.b64decode(result['ipc'])).read_all().to_pylist()
    def connection(authorizer=auth):
        return StreamConnection(f['schema'],kafka=f['kafka'],topic=f['topic_prefix']+'-python',source=f['source'],authorizer=authorizer)
    with connection() as c:
        with pytest.raises(Exception,match='denied'):
            c.insert(change('blocked',1),subject=f['subject'])
        c.insert(change('a',1),subject=f['subject'])
        c.insert_batch([change('b',1),change('hidden',1),change('a',2)],subject=f['subject'])
        result=c.query(f['query'],subject=f['subject'])
        assert rows(result)==[{'name':'a-2'},{'name':'b-1'}]
        hybrid=c.query(f['query'],subject=f['subject'],snapshot=dict(source=f['source'],offsets=f['offsets'],changes=[change('a',1),change('b',1)]))
        assert rows(hybrid)==rows(result)
        c.subscribe(f['stream_query'],subject=f['subject'],offsets=f['offsets'])
        frames=[]; offsets=None; raw_batches=[]
        for _ in range(20):
            batch=c.next_batch(16,100)
            raw_batches.append(batch)
            frames.extend(batch['frames']); offsets=batch['offsets']
            if offsets['0']==4: break
        assert offsets=={'0':4}
        assert sorted((x['key'],x['version']) for x in frames)==[('a','1'),('a','2'),('b','1')]
        c.unsubscribe()
        c.subscribe(f['stream_query'],subject=f['subject'],offsets=offsets)
        assert c.next_batch(16,20)['frames']==[]
        c.unsubscribe()
        Path('/tmp/orchid-stream-python-result.json').write_text(json.dumps(dict(query=result,hybrid=hybrid,frames=frames,offsets=offsets,raw_batches=raw_batches)))
    assert {x['permission'] for x in checks}=={'edit','view'}
    with connection(lambda s,c: [True]*(len(c)-1)) as c:
        with pytest.raises(ValueError): c.insert(change('a',3),subject=f['subject'])
        with pytest.raises(RuntimeError,match='closed'): c.query(f['query'],subject=f['subject'])

@pytest.mark.skipif(not os.environ.get('ORCHIDDB_STREAM_FIXTURE'), reason='live Kafka fixture required')
def test_expired_auth_and_configured_limits_fail_closed():
    import time
    import pyarrow as pa
    f = json.loads(Path(os.environ['ORCHIDDB_STREAM_FIXTURE']).read_text())
    options=dict(kafka=f['kafka'],topic=f['topic_prefix']+'-python',source=f['source'])
    def late(subject, checks):
        time.sleep(.03)
        return [True]*len(checks)
    with StreamConnection(f['schema'],authorizer=late,auth_timeout_ms=5,**options) as c:
        with pytest.raises(Exception,match='expired|timed out'):
            c.insert(dict(table='docs',key='a',version=999,data=dict(name='late-write')),subject=f['subject'])
    with StreamConnection(f['schema'],authorizer=lambda s,c:[True]*len(c),**options) as c:
        result=c.query("MATCH (d:Doc) WHERE d.name = 'late-write' RETURN d.name",subject=f['subject'])
        assert pa.ipc.open_stream(base64.b64decode(result['ipc'])).read_all().num_rows==0
    with StreamConnection(f['schema'],authorizer=lambda s,c:[True]*len(c),limits=dict(records=1),**options) as c:
        with pytest.raises(Exception,match='record limit'):
            c.query(f['query'],subject=f['subject'])
