from contextlib import contextmanager
from threading import Lock
from typing import Protocol, ContextManager, Any
from ._runtime import _Runtime, CompiledQuery

class ArrowEngine(Protocol):
    dialect: str
    def query_arrow(self, query: CompiledQuery, batch_size: int = 65536) -> ContextManager[Any]:
        """Yield an Arrow RecordBatchReader; release only resources owned by the query."""
        ...

class DuckDBEngine:
    """Borrows the exact caller connection; never commits, rolls back, or closes it.

    Do not use the connection directly while this adapter has an open result.
    The lock prevents overlapping results through this adapter only.
    """
    dialect = "duckdb"
    def __init__(self, connection):
        self.connection = connection
        self._lease = Lock()

    @contextmanager
    def query_arrow(self, query: CompiledQuery, batch_size: int = 65536):
        if query.diagnostics.get("transfers"):
            raise ValueError("Use query_federated for a multi-engine plan")
        if query.version != 1 or query.dialect != self.dialect:
            raise ValueError("Compiled query dialect does not match engine")
        if batch_size <= 0:
            raise ValueError("batch_size must be positive")
        if not self._lease.acquire(blocking=False):
            raise RuntimeError("An Arrow reader is already active on this engine")
        reader = None
        try:
            # execute uses the original connection, preserving transactions and UDFs.
            reader = self.connection.execute(query.sql).to_arrow_reader(batch_size)
            yield reader
        finally:
            try:
                if reader is not None:
                    reader.close()
            finally:
                self._lease.release()

    @contextmanager
    def statistics_arrow(self, request):
        """Bounded statistics reader on the same transaction, with interruption."""
        from threading import Timer
        if not self._lease.acquire(blocking=False):
            raise RuntimeError("An Arrow reader is already active on this engine")
        reader = None
        timer = Timer(request["timeout_ms"] / 1000, self.connection.interrupt)
        timer.daemon = True
        try:
            timer.start()
            reader = self.connection.execute(request["sql"]).to_arrow_reader(min(4096, request["max_rows"]))
            yield reader
        finally:
            timer.cancel()
            timer.join()
            try:
                if reader is not None:
                    reader.close()
            finally:
                self._lease.release()

class Connection:
    """Retains a graph schema and executes on caller-owned engine connections."""
    def __init__(self, engine, schema=None, *, catalog=None, engines=None, library=None):
        import copy
        from .catalog import Catalog
        if catalog is not None:
            if schema is not None:
                raise TypeError("supply either schema or catalog")
            if not isinstance(catalog, Catalog):
                raise TypeError("catalog must be a Catalog")
            schema = catalog
        if schema is None:
            raise TypeError("schema or catalog is required")
        if isinstance(schema, Catalog):
            library = library or schema._library
            schema = schema._schema()
        self.engine = engine
        self._runtime = _Runtime(library)
        try:
            self._schema = self._runtime.operation_command(
                dict(op="validate_schema", schema=copy.deepcopy(schema)))
        except BaseException:
            self._runtime.close()
            raise
        self._engines = dict(engines or {})
        self._closed = False

    def _request(self, query, language, parameters, authorization):
        if self._closed:
            raise RuntimeError("Connection is closed")
        if not isinstance(query, str):
            raise TypeError("query must be text, separate from the registered schema")
        request = dict(self._schema, version=1, dialect=self.engine.dialect,
                       language=language, query=query, parameters=parameters or {})
        if authorization is not None:
            request["authorization"] = authorization.to_dict() if hasattr(authorization, "to_dict") else dict(authorization)
        return request

    def query(self, query: str, *, language="cypher", parameters=None, authorization=None, batch_size=65536):
        request = self._request(query, language, parameters, authorization)
        if self._engines:
            from .federation import _query_federated
            return _query_federated(self._runtime, request, self._engines, batch_size)
        return self.engine.query_arrow(self._runtime.prepare(request), batch_size)

    query_arrow = query

    def generate_statistics(self):
        return self._runtime.generate_statistics(self._request("RETURN 1", "cypher", None, None), self.engine)
    def clear_statistics(self): self._runtime.clear_statistics()
    def save_statistics(self, path): self._runtime.save_statistics(path)
    def load_statistics(self, path): self._runtime.load_statistics(path)
    def close(self):
        self._runtime.close()
        self._closed = True
    def __enter__(self):
        if self._closed: raise RuntimeError("Connection is closed")
        return self
    def __exit__(self, *args): self.close()

