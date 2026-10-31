#!/usr/bin/env python3
"""Run real native ABI checks and extension queries on each release target."""
import argparse
import ctypes
import json
from pathlib import Path


def verify(binaries, extension, version, commit, export_host_symbols=False):
    library = next(binaries.glob('liborchiddb_compiler.*'))
    native = ctypes.CDLL(str(library))
    native.orchiddb_abi_version.restype = ctypes.c_uint32
    native.orchiddb_version.restype = ctypes.c_char_p
    native.orchiddb_core_revision.restype = ctypes.c_char_p
    assert native.orchiddb_abi_version() == 2
    assert native.orchiddb_version().decode() == version
    assert native.orchiddb_core_revision().decode() == commit
    native.orchiddb_execution_command.argtypes = [ctypes.c_char_p]
    native.orchiddb_execution_command.restype = ctypes.c_void_p
    native.orchiddb_string_free.argtypes = [ctypes.c_void_p]
    pointer = native.orchiddb_execution_command(b'{"op":"validate_schema","schema":{"tables":[]}}')
    assert pointer
    try:
        assert json.loads(ctypes.string_at(pointer)) == {'ok': True, 'result': {'tables': []}}
    finally:
        native.orchiddb_string_free(pointer)
    def command(value):
        pointer = native.orchiddb_execution_command(json.dumps(value).encode())
        assert pointer
        try:
            response = json.loads(ctypes.string_at(pointer))
            assert response['ok'], response
            return response['result']
        finally:
            native.orchiddb_string_free(pointer)
    # Every shipped native runtime must support host-bound OPA policies.
    handle = command({'op': 'opa_create', 'config': {
        'policy_uri': 'http://127.0.0.1:1/allow', 'user': 'release-smoke'}})
    assert isinstance(handle, int) and handle > 0
    command({'op': 'opa_close', 'handle': handle})
    assert not hasattr(native, 'orchiddb_compile_json')
    import duckdb
    if export_host_symbols:
        # Promote the already loaded host; never inject a second DuckDB.
        import _duckdb
        ctypes.CDLL(_duckdb.__file__, mode=ctypes.RTLD_GLOBAL)
    with duckdb.connect(config={'allow_unsigned_extensions': 'true'}) as database:
        database.execute("LOAD '" + str(extension).replace("'", "''") + "'")
        assert database.execute("SELECT __orchiddb_cypher_to_float_v1('42')").fetchall() == [(42,)]
    print('Validated native ABI and extension execution:', binaries.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--extension', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--export-host-symbols', action='store_true', help='Expose the existing Python DuckDB C++ symbols for Linux CPP extensions')
    args = parser.parse_args()
    verify(args.binaries.resolve(), args.extension.resolve(), args.version, args.commit, args.export_host_symbols)
