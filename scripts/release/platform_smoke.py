#!/usr/bin/env python3
"""Run real native, CLI, and extension queries on each release target."""
import argparse
import ctypes
import json
from pathlib import Path
import subprocess
import tempfile


def verify(binaries, extension, version, commit):
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
    assert not hasattr(native, 'orchiddb_compile_json')
    assert subprocess.check_output([str(binaries / "orchiddb"), "--version"], text=True).strip() == "orchiddb " + version
    with tempfile.TemporaryDirectory() as temporary:
        schema = Path(temporary) / 'schema.json'
        schema.write_text('{"tables":[]}')
        output = subprocess.check_output([str(binaries / 'orchiddb'), 'query', 'RETURN 42 AS answer', '--schema', str(schema), '--format', 'table', '--no-iceberg'], text=True)
        assert '42' in output and 'answer' in output, output
    import duckdb
    with duckdb.connect(config={'allow_unsigned_extensions': 'true'}) as database:
        database.execute("LOAD '" + str(extension).replace("'", "''") + "'")
        database.execute('CALL orchid_register_schema(?, ?)', ['release', '{"tables":[]}'])
        assert database.execute("SELECT * FROM orchid_query('release', 'RETURN 42 AS answer')").fetchall() == [(42,)]
    print('Validated native ABI, CLI execution, and extension execution:', binaries.name)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binaries', type=Path, required=True)
    parser.add_argument('--extension', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--commit', required=True)
    args = parser.parse_args()
    verify(args.binaries.resolve(), args.extension.resolve(), args.version, args.commit)