class PostgresEngine:
    """Borrow a psycopg 3 connection. The application owns transactions and TLS.

    PostgreSQL's wire protocol is converted to Arrow batches here. No database
    driver is imported until the application constructs its connection.
    """
    dialect = "postgres"

    def __init__(self, connection):
        self.connection = connection
        self._lease = Lock()

    @contextmanager
    def query_arrow(self, query: CompiledQuery, batch_size: int = 65536):
        import pyarrow as pa
        if query.version != 1 or query.dialect != self.dialect:
            raise ValueError("Compiled query protocol/dialect does not match engine")
        if query.diagnostics.get('transfers'):
            raise ValueError('Use query_federated for a multi-engine plan')
        if batch_size <= 0:
            raise ValueError('batch_size must be positive')
        if not self._lease.acquire(blocking=False):
            raise RuntimeError('An Arrow reader is already active on this engine')
        reader = None
        try:
            with _postgres_savepoint(self.connection), self.connection.cursor() as cursor:
                # Preserve exact decimal values inside PostgreSQL JSONB list cells.
                import json
                from decimal import Decimal
                from functools import partial
                from psycopg.types.json import set_json_loads
                set_json_loads(partial(json.loads, parse_float=Decimal), cursor)
                cursor.execute(query.sql)
                field_types = query.diagnostics.get('field_types', [])
                schema = pa.schema([pa.field(c.name, _logical_arrow_type(field_types[i], pa)
                    if c.type_code in (114, 3802, 199, 3807) and i < len(field_types) and field_types[i]
                    else _postgres_arrow_type(c, pa)) for i, c in enumerate(cursor.description)])
                def batches():
                    while rows := cursor.fetchmany(batch_size):
                        yield pa.RecordBatch.from_arrays([pa.array([_postgres_json_value(r[i], f.type, pa) if cursor.description[i].type_code in (114,3802,199,3807) else r[i] for r in rows], type=f.type) for i, f in enumerate(schema)], schema=schema)
                reader = pa.RecordBatchReader.from_batches(schema, batches())
                yield reader
        finally:
            try:
                if reader is not None:
                    reader.close()
            finally:
                self._lease.release()


def _postgres_arrow_type(column, pa):
    oid = column.type_code
    scalar = {16:pa.bool_(), 20:pa.int64(), 21:pa.int16(), 23:pa.int32(), 700:pa.float32(), 701:pa.float64(),
              25:pa.string(), 1042:pa.string(), 1043:pa.string(), 19:pa.string(), 17:pa.binary(),
              1082:pa.date32(), 1083:pa.time64('us'), 1114:pa.timestamp('us'), 1184:pa.timestamp('us',tz='UTC')}
    arrays = {1000:16,1005:21,1007:23,1016:20,1021:700,1022:701,1009:25,1015:1043,1001:17,1182:1082,1183:1083,1115:1114,1185:1184}
    if oid in scalar:
        return scalar[oid]
    if oid in arrays:
        return pa.list_(scalar[arrays[oid]])
    if oid in (1700, 1231):
        if column.precision is None:
            decimal_type = pa.decimal256(76, 38)
        else:
            precision = column.precision
            scale = column.scale or 0
            decimal_type = pa.decimal128(precision, scale) if precision <= 38 else pa.decimal256(precision, scale)
        return pa.list_(decimal_type) if oid == 1231 else decimal_type
    raise ValueError(f'Unsupported PostgreSQL Arrow type OID {oid}; cast it in a source view')


@contextmanager
def _postgres_savepoint(connection):
    """Recover adapter errors without aborting an application transaction."""
    if connection.autocommit:
        yield
        return
    from uuid import uuid4
    name = '__orchiddb_' + uuid4().hex
    connection.execute('SAVEPOINT ' + name)
    try:
        yield
    except BaseException:
        connection.execute('ROLLBACK TO SAVEPOINT ' + name)
        raise
    finally:
        connection.execute('RELEASE SAVEPOINT ' + name)


def _logical_arrow_type(name, pa):
    if name.startswith('list:'):
        return pa.list_(_logical_arrow_type(name[5:], pa))
    if name.startswith('struct_fields:'):
        import json
        return pa.struct([pa.field(field, _logical_arrow_type(kind, pa))
                          for field, kind in json.loads(name[len('struct_fields:'):])])
    if name.startswith('decimal:'):
        _, precision, scale = name.split(':')
        return pa.decimal128(int(precision), int(scale))
    if name == 'null': return pa.null()
    if name == 'time': return pa.time64('us')
    if name == 'boolean': return pa.bool_()
    if name == 'date': return pa.date32()
    if name == 'timestamp': return pa.timestamp('us')
    return pa.type_for_alias(name)


def _postgres_json_value(value, ty, pa):
    if value is None:
        return None
    if pa.types.is_list(ty):
        return [_postgres_json_value(v, ty.value_type, pa) for v in value]
    if pa.types.is_struct(ty):
        return {field.name: _postgres_json_value(value.get(field.name), field.type, pa)
                for field in ty}
    import datetime
    from decimal import Decimal
    if pa.types.is_integer(ty): return int(value)
    if pa.types.is_boolean(ty) and isinstance(value, str): return value.lower() == 'true'
    if pa.types.is_decimal(ty): return Decimal(value)
    if pa.types.is_floating(ty): return float(value)
    if pa.types.is_binary(ty) and isinstance(value, str):
        if value.startswith('\\x'): return bytes.fromhex(value[2:])
        import base64
        return base64.b64decode(value, validate=True)
    if pa.types.is_date(ty) and isinstance(value, str): return datetime.date.fromisoformat(value)
    if pa.types.is_time(ty) and isinstance(value, str): return datetime.time.fromisoformat(value)
    if pa.types.is_timestamp(ty) and isinstance(value, str): return datetime.datetime.fromisoformat(value)
    return value
