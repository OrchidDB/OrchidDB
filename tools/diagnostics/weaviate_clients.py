"""Run every language against the smoke test's live, counted Weaviate proxy."""
import copy, json, os, pathlib, subprocess, sys, tempfile
ROOT=pathlib.Path(__file__).resolve().parents[2]

def run_clients(config, setup_sql, endpoint, token, calls):
    import pyarrow as pa
    request=copy.deepcopy(config)
    request=json.loads(json.dumps(request).replace('"vectors"','"text"'))
    request.update(version=1,dialect='duckdb',language='cypher',query='MATCH (q:Question)-[r:RELEVANT_TO {score:0,limit:1}]->(d:Document) RETURN q.id AS question,d.id AS document ORDER BY question')
    expected=[[10,1],[20,2],[30,1],[40,3],[50,2]]
    options=dict(endpoint=endpoint,batch_size=2,authentication=dict(type='api_key',token=token))
    fixture=dict(expected_adapters=['weaviate'],cases=[dict(name='weaviate-batched-rag-edge',adapter='weaviate',endpoint=endpoint,options=options,request=request,setup_sql=[setup_sql,"INSERT INTO questions VALUES (30,'a',[1,0]),(40,'b',[1,0]),(50,'a',[0,1])"],expected_rows=expected)])
    env=os.environ|dict(PYTHONPATH=str(ROOT/'clients/python/src'),ORCHIDDB_NATIVE_LIBRARY=str(ROOT/'target/debug/liborchiddb_compiler.dylib'),
        DYLD_LIBRARY_PATH=str(ROOT/'target/duckdb-download/aarch64-apple-darwin/1.5.2'),
        DUCKDB_LIB_DIR=str(ROOT/'target/duckdb-download/aarch64-apple-darwin/1.5.2'),
        JAVA_HOME='/opt/homebrew/opt/openjdk@21/libexec/openjdk.jdk/Contents/Home',
        CARGO_BUILD_JOBS='2',CGO_ENABLED='1',PATH='/opt/homebrew/opt/erlang/bin:'+os.environ['PATH'])
    env.pop('ORCHIDDB_STREAM_FIXTURE',None)
    checks={}
    with tempfile.TemporaryDirectory(prefix='orchid-weaviate-clients-') as folder:
        folder=pathlib.Path(folder);path=folder/'fixture.json';path.write_text(json.dumps(fixture));path.chmod(0o600);env['ORCHIDDB_REMOTE_FIXTURE']=str(path)
        commands=[
            ('python',[sys.executable,'-m','pytest','clients/python/tests/test_remote_engines.py','-q'],ROOT,3),
            ('js',['node','--test','test/remote-engines.test.mjs'],ROOT/'clients/js',3),
            ('go',['go','test','-ldflags=-linkmode=external','-run','TestLiveRemoteFederation','-count=1','-v'],ROOT/'clients/go/examples',3),
            ('java',['mvn','-pl','orchiddb-java','-am','-Dtest=RemoteEngineIntegrationTest','-Dsurefire.failIfNoSpecifiedTests=false','test'],ROOT/'clients/java',6),
            ('cpp',[str(ROOT/'clients/cpp/build/orchiddb_remote')],ROOT,9),
            ('elixir',['mix','test','test/remote_engine_test.exs'],ROOT/'clients/elixir',9),
            ('rust',['cargo','test','--locked','-p','orchiddb-client','--features','quickwit,elasticsearch,weaviate','--test','remote_engines','--','--nocapture'],ROOT,6),
        ]
        # Rebuild only changed test hosts; all native libraries are caller supplied.
        subprocess.run(['cmake','--build','clients/cpp/build','-j2'],cwd=ROOT,env=env,stdout=subprocess.DEVNULL,check=True)
        for client,command,cwd,expected_calls in commands:
            start=len(calls)
            with open('/tmp/orchid-weaviate-'+client+'.log','w') as log:
                result=subprocess.run(command,cwd=cwd,env=env,stdout=log,stderr=subprocess.STDOUT)
            if result.returncode: raise AssertionError(f'{client} failed; see /tmp/orchid-weaviate-{client}.log')
            queries=calls[start:]
            assert len(queries)==expected_calls,(client,len(queries),expected_calls)
            assert all(0<q.count('nearVector:')<=2 for q in queries),(client,queries)
            assert sum(q.count('nearVector:') for q in queries)==expected_calls//3*5,client
            checks[client]=dict(graphql_posts=len(queries),lookups=expected_calls//3*5,max_lookups_per_post=2)
            print(f'PASS {client}: batched Weaviate RAG edge, {len(queries)} HTTP POSTs',flush=True)
        schema={k:v for k,v in request.items() if k not in ('version','dialect','language','query')}
        (folder/'schema.json').write_text(json.dumps(schema));(folder/'init.sql').write_text(';\n'.join(fixture['cases'][0]['setup_sql']))
        (folder/'engines.json').write_text(json.dumps(dict(text=options)))
        start=len(calls)
        result=subprocess.run([str(ROOT/'target/debug/orchiddb'),'query',request['query'],'--schema',str(folder/'schema.json'),'--init',str(folder/'init.sql'),'--engines',str(folder/'engines.json'),'--no-iceberg'],env=env,capture_output=True,check=True)
        table=pa.ipc.open_stream(result.stdout).read_all()
        assert [[r['question'],r['document']] for r in table.to_pylist()]==expected
        assert len(calls)-start==3
        checks['cli']=dict(graphql_posts=3,lookups=5,max_lookups_per_post=2)
        print('PASS CLI: batched Weaviate RAG edge, 3 HTTP POSTs',flush=True)
    (ROOT/'artifacts/e2e/weaviate-all-clients.json').write_text(json.dumps(checks,indent=2)+'\n')
