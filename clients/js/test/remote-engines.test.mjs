import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {Compiler, RemoteEngine, queryFederated} from '../dist/internal.js';
import {openDatabase, arrowEngine} from '../examples/duckdb-wasm.mjs';

const path = process.env.ORCHIDDB_REMOTE_FIXTURE;
const cases = path ? JSON.parse(readFileSync(path, 'utf8')).cases : [];
for (const original of [...cases].filter(c => c.name?.endsWith('correlated-bm25'))) {
  const fixture = structuredClone(original);
  fixture.name = `${fixture.adapter}-large-integer-correlation`;
  fixture.setup_sql.push('UPDATE queries SET id=9007199254740993 WHERE id=10');
  fixture.request.query = fixture.request.query.replace('RETURN q.id,', 'RETURN toString(q.id),');
  fixture.expected_rows = original.expected_rows.map(row => [row[0] === 10 ? 9007199254740993n : BigInt(row[0]), ...row.slice(1)])
    .sort((a,b) => a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] - b[1])
    .map(row => [String(row[0]), ...row.slice(1)]);
  cases.push(fixture);
}
if (!path) test('live remote federation requires ORCHIDDB_REMOTE_FIXTURE', {skip: true}, () => {});
const plain = value => typeof value === 'bigint' && Number.isSafeInteger(Number(value)) ? Number(value) : value;
for (const fixture of cases) test(`live remote federation: ${fixture.name ?? fixture.adapter}`, async () => {
  const compiler = new Compiler();
  const db = await openDatabase();
  const connection = db.connect();
  let remote;
  try {
    for (const sql of fixture.setup_sql ?? []) connection.query(sql);
    remote = new RemoteEngine(compiler, fixture.adapter, {endpoint: fixture.endpoint, page_size: 2});
    const engines = new Map(Object.entries(fixture.request.engines).map(([id, descriptor]) =>
      [id, descriptor.dialect === fixture.adapter ? remote : arrowEngine(connection)]));
    const rows = await queryFederated(compiler, fixture.request, engines, async result => {
      const rows = [];
      for await (const batch of result) for (let i = 0; i < batch.numRows; i++)
        rows.push(batch.schema.fields.map((_, j) => plain(batch.getChildAt(j).get(i))));
      return rows;
    });
    assert.deepEqual(rows, fixture.expected_rows);
    remote.clearMetadataCache();
    assert.equal(connection.query('SELECT 42 AS n').getChild('n').get(0), 42);
    remote.close();
    assert.throws(() => remote.clearMetadataCache(), /closed/);
    remote.close();
  } finally {remote?.close(); compiler.close(); connection.close(); db.reset();}
});

for (const fixture of cases.filter(c => c.name?.endsWith('paged-scan'))) {
  test(`live remote error preserves caller sessions: ${fixture.adapter}`, async () => {
    const compiler = new Compiler();
    const db = await openDatabase(); const connection = db.connect();
    const remote = new RemoteEngine(compiler, fixture.adapter, {endpoint: fixture.endpoint});
    try {
      const old = fixture.request.tables.find(t => t.engine !== fixture.request.execution_engine).name;
      const request = JSON.parse(JSON.stringify(fixture.request), (_, v) => v === old ? old + '_missing' : v);
      const engines = new Map(Object.entries(request.engines).map(([id, descriptor]) =>
        [id, descriptor.dialect === fixture.adapter ? remote : arrowEngine(connection)]));
      await assert.rejects(queryFederated(compiler, request, engines, async () => {
        assert.fail('Missing index must raise, not return partial rows');
      }), /404|not found|does not exist|not_found/);
      assert.equal(connection.query('SELECT 42 AS n').getChild('n').get(0), 42);
      const count = await queryFederated(compiler, fixture.request, engines, async result => {
        let count = 0; for await (const batch of result) count += batch.numRows; return count;
      });
      assert.equal(count, fixture.expected_rows.length);
    } finally {remote.close(); compiler.close(); connection.close(); db.reset();}
  });
}
