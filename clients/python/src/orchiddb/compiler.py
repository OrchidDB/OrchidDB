import ctypes
import json
import os
import platform
from dataclasses import dataclass, field
from pathlib import Path
from typing import Mapping, Any

class CompilationError(ValueError):
    pass

@dataclass(frozen=True)
class CompiledQuery:
    sql: str
    fields: tuple[str, ...]
    dialect: str
    version: int = 1
    diagnostics: Mapping[str, Any] = field(default_factory=dict)

class Compiler:
    """Owns a library handle, never a database. Calls release native strings reliably."""
    def __init__(self, library: str | os.PathLike | None = None):
        name = {"Darwin": "liborchiddb_compiler.dylib", "Linux": "liborchiddb_compiler.so", "Windows": "orchiddb_compiler.dll"}.get(platform.system())
        path = library or os.environ.get("ORCHIDDB_NATIVE_LIBRARY") or Path(__file__).parent / "native" / str(name)
        self._lib = ctypes.CDLL(str(path))
        self._lib.orchiddb_abi_version.restype = ctypes.c_uint32
        if self._lib.orchiddb_abi_version() != 1:
            raise RuntimeError("Incompatible OrchidDB compiler ABI; expected 1")
        self._lib.orchiddb_compile_json.argtypes = [ctypes.c_char_p]
        self._lib.orchiddb_compile_json.restype = ctypes.c_void_p
        self._lib.orchiddb_statistics_json.argtypes = [ctypes.c_char_p]
        self._lib.orchiddb_statistics_json.restype = ctypes.c_void_p
        self._catalog_id = None
        self.statistics_snapshot = None
        self.statistics_report = None
        self._lib.orchiddb_string_free.argtypes = [ctypes.c_void_p]
        self._lib.orchiddb_string_free.restype = None
        self._lib.orchiddb_core_revision.restype = ctypes.c_char_p
        self.core_revision = self._lib.orchiddb_core_revision().decode()
        pin = Path(__file__).with_name("CORE_REVISION")
        expected = pin.read_text().strip() if pin.exists() else None
        explicit = library or os.environ.get("ORCHIDDB_NATIVE_LIBRARY")
        if expected and self.core_revision != expected and not (explicit and self.core_revision == expected + "-dirty"):
            raise RuntimeError("Compiler core revision does not match this client")

    def _call(self, function, request):
        payload = json.dumps(dict(request), ensure_ascii=False, allow_nan=False).encode("utf-8")
        pointer = function(payload)
        if not pointer:
            raise RuntimeError("Native compiler returned a null pointer")
        try:
            response = json.loads(ctypes.string_at(pointer))
        finally:
            self._lib.orchiddb_string_free(pointer)
        if not response["ok"]:
            raise CompilationError(response["error"])
        return response["result"]

    @staticmethod
    def _plan_dict(plan):
        return dict(plan.diagnostics, version=plan.version, sql=plan.sql,
                    dialect=plan.dialect, fields=list(plan.fields))

    @staticmethod
    def _compiled(result):
        return CompiledQuery(result["sql"], tuple(result["fields"]), result["dialect"],
                             result["version"], result)

    @staticmethod
    def _arrow_payload(reader):
        import base64
        import pyarrow as pa
        sink = pa.BufferOutputStream()
        with pa.ipc.new_stream(sink, reader.schema) as writer:
            for batch in reader:
                writer.write_batch(batch)
        return {"ipc": base64.b64encode(sink.getvalue()).decode("ascii")}

    def operation_command(self, command):
        """Execute a compiler protocol command whose result is not a query plan."""
        return self._call(self._lib.orchiddb_compile_json, command)

    def remote_command(self, command):
        """Use the shared optional HTTP transport; credentials stay out of plans."""
        try:
            function = self._lib.orchiddb_remote_json
        except AttributeError as error:
            raise RuntimeError("Native compiler does not include remote engine support") from error
        function.argtypes = [ctypes.c_char_p]
        function.restype = ctypes.c_void_p
        return self._call(function, command)

    def bind_arrow(self, plan, relation, reader):
        """Bind Arrow rows into a SELECT statement without database-side setup."""
        return self._compiled(self.operation_command(dict(
            op="bind", plan=self._plan_dict(plan), relation=relation,
            **self._arrow_payload(reader))))

    def bind_rows(self, plan, relation, rows):
        return self._compiled(self.operation_command(dict(
            op="bind", plan=self._plan_dict(plan), relation=relation, rows=rows)))

    def bind_operation(self, plan, relation, *, reader=None, rows=None):
        if (reader is None) == (rows is None):
            raise ValueError("Provide exactly one of reader or rows")
        data = self._arrow_payload(reader) if reader is not None else {"rows": rows}
        return self.operation_command(dict(op="bind_operation", plan=self._plan_dict(plan),
                                           relation=relation, **data))

    def statistics_command(self, command):
        """Shared collection protocol for application-owned database sessions."""
        return self._call(self._lib.orchiddb_statistics_json, command)

    def compile(self, request: Mapping[str, Any]) -> CompiledQuery:
        result = self.statistics_command(dict(op="compile", catalog_id=self._catalog_id, request=dict(request))) if self._catalog_id else self._call(self._lib.orchiddb_compile_json, request)
        if result["version"] != 1:
            raise RuntimeError("Unsupported compiled query protocol")
        return CompiledQuery(result["sql"], tuple(result["fields"]), result["dialect"], diagnostics={k: v for k, v in result.items() if k not in ("sql", "fields", "dialect", "version")})

    def _retain(self, result):
        old = self._catalog_id
        self._catalog_id = result["catalog_id"]
        self.statistics_snapshot = result.get("snapshot", self.statistics_snapshot)
        self.statistics_report = result.get("report")
        if old is not None:
            self.statistics_command(dict(op="release", catalog_id=old))

    def clear_statistics(self):
        if self._catalog_id is not None:
            self.statistics_command(dict(op="release", catalog_id=self._catalog_id))
        self._catalog_id = self.statistics_snapshot = self.statistics_report = None

    def save_statistics(self, path):
        if self.statistics_snapshot is None:
            raise ValueError("No statistics generated")
        Path(path).write_text(json.dumps(self.statistics_snapshot, allow_nan=False), encoding="utf-8")

    def load_statistics(self, path):
        snapshot = json.loads(Path(path).read_text(encoding="utf-8"))
        result = self.statistics_command(dict(op="install", snapshot=snapshot))
        self._retain(dict(result, snapshot=snapshot, report=snapshot.get("report")))

    def generate_statistics(self, request, engine):
        """Collect once through an existing bounded adapter, then retain the snapshot."""
        import base64
        import time
        import pyarrow as pa
        state = self.statistics_command(dict(op="begin", request=dict(request)))
        analysis = state["id"]
        try:
            while state.get("request") is not None:
                task = state["request"]
                submit = dict(op="submit", id=analysis, request_id=task["id"])
                try:
                    if task["dialect"] != engine.dialect:
                        raise ValueError("Statistics and engine dialects differ")
                    if not hasattr(engine, "statistics_arrow"):
                        raise ValueError("Adapter does not provide bounded statistics execution")
                    deadline = time.monotonic() + task["timeout_ms"] / 1000
                    rows, size = 0, 0
                    with engine.statistics_arrow(task) as reader:
                        for batch in reader:
                            if time.monotonic() >= deadline:
                                raise TimeoutError("Statistics request timed out")
                            offset = 0
                            while offset < batch.num_rows and rows < task["max_rows"]:
                                if time.monotonic() >= deadline:
                                    raise TimeoutError("Statistics request timed out")
                                count = min(batch.num_rows - offset, task["max_rows"] - rows)
                                limit = min(1024 * 1024, task["max_bytes"] - size)
                                while True:
                                    piece = batch.slice(offset, count)
                                    sink = pa.BufferOutputStream()
                                    with pa.ipc.new_stream(sink, piece.schema) as writer:
                                        writer.write_batch(piece)
                                    payload = sink.getvalue().to_pybytes()
                                    if len(payload) <= limit:
                                        break
                                    if count <= 1:
                                        raise ValueError("Statistics transport byte budget reached")
                                    count = max(1, count // 2)
                                self.statistics_command(dict(submit, ipc=base64.b64encode(payload).decode("ascii"), done=False))
                                rows += count
                                offset += count
                                size += len(payload)
                            if rows >= task["max_rows"]:
                                break
                    state = self.statistics_command(dict(submit, rows=[]))
                except Exception as error:
                    state = self.statistics_command(dict(submit, error=str(error)))
            result = self.statistics_command(dict(op="finish", id=analysis))
            self._retain(result)
            return result
        except BaseException:
            self.statistics_command(dict(op="cancel", id=analysis))
            raise

    def close(self):
        self.clear_statistics()

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        self.close()
