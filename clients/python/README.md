# OrchidDB for Python

Register schema once, then execute query text and parameters separately.

```sh
# From the repository root; build this checkout's matching native runtime.
make native
python -m pip install -e 'clients/python[arrow]' duckdb
python clients/python/examples/people.py
```

```python
import json
import duckdb
from orchiddb import Connection, DuckDBEngine

schema = json.load(open("cli/examples/people.json"))
with duckdb.connect() as database:
    database.execute(open("cli/examples/setup.sql").read())
    with Connection(DuckDBEngine(database), schema) as graph:
        with graph.query("MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
                         parameters={"name": "Ada"}) as reader:
            print(reader.read_all().to_pylist())  # [{'name': 'Ada'}]
```

`query`/`query_arrow` yields a PyArrow reader. Retained batches own their buffers.
Close a result before reusing the borrowed connection. OrchidDB never commits,
rolls back, or closes your database connection. Graph schema is copied at creation;
query fields in schema configuration are rejected. There is no `Compiler`,
`compile`, or `plan` public route.

Use `PostgresEngine(your_psycopg_connection)` for PostgreSQL. Drivers remain
application dependencies. Choose `language="gremlin"` or `language="sparql"`
on query calls; RDF mappings belong in schema configuration. Supply per-query
`authorization` for protected mappings.

Optional statistics: `graph.generate_statistics()`, `save_statistics(path)`,
`load_statistics(path)`, and `clear_statistics()`. Generation collects bounded
samples through the same session; the shared core computes the estimates.

For federation, pass named adapters as `engines={...}` when constructing the
connection; schema declares `engines`, table ownership, and `execution_engine`.
Use `RemoteEngine("quickwit", endpoint)` or `RemoteEngine("elasticsearch", endpoint)`
for remote search sessions. Close them explicitly. Queries still use `graph.query`.

[Source builds and API migration](../README.md) · [Runnable example](examples/people.py)
