"""Execute SQL and request islands using caller-owned sessions."""
from contextlib import contextmanager
from ._runtime import CompiledQuery


def _query(sql, dialect, columns):
    return CompiledQuery(sql, tuple(c["name"] for c in columns), dialect,
                         diagnostics={"field_types": [c["data_type"] for c in columns]})


@contextmanager
def _query_federated(compiler, request, engines, batch_size=65536):
    plan = compiler.prepare(request)
    target_id = plan.diagnostics.get("execution_engine")
    if target_id is None:
        raise ValueError("Federated execution requires execution_engine")

    def engine(name, dialect=None, method="query_arrow"):
        result = engines.get(name)
        if result is None or (dialect is not None and result.dialect != dialect):
            raise ValueError(f"Missing engine or dialect mismatch: {name}")
        if not callable(getattr(result, method, None)):
            raise ValueError(f"Engine {name} does not support {method}")
        return result

    target = engine(target_id, plan.dialect)
    # Validate every route before acquiring database result leases.
    for transfer in plan.diagnostics.get("transfers", []):
        operation = transfer.get("request") or transfer.get("operation")
        if not operation or transfer["sql"]:
            engine(transfer["source_engine"], transfer["source_dialect"])
        if operation:
            template = operation["template"]
            engine(operation["engine"], template.get("adapter", template.get("dialect")),
                   "execute_requests" if transfer.get("request") else "query_arrow")

    # Binding one exchange rewrites dependencies in the remaining transfers.
    while plan.diagnostics.get("transfers"):
        transfer = plan.diagnostics["transfers"][0]
        relation = transfer["target_relation"]
        operation = transfer.get("request") or transfer.get("operation")
        if operation:
            if transfer["sql"]:
                source = _query(transfer["sql"], transfer["source_dialect"], operation["input_columns"])
                with engine(transfer["source_engine"]).query_arrow(source, batch_size) as reader:
                    bound = compiler.bind_operation(plan, relation, reader=reader)
            else:
                bound = compiler.bind_operation(plan, relation, rows=[[]])
            if "requests" in bound:
                with engine(bound["engine"], method="execute_requests").execute_requests(
                        bound["requests"], transfer["columns"]) as reader:
                    plan = compiler.bind_arrow(plan, relation, reader)
            else:
                import pyarrow as pa
                batches, schema = [], None
                for sql in bound["sql"]:
                    query = _query(sql, bound["dialect"], transfer["columns"])
                    with engine(bound["engine"]).query_arrow(query, batch_size) as reader:
                        schema = reader.schema
                        batches.extend(reader)
                if schema is None:
                    plan = compiler.bind_rows(plan, relation, [])
                else:
                    with pa.RecordBatchReader.from_batches(schema, batches) as reader:
                        plan = compiler.bind_arrow(plan, relation, reader)
        else:
            source = _query(transfer["sql"], transfer["source_dialect"], transfer["columns"])
            with engine(transfer["source_engine"]).query_arrow(source, batch_size) as reader:
                plan = compiler.bind_arrow(plan, relation, reader)
    with target.query_arrow(plan, batch_size) as result:
        yield result
