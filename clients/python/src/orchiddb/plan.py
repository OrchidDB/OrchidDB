"""Internal source-first plan execution; no database driver is imported here."""
from contextlib import contextmanager, ExitStack
from ._runtime import CompiledQuery


@contextmanager
def execute_plan(plan, sources, functions, duckdb=None, batch_size=65536, decode=None):
    import pyarrow as pa
    from .execution import _logical_arrow_type
    if plan.get('version') != 1 or batch_size <= 0:
        raise ValueError('Unsupported plan version or invalid batch size')
    nodes = plan['nodes']
    if not 0 <= plan['result'] < len(nodes):
        raise ValueError('Invalid plan result')

    def expressions(expr, width):
        kind = expr['kind']
        if kind == 'column':
            if not 0 <= expr['index'] < width:
                raise ValueError('Local column out of bounds')
        elif kind in ('cast', 'graph_element'):
            if kind == 'graph_element' and decode is None:
                raise ValueError('Graph representation decoder is required')
            expressions(expr['input'], width)
            _logical_arrow_type(expr['data_type'], pa)
        elif kind == 'literal':
            with pa.ipc.open_stream(bytes(expr['ipc'])) as reader:
                table = reader.read_all()
                if table.num_rows != 1 or table.num_columns != 1:
                    raise ValueError('Invalid literal')
        elif kind == 'call':
            if not callable(functions.get(expr['function'])):
                raise ValueError('Missing local function ' + expr['function'])
            for arg in expr['arguments']:
                expressions(arg, width)
        else:
            raise ValueError('Unsupported local expression')

    # Validate the complete graph before opening any source or calling a UDF.
    schemas = []
    for i, node in enumerate(nodes):
        schemas.append(pa.schema([pa.field(c['name'], _logical_arrow_type(c['data_type'], pa), nullable=c['nullable']) for c in node['columns']]))
        kind = node['kind']
        deps = []
        if kind in ('source_sql', 'source_request'):
            source = sources.get(node['source'])
            dialect = node.get('dialect', node.get('template', {}).get('adapter'))
            method = 'query_arrow' if kind == 'source_sql' else 'execute_requests'
            if source is None or source.dialect != dialect or not callable(getattr(source, method, None)):
                raise ValueError('Missing source binding or capability: ' + node['source'])
            if node.get('input') is not None:
                deps.append(node['input'])
        elif kind == 'local_project':
            deps.append(node['input'])
        elif kind == 'duckdb':
            if plan['engine'] != 'duckdb' or duckdb is None or duckdb.dialect != 'duckdb' or not callable(getattr(duckdb, 'query_inputs', None)):
                raise ValueError('Plan requires a DuckDB query_inputs binding')
            deps.extend(r['node'] for r in node['inputs'])
        elif kind != 'input':
            raise ValueError('Unknown plan operation ' + kind)
        if any(not 0 <= dep < i for dep in deps):
            raise ValueError('Plan is not topologically ordered')
        if kind == 'local_project':
            if len(node['expressions']) != len(node['columns']):
                raise ValueError('Projection width mismatch')
            for expr in node['expressions']:
                expressions(expr, len(nodes[node['input']]['columns']))

    def evaluate(expr, batch):
        kind = expr['kind']
        if kind == 'column':
            return batch.column(expr['index'])
        if kind == 'cast':
            return evaluate(expr['input'], batch).cast(_logical_arrow_type(expr['data_type'], pa), safe=True)
        if kind == 'graph_element':
            values = evaluate(expr['input'], batch)
            encoded = pa.RecordBatch.from_arrays([values], names=['descriptor'])
            sink = pa.BufferOutputStream()
            with pa.ipc.new_stream(sink, encoded.schema) as writer:
                writer.write_batch(encoded)
            result = decode({'op':'decode_graph_values', 'data_type':expr['data_type'], 'ipc':list(sink.getvalue().to_pybytes())})
            if len(result) != batch.num_rows:
                raise ValueError('Invalid graph decoder row count')
            return pa.array(result, type=pa.struct([pa.field('__orchiddb_value_v1', pa.string())]))
        if kind == 'literal':
            with pa.ipc.open_stream(bytes(expr['ipc'])) as reader:
                value = reader.read_all().column(0)[0]
            return pa.repeat(value, batch.num_rows)
        result = functions[expr['function']]([evaluate(arg, batch) for arg in expr['arguments']], batch.num_rows)
        if len(result) != batch.num_rows or result.type != _logical_arrow_type(expr['returns'], pa):
            raise ValueError('Local UDF returned an invalid type or row count')
        return result

    def normalize(reader, schema):
        if len(reader.schema) != len(schema) or any(a.type != b.type for a, b in zip(reader.schema, schema)):
            raise ValueError('Source result schema mismatch')
        for batch in reader:
            if any(not field.nullable and column.null_count for field, column in zip(schema, batch.columns)):
                raise ValueError('NULL in non-nullable result column')
            yield pa.RecordBatch.from_arrays(batch.columns, schema=schema)

    with ExitStack() as stack:
        outputs = {}
        for i, node in enumerate(nodes):
            kind, schema = node['kind'], schemas[i]
            lease = ExitStack()
            stack.callback(lease.close)
            if kind == 'source_sql':
                work = CompiledQuery(node['sql'], tuple(schema.names), node['dialect'], diagnostics={'field_types': [c['data_type'] for c in node['columns']]})
                reader = lease.enter_context(sources[node['source']].query_arrow(work, batch_size))
                output = pa.RecordBatchReader.from_batches(schema, normalize(reader, schema))
            elif kind == 'input':
                batch = pa.RecordBatch.from_struct_array(pa.array([{}] if node['one_row'] else [], type=pa.struct([])))
                output = pa.RecordBatchReader.from_batches(schema, [batch])
            elif kind == 'local_project':
                def project(reader, exprs, schema):
                    for batch in reader:
                        yield pa.RecordBatch.from_arrays([evaluate(e, batch) for e in exprs], schema=schema)
                output = pa.RecordBatchReader.from_batches(schema, project(outputs[node['input']], node['expressions'], schema))
            elif kind == 'source_request':
                # Bind bounded batches, keeping the service adapter's correlation
                # protocol intact. Each request cursor closes before the next.
                def requests(node, schema):
                    batches = outputs[node['input']] if node.get('input') is not None else [None]
                    for batch in batches:
                        rows = [()] if batch is None else zip(*(c.to_pylist() for c in batch.columns))
                        pending = []
                        for row in rows:
                            pending.append(_bind_template(node['template'], row))
                            if len(pending) == batch_size:
                                with sources[node['source']].execute_requests(pending, node['columns']) as reader:
                                    yield from normalize(reader, schema)
                                pending = []
                        if pending:
                            with sources[node['source']].execute_requests(pending, node['columns']) as reader:
                                yield from normalize(reader, schema)
                output = pa.RecordBatchReader.from_batches(schema, requests(node, schema))
            else:
                work = CompiledQuery(node['sql'], tuple(schema.names), 'duckdb', diagnostics={'field_types': [c['data_type'] for c in node['columns']]})
                inputs = {r['name']: outputs[r['node']] for r in node['inputs']}
                reader = lease.enter_context(duckdb.query_inputs(work, inputs, batch_size))
                output = pa.RecordBatchReader.from_batches(schema, normalize(reader, schema))
            stack.callback(output.close)
            # Replay a non-reentrant source through disk, never an in-memory lake.
            later = nodes[i + 1:]
            reentry = kind in ('source_sql', 'source_request') and any(n.get('source') == node['source'] for n in later)
            reentry |= kind == 'duckdb' and any(n['kind'] == 'duckdb' for n in later)
            if reentry:
                import tempfile
                file = stack.enter_context(tempfile.TemporaryFile())
                with pa.ipc.new_file(file, schema) as writer:
                    for batch in output:
                        writer.write_batch(batch)
                output.close()
                lease.close()
                file.seek(0)
                replay = pa.ipc.open_file(file)
                output = pa.RecordBatchReader.from_batches(schema, (replay.get_batch(j) for j in range(replay.num_record_batches)))
                stack.callback(output.close)
            outputs[i] = output
        yield outputs[plan['result']]


def _bind_template(template, row):
    import copy
    if len(row) != template['parameters']:
        raise ValueError('Request parameter count mismatch')
    request = copy.deepcopy(template['request'])
    for binding in template.get('bindings', []):
        value = row[binding['parameter']]
        if binding.get('encoding') == 'sql_null':
            value = value is None
        pointer = binding['pointer']
        if not pointer:
            request = value
            continue
        parts = [p.replace('~1', '/').replace('~0', '~') for p in pointer[1:].split('/')]
        target = request
        for part in parts[:-1]:
            target = target[int(part)] if isinstance(target, list) else target[part]
        key = int(parts[-1]) if isinstance(target, list) else parts[-1]
        if isinstance(target, dict) and key not in target:
            raise ValueError('Unknown request binding pointer')
        target[key] = value
    return request
