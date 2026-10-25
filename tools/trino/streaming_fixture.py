#!/usr/bin/env python3
"""Manage an isolated Trino node with the real Orchid Arrow connector installed."""
from pathlib import Path
import json, subprocess, sys, time, urllib.request
ROOT=Path(__file__).resolve().parents[2]
STATE=ROOT/'target/trino-streaming'
NAME='orchiddb-trino-streaming'
TOKEN='orchid-test-exchange-token-32-bytes-minimum'
def run(*args): return subprocess.check_output(args,text=True).strip()
def main():
    command=sys.argv[1] if len(sys.argv)>1 else 'up'
    if command=='down':
        subprocess.run(['docker','stop',NAME],check=False)
        subprocess.run(['docker','rm',NAME],check=False)
        return
    if command!='up': raise SystemExit('usage: streaming_fixture.py up|down')
    if not (ROOT/'target/trino-connector/plugin/orchid-trino.jar').exists():
        raise SystemExit('Run tools/trino/build_connector.sh first')
    if subprocess.run(['docker','inspect',NAME],capture_output=True).returncode==0:
        raise SystemExit(f'{NAME} already exists; use down before recreating this test fixture')
    (STATE/'catalog').mkdir(parents=True,exist_ok=True)
    (STATE/'config.properties').write_text('coordinator=true\nnode-scheduler.include-coordinator=true\nhttp-server.http.port=8080\ndiscovery.uri=http://localhost:8080\nquery.max-memory=512MB\nquery.max-memory-per-node=512MB\n')
    (STATE/'node.properties').write_text('node.environment=streaming\nnode.id=orchid-streaming-node\nnode.data-dir=/tmp/trino-data\n')
    (STATE/'jvm.config').write_text('-server\n-Xmx1G\n-XX:+UseG1GC\n--add-opens=java.base/java.nio=ALL-UNNAMED\n--enable-native-access=ALL-UNNAMED\n')
    (STATE/'catalog/orchid.properties').write_text('connector.name=orchid_arrow\nexchange.uri=http://host.docker.internal:18883\nexchange.bearer-token='+TOKEN+'\n')
    run('docker','run','-d','--name',NAME,'--add-host','host.docker.internal:host-gateway','-p','127.0.0.1:18083:8080',
        '-v',f'{STATE}:/etc/trino:ro','-v',f'{ROOT}/target/trino-connector/plugin:/usr/lib/trino/plugin/orchid:ro','trinodb/trino:484')
    deadline=time.monotonic()+120
    while True:
        try:
            req=urllib.request.Request('http://127.0.0.1:18083/v1/info')
            with urllib.request.urlopen(req,timeout=5) as response:
                data=json.load(response)
                if not data.get('starting',True): break
        except OSError: pass
        state=run('docker','inspect','--format','{{.State.Status}}',NAME)
        if state=='exited' or time.monotonic()>deadline:
            logs=subprocess.check_output(['docker','logs',NAME],stderr=subprocess.STDOUT,text=True)
            (STATE/'startup.log').write_text(logs)
            raise RuntimeError('Trino did not start; see target/trino-streaming/startup.log')
        time.sleep(1)
    print('Trino streaming fixture ready on 127.0.0.1:18083')
if __name__=='__main__':main()
