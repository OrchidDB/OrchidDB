#!/usr/bin/env python3
"""Local-only Kafka/SpiceDB client fixture. Run in a terminal until clients finish."""
import json, os, pathlib, subprocess, time, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPICE = 'http://127.0.0.1:18443'
TOKEN = 'orchid-clients-test-key'  # Isolated in-memory test instance only.
def spice(path, data):
    request = urllib.request.Request(SPICE + path, json.dumps(data).encode(),
        {'Authorization': 'Bearer '+TOKEN, 'Content-Type': 'application/json'})
    return json.load(urllib.request.urlopen(request, timeout=10))

def authorize(subject, checks):
    return [spice('/v1/permissions/check', {'consistency': {'fullyConsistent': True},
        'resource': {'objectType': c['resource_type'], 'objectId': c['resource_id']},
        'permission': c['permission'], 'subject': {'object': {
            'objectType': subject['subject_type'], 'objectId': subject['subject_id']}}})['permissionship'] == 'PERMISSIONSHIP_HAS_PERMISSION' for c in checks]

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        try:
            v = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            decisions = authorize(v['subject'], v['checks'])
            data = json.dumps(decisions).encode()
            self.send_response(200); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)
        except Exception:
            self.send_response(503); self.end_headers()
    def log_message(self, *args): pass

def main():
    spice('/v1/schema/write', {'schema': 'definition user {}\ndefinition document {\n relation writer: user\n relation reader: user\n permission edit = writer\n permission view = reader\n}'})
    updates = []
    for key, relations in [('a', ['writer', 'reader']), ('b', ['writer', 'reader']), ('hidden', ['writer'])]:
        for relation in relations:
            updates.append({'operation':'OPERATION_TOUCH','relationship': {'resource': {'objectType':'document','objectId':key}, 'relation':relation, 'subject':{'object':{'objectType':'user','objectId':'alice'}}}})
    spice('/v1/relationships/write', {'updates': updates})
    prefix = 'orchid-clients-' + str(time.time_ns())
    for client in ['python','js','go','java','cpp','elixir','rust','cli']:
        subprocess.run(['docker','exec','orchid-clients-stream-kafka-1','/opt/kafka/bin/kafka-topics.sh','--bootstrap-server','localhost:19092','--create','--topic',prefix+'-'+client,'--partitions','1','--replication-factor','1'], check=True, stdout=subprocess.DEVNULL)
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    fixture = {'kafka': {'bootstrap.servers':'127.0.0.1:19092'}, 'topic_prefix': prefix, 'source':'clients-test-v1',
        'auth_url': f'http://127.0.0.1:{server.server_port}/check',
        'subject': {'subject_type':'user','subject_id':'alice'},
        'schema': {'tables': {'docs': {'label':'Doc','resource_type':'document','read_permission':'view','write_permission':'edit','properties':{'type':'object','properties':{'name':{'type':'string'}},'required':['name']}}}},
        'query':'MATCH (d:Doc) RETURN d.name AS name ORDER BY name',
        'stream_query':'MATCH (d:Doc) RETURN d.name AS name',
        'offsets': {'0':0}}
    path = pathlib.Path('/tmp/orchid-stream-clients.json'); path.write_text(json.dumps(fixture))
    print(path, flush=True)
    server.serve_forever()

if __name__ == '__main__': main()
