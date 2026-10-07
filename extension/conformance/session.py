"""Fixture transport for tests; registers schema before executing query text."""
import json


def prepare(db, request, update=False):
    if isinstance(request, str):
        request = json.loads(request)
    schema = {k: v for k, v in request.items()
              if k not in ('version', 'dialect', 'language', 'query', 'parameters', 'bindings', 'authorization')}
    if request.get('dialect', 'duckdb') != 'duckdb':
        schema['engines'] = {'invalid': {'dialect': request['dialect']}}
    db.execute('CALL orchid_register_schema(?, ?)', ['fixture', json.dumps(schema)]).fetchall()
    values = ['fixture', request['query']]
    sql = 'orchid_sparql_update(?, ?' if update else 'orchid_query(?, ?'
    if not update:
        sql += ', language := ?'
        values.append(request.get('language', 'cypher'))
        for key in ('parameters', 'bindings'):
            if request.get(key):
                sql += ', ' + key + ' := ?'
                values.append(request[key])
    return sql + ')', values


def execute(db, sql, request):
    function, values = prepare(db, request, 'orchid_sparql_update' in sql)
    return db.execute(sql.replace('orchid_query(?)', function).replace('orchid_sparql_update(?)', function), values)


def describe(db, request):
    rows = execute(db, 'DESCRIBE SELECT * FROM orchid_query(?)', request).fetchall()
    names = [row[0] for row in rows]
    fields = [name for name in names if not name.startswith('__rdf:') and name != '__orchid_void']
    form = 'Boolean' if len(rows) == 1 and rows[0][1] == 'BOOLEAN' else 'RdfGraph' if fields == ['subject', 'predicate', 'object'] else 'RowSet'
    return {'fields': fields, 'result_form': form}
