import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import duckdb from '@duckdb/duckdb-wasm/blocking';
const require = createRequire(import.meta.url);
export const request = {
  version: 1, dialect: 'duckdb', language: 'cypher',
  query: 'MATCH (p:Person) RETURN p.id AS id, p.name AS name ORDER BY p.id',
  tables: [{name:'people', columns:[{name:'id',data_type:'int64'},{name:'name',data_type:'string'}]}],
  nodes: [{label:'Person',table:'people',id:'id',properties:{id:'id',name:'name'}}]
};
export async function openDatabase() {
  const db = await duckdb.createDuckDB({
    mvp: {mainModule:require.resolve('@duckdb/duckdb-wasm/dist/duckdb-mvp.wasm')},
    eh: {mainModule:require.resolve('@duckdb/duckdb-wasm/dist/duckdb-eh.wasm')}
  }, new duckdb.VoidLogger(), duckdb.NODE_RUNTIME);
  await db.instantiate();
  return db;
}
/** Borrow connection; closing a result cancels its stream, never closes the connection. */
export function arrowEngine(connection) {
  let active = false;
  return {
    id:'wasm-lake', dialect:'duckdb',
    async execute(plan) {
      if (active) throw new Error('Connection already has an active result');
      active = true;
      let reader;
      try { reader = await connection.send(plan.sql, true); }
      catch (error) { active = false; throw error; }
      let closed = false;
      return {
        schema: reader.schema,
        async *[Symbol.asyncIterator]() {
          if (closed) throw new Error('Result is closed');
          for (const batch of reader) yield batch;
        },
        close() {
          if (!closed) {
            closed = true;
            try { reader.cancel(); connection.cancelSent(); }
            finally { active = false; }
          }
        }
      };
    }
  };
}
export async function main() {
  const { Compiler, batches } = await import('@orchiddb/client');
  const db = await openDatabase();
  const connection = db.connect();
  try {
    connection.query("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'Orchid'), (2, NULL)");
    const compiler = new Compiler();
    const result = await compiler.query(request, arrowEngine(connection));
    const rows = [];
    for await (const batch of batches(result)) {
      for (let i = 0; i < batch.numRows; i++) rows.push([batch.getChild('id').get(i), batch.getChild('name').get(i)]);
    }
    assert.deepEqual(rows, [[1n, 'Orchid'], [2n, null]]);
    assert.equal(connection.query('SELECT 42 AS n').getChild('n').get(0), 42);
    console.log(rows);
  } finally { connection.close(); db.reset(); }
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) await main();
