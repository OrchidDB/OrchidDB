"""Trino execution using a caller-owned trino.dbapi connection."""
from contextlib import contextmanager
from threading import Lock

from .execution import _logical_arrow_type, _postgres_json_value


class TrinoEngine:
    dialect = "trino"

    def __init__(self, connection):
        self.connection = connection
        self._lease = Lock()

    @contextmanager
    def query_arrow(self, query, batch_size=65536):
        import pyarrow as pa
        if query.version != 1 or query.dialect != self.dialect:
            raise ValueError("Query protocol/dialect does not match engine")
        if query.diagnostics.get("transfers"):
            raise ValueError("Use a federated connection for a multi-engine query")
        if batch_size <= 0:
            raise ValueError("batch_size must be positive")
        if not self._lease.acquire(blocking=False):
            raise RuntimeError("An Arrow reader is already active on this engine")
        reader = cursor = None
        try:
            cursor = self.connection.cursor()
            cursor.execute(query.sql)
            types = query.diagnostics.get("field_types", [])
            if len(types) != len(cursor.description) or any(not t for t in types):
                raise ValueError("Trino results require complete logical field types")
            schema = pa.schema([pa.field(c[0], _logical_arrow_type(t, pa))
                                for c, t in zip(cursor.description, types)])

            def value(cell, ty):
                if cell is None:
                    return None
                if pa.types.is_list(ty):
                    return [value(item, ty.value_type) for item in cell]
                if pa.types.is_struct(ty):
                    return {field.name: value(cell.get(field.name) if isinstance(cell, dict) else cell[i], field.type)
                            for i, field in enumerate(ty)}
                return _postgres_json_value(cell, ty, pa)

            def batches():
                while rows := cursor.fetchmany(batch_size):
                    yield pa.RecordBatch.from_arrays([
                        pa.array([value(row[i], field.type) for row in rows], type=field.type)
                        for i, field in enumerate(schema)], schema=schema)

            reader = pa.RecordBatchReader.from_batches(schema, batches())
            yield reader
        finally:
            try:
                if reader is not None:
                    reader.close()
            finally:
                try:
                    if cursor is not None:
                        cursor.close()
                finally:
                    self._lease.release()
