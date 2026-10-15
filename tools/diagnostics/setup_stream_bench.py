"""Initialize ONLY the isolated benchmark SpiceDB (loopback port 18443)."""
import json, sys, urllib.request

def post(path, data):
    request = urllib.request.Request('http://127.0.0.1:18443/v1/' + path,
        data=json.dumps(data).encode(),headers={'Authorization':'Bearer orchid-benchmark-only','Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)

post('schema/write', {'schema': '''definition user {}
definition event {
 relation viewer: user:*
 relation editor: user:*
 permission view = viewer
 permission edit = editor
}'''})
count = int(sys.argv[1]) if len(sys.argv)>1 else 3000
for start in range(0, count, 250):
    updates = [{'operation':'OPERATION_TOUCH','relationship':{
        'resource':{'objectType':'event','objectId':str(i)}, 'relation':relation,
        'subject':{'object':{'objectType':'user','objectId':'*'}}}}
        for i in range(start,min(start+250,count)) for relation in ['viewer','editor']]
    post('relationships/write', {'updates':updates})
print(json.dumps({'authorized_entities':count}))
