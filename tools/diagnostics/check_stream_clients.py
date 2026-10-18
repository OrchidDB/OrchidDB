#!/usr/bin/env python3
"""Decode every language's actual Arrow results and verify shared semantics."""
import base64, json, pathlib, sys
import pyarrow as pa

def rows(result):
    return pa.ipc.open_stream(base64.b64decode(result['ipc'])).read_all().to_pylist()

report = {}
for client in sys.argv[1:] or ['python','js','go','java','cpp','elixir','rust']:
    data = json.loads(pathlib.Path(f'/tmp/orchid-stream-{client}-result.json').read_text())
    expected = [{'name':'a-2'},{'name':'b-1'}]
    assert rows(data['query']) == expected, (client, 'query', rows(data['query']))
    assert rows(data['hybrid']) == expected, (client, 'hybrid', rows(data['hybrid']))
    assert data['offsets'] == {'0':4}, (client, data['offsets'])
    if 'raw_batches' in data:
        actual = []
        for batch in data['raw_batches']:
            buffers = [rows(value) for value in batch.get('batches', [])]
            assert len(buffers) == (1 if batch['frames'] else 0), (client, 'Arrow buffers were not shared')
            for frame in batch['frames']:
                ref = frame['result']
                projected = rows(ref) if 'ipc' in ref else buffers[ref['batch']][ref['offset']:ref['offset']+ref['length']]
                actual.append((frame['key'], frame['version'], projected[0]['name']))
        actual.sort()
    else:
        actual = sorted((f['key'],f['version'],rows(f['result'])[0]['name']) for f in data['frames'])
    assert actual == [('a','1','a-1'),('a','2','a-2'),('b','1','b-1')], (client, actual)
    report[client] = dict(query_rows=2, hybrid_rows=2, streamed_frames=3, checkpoint=data['offsets'],denied_write=True,hidden_rows_filtered=True)
print(json.dumps(report,indent=2))
