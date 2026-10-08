import hashlib
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace

import pyarrow as pa
import pymysql

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'clients/python/src'))
from orchiddb.starrocks import StarRocksEngine


def logical_type(ty):
    if pa.types.is_list(ty):
        return 'list:' + logical_type(ty.value_type)
    if pa.types.is_decimal(ty):
        return f'decimal:{ty.precision}:{ty.scale}'
    if pa.types.is_date(ty):
        return 'date'
    if pa.types.is_timestamp(ty):
        return 'timestamp'
    return str(ty)


connection = pymysql.connect(host='127.0.0.1', port=int(os.environ.get('ORCHIDDB_TEST_STARROCKS_PORT', '19030')),
                             user='root', database=os.environ['ORCHIDDB_TEST_STARROCKS_DATABASE'],
                             autocommit=True, read_timeout=35, write_timeout=35)
def configure_session():
    with connection.cursor() as cursor:
        cursor.execute('SET query_timeout=30')
        cursor.execute('SET enable_recursive_cte=true')
        cursor.execute('SET recursive_cte_max_depth=1000')


configure_session()
engine = StarRocksEngine(connection)
print('ready', flush=True)
for line in sys.stdin:
    request = None
    try:
        request = json.loads(line)
        schema = pa.ipc.open_stream(bytes(request['schema_ipc'])).schema
        query = SimpleNamespace(version=1, dialect='starrocks', sql=request['sql'],
                                diagnostics={'field_types': [logical_type(f.type) for f in schema]})
        with engine.query_arrow(query) as reader:
            sink = pa.BufferOutputStream()
            with pa.ipc.new_stream(sink, schema) as writer:
                for batch in reader:
                    writer.write_batch(pa.RecordBatch.from_arrays(batch.columns, schema=schema))
            result = sink.getvalue().to_pybytes()
        print(json.dumps({'bytes': len(result)}), flush=True)
        sys.stdout.buffer.write(result)
        sys.stdout.buffer.flush()
    except Exception as error:
        if isinstance(request, dict) and isinstance(request.get('sql'), str):
            directory = Path(__file__).resolve().parents[2] / 'target/conformance/starrocks/sql-errors'
            directory.mkdir(parents=True, exist_ok=True)
            digest = hashlib.sha256(request['sql'].encode()).hexdigest()
            (directory / (digest + '.json')).write_text(json.dumps({'sql': request['sql'], 'error': str(error)}))
        print(json.dumps({'error': str(error)}), flush=True)
        if not connection.open:
            try:
                connection.ping(reconnect=True)
                configure_session()
            except pymysql.Error:
                pass
