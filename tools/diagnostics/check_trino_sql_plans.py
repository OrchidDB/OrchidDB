"""Build a Trino validation script from trino_sql_plans output.

Usage: python3 tools/diagnostics/check_trino_sql_plans.py target/trino-sql-after.sql
Run the emitted SQL with the Trino CLI. Every differences column must be zero.
Fixtures are query-local and include duplicate edges, nulls, and orphan endpoints.
"""
import pathlib
import sys

FIXTURE = """people(id,name) AS (VALUES
    (BIGINT '1','Ada'),(BIGINT '2','Bob'),(BIGINT '3',CAST(NULL AS VARCHAR)),
    (BIGINT '4','Bob'),(BIGINT '5','lonely'),(BIGINT '42','Ada'),
    (BIGINT '6','é'),(BIGINT '7','é')),
links(id,src,dst) AS (VALUES
    (BIGINT '10',BIGINT '1',BIGINT '2'),(BIGINT '11',BIGINT '1',BIGINT '2'),
    (BIGINT '12',BIGINT '2',BIGINT '3'),(BIGINT '13',BIGINT '3',BIGINT '1'),
    (BIGINT '14',BIGINT '1',BIGINT '99'),(BIGINT '15',BIGINT '77',BIGINT '2'),
    (BIGINT '16',CAST(NULL AS BIGINT),BIGINT '2'),
    (BIGINT '17',BIGINT '2',CAST(NULL AS BIGINT)),(BIGINT '18',BIGINT '2',BIGINT '2'))"""

EXPECTED = {
    "MATCH(p:Person) RETURN p.name": 'SELECT name FROM people',
    "MATCH(p:Person) RETURN count(p)": 'SELECT count(*) FROM people',
    "MATCH(p:Person) WHERE p.id=42 RETURN p.name LIMIT 10": 'SELECT name FROM people WHERE id=42 LIMIT 10',
    "MATCH(a:Person)-[:LINK]->(b:Person) RETURN a.name,b.name":
        'SELECT a.name,b.name FROM people a JOIN links e ON a.id=e.src JOIN people b ON b.id=e.dst',
    "MATCH(a:Person) OPTIONAL MATCH(a)-[:LINK]->(b:Person) RETURN a.name,b.name":
        'SELECT a.name,m.name FROM people a LEFT JOIN (SELECT e.src,b.name FROM links e JOIN people b ON b.id=e.dst) m ON a.id=m.src',
    "MATCH(a:Person)-[:LINK*1..2]->(b:Person) RETURN a.id,b.id":
        '''SELECT a.id,b.id FROM people a JOIN links e ON a.id=e.src JOIN people b ON b.id=e.dst
        UNION ALL SELECT a.id,b.id FROM people a JOIN links e ON a.id=e.src
        JOIN people middle ON middle.id=e.dst JOIN links f ON f.src=middle.id AND f.id<>e.id
        JOIN people b ON b.id=f.dst''',
    "MATCH(p:Person) RETURN p.name,count(p) AS n ORDER BY n DESC":
        'SELECT name,count(id) FROM people GROUP BY name',
    "MATCH(p:Person) RETURN DISTINCT p.name": 'SELECT DISTINCT name FROM people',
    "MATCH(p:Person) RETURN DISTINCT p.name ORDER BY p.name LIMIT 5":
        'SELECT DISTINCT name FROM people ORDER BY name NULLS LAST LIMIT 5',
}

def validation(text):
    seen = set()
    for section in text.split('\n-- ')[1:]:
        query, sql = section.split('\n', 1)
        if query not in EXPECTED:
            continue
        if '-- ERROR:' in sql:
            raise ValueError(f'{query}: {sql}')
        seen.add(query)
        sql = sql.strip().removesuffix(';')
        prefix = 'WITH '
        for marker in ('WITH RECURSIVE ', 'WITH '):
            if sql.startswith(marker):
                prefix = marker
                sql = sql[len(marker):]
                break
        # Existing generated CTEs must precede the actual-result CTE.
        if prefix == 'WITH RECURSIVE ' or sql.startswith('"__w_sql_cte_'):
            sql = prefix + FIXTURE + ', ' + sql
        else:
            sql = prefix + FIXTURE + ' ' + sql
        expected = EXPECTED[query]
        # Nest each complete query so its WITH/ORDER BY/LIMIT scope is intact.
        yield f'''WITH actual AS ({sql}), expected AS (WITH {FIXTURE} {expected})
SELECT '{query.replace("'", "''")}' AS test, count(*) AS differences FROM (
  (SELECT * FROM actual EXCEPT ALL SELECT * FROM expected)
  UNION ALL (SELECT * FROM expected EXCEPT ALL SELECT * FROM actual)
) differences;'''
    missing = set(EXPECTED) - seen
    if missing:
        raise ValueError(f'Missing probes: {sorted(missing)}')

if __name__ == '__main__':
    print('\n\n'.join(validation(pathlib.Path(sys.argv[1]).read_text())))
