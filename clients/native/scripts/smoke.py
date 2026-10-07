#!/usr/bin/env python3
"""Real ABI smoke test; emits metadata, never source query/SQL contents."""
import ctypes
import json
import sys
from pathlib import Path


def load(path):
    lib = ctypes.CDLL(str(Path(path).resolve()))
    lib.orchiddb_abi_version.restype = ctypes.c_uint32
    lib.orchiddb_version.restype = ctypes.c_char_p
    lib.orchiddb_core_revision.restype = ctypes.c_char_p
    lib.orchiddb_compile_json.argtypes = [ctypes.c_char_p]
    lib.orchiddb_compile_json.restype = ctypes.c_void_p
    lib.orchiddb_string_free.argtypes = [ctypes.c_void_p]
    lib.orchiddb_string_free.restype = None
    return lib


def metadata(lib):
    return {"abi_version": lib.orchiddb_abi_version(),
            "version": lib.orchiddb_version().decode(),
            "core_revision": lib.orchiddb_core_revision().decode()}


def invoke(lib, request):
    output = lib.orchiddb_compile_json(request)
    if not output:
        raise RuntimeError("compiler returned null")
    try:
        return json.loads(ctypes.string_at(output))
    finally:
        lib.orchiddb_string_free(output)


def verify(path):
    lib = load(path)
    meta = metadata(lib)
    assert meta["abi_version"] == 1
    query = {"version": 1, "dialect": "duckdb", "language": "cypher",
             "query": "RETURN 42 AS answer", "tables": [], "nodes": []}
    result = invoke(lib, json.dumps(query).encode())
    assert result["ok"] is True, result
    assert result["result"]["fields"] == ["answer"]
    assert "42" in result["result"]["sql"]
    for invalid in [None, b"not JSON", b"\xff"]:
        assert invoke(lib, invalid)["ok"] is False
    lib.orchiddb_string_free(None)
    return meta


if __name__ == "__main__":
    print(json.dumps(verify(sys.argv[1]), sort_keys=True))
