"""Optional Quickwit/Elasticsearch/Weaviate sessions using OrchidDB's shared HTTP transport."""
from contextlib import contextmanager


class RemoteEngine:
    """A caller-owned remote session. Close it explicitly or use a with block.

    Requires the ``arrow`` extra. Options use the native HTTP session names,
    including authentication, page_size, max_rows, and request_timeout_ms.
    Calls are synchronous; use a worker thread in an async application.
    """
    def __init__(self, adapter, endpoint, *, library=None, **options):
        from ._runtime import _Runtime
        compiler = _Runtime(library)
        if adapter not in ("quickwit", "elasticsearch", "weaviate"):
            raise ValueError("RemoteEngine supports quickwit, elasticsearch, or weaviate")
        self._runtime = compiler
        self.dialect = adapter
        result = compiler.remote_command(dict(op="open", adapter=adapter,
                                               options=dict(options, endpoint=endpoint)))
        self._id = result["id"]

    def _command(self, op, **values):
        if self._id is None:
            raise RuntimeError("Remote engine is closed")
        return self._runtime.remote_command(dict(op=op, id=self._id, **values))

    @contextmanager
    def execute_requests(self, requests, columns):
        """Yield typed Arrow results, retaining int64 precision and JSON validity."""
        import base64
        import pyarrow as pa
        result = self._command("execute", requests=requests, columns=columns, format="ipc")
        with pa.ipc.open_stream(base64.b64decode(result["ipc"])) as reader:
            yield reader

    def clear_metadata_cache(self):
        self._command("clear_metadata_cache")

    def close(self):
        if self._id is not None:
            self._command("close")
            self._id = None

    def __enter__(self):
        if self._id is None:
            raise RuntimeError("Remote engine is closed")
        return self

    def __exit__(self, *args):
        self.close()
