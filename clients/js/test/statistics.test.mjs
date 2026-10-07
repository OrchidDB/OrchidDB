import test from 'node:test';
import assert from 'node:assert/strict';
import { tableFromArrays, tableFromIPC } from 'apache-arrow';
import { Compiler, asyncDuckDBEngine } from '../dist/index.js';

function compiler() {
  const c = Object.create(Compiler.prototype);
  c.catalogId = 'old'; c.statisticsSnapshot = {old: true};
  const calls = [];
  const task = {id:'scan', source:'people', kind:'sample', sql:'SELECT id FROM people LIMIT 2', dialect:'duckdb', max_rows:2, max_bytes:65536, timeout_ms:1000};
  c.statisticsCommand = command => {
    calls.push(command);
    if (command.op === 'begin') return {id:'analysis', request:task};
    if (command.op === 'submit') return {id:'analysis', request:command.done === false ? task : null};
    if (command.op === 'finish') return {catalog_id:'new', snapshot:{version:1}, report:{complete:true}};
    return {};
  };
  return {c, calls};
}

test('bounded driver submits Arrow, closes lease, and atomically replaces catalog', async () => {
  const {c, calls} = compiler(); let closed = false;
  const engine = {dialect:'duckdb', async executeStatistics() {
    return {async *[Symbol.asyncIterator]() {yield* tableFromArrays({id:[1n,2n,3n]}).batches;}, close() {closed = true;}};
  }};
  await c.generateStatistics({}, engine);
  const sample = calls.find(call => call.ipc);
  assert.deepEqual(tableFromIPC(Buffer.from(sample.ipc, 'base64')).getChild('id').toArray(), new BigInt64Array([1n,2n]));
  assert.equal(closed, true);
  assert.equal(c.catalogId, 'new');
  assert.deepEqual(calls.at(-1), {op:'release', catalog_id:'old'});
});

test('unbounded adapter is reported without executing fallback', async () => {
  const {c, calls} = compiler();
  await c.generateStatistics({}, {dialect:'duckdb', execute() {assert.fail('unsafe fallback');}});
  assert.match(calls.find(call => call.error).error, /bounded/);
});

test('fatal coordinator failure preserves previous catalog and cancels analysis', async () => {
  const {c, calls} = compiler(); const command = c.statisticsCommand;
  c.statisticsCommand = input => {if(input.op === 'finish') throw new Error('failed'); return command(input);};
  await assert.rejects(c.generateStatistics({}, {dialect:'duckdb'}), /failed/);
  assert.equal(c.catalogId, 'old');
  assert.deepEqual(calls.at(-1), {op:'cancel', id:'analysis'});
});

test('async DuckDB adapter interrupts acquisition and releases lease', async () => {
  let reject; let cancellations = 0;
  const engine = asyncDuckDBEngine({send: () => new Promise((_, r) => {reject = r;}), cancelSent() {cancellations++; reject?.(new Error('cancelled'));}});
  const abort = new AbortController();
  const pending = engine.executeStatistics({sql:'SELECT 1'}, abort.signal);
  abort.abort();
  await assert.rejects(pending, /cancelled/);
  assert.ok(cancellations >= 1);
  const again = engine.executeStatistics({sql:'SELECT 2'}, abort.signal);
  await assert.rejects(again, /abort/i);
});

test('async DuckDB adapter reads actual worker Arrow data in the caller transaction', async () => {
  const {openAsyncDatabase} = await import('./async-duckdb.mjs');
  const db = await openAsyncDatabase(); const connection = await db.connect();
  try {
    await connection.query('CREATE TABLE statistics_items(id BIGINT); BEGIN; INSERT INTO statistics_items VALUES (1), (2)');
    const engine = asyncDuckDBEngine(connection);
    const result = await engine.executeStatistics({sql:'SELECT id FROM statistics_items LIMIT 2'}, new AbortController().signal);
    const ids = [];
    try {for await(const batch of result) for(let i=0;i<batch.numRows;i++) ids.push(batch.getChild('id').get(i));}
    finally {await result.close();}
    assert.deepEqual(ids,[1n,2n]);
    await connection.query('ROLLBACK');
    assert.equal((await connection.query('SELECT count(*) n FROM statistics_items')).getChild('n').get(0),0n);
  } finally {await connection.close(); await db.terminate();}
});

test('async DuckDB deadline cancels a real scan and preserves the session', async () => {
  const {openAsyncDatabase} = await import('./async-duckdb.mjs');
  const db = await openAsyncDatabase(); const connection = await db.connect();
  const controller = new AbortController();
  try {
    const engine = asyncDuckDBEngine(connection);
    const timer = setTimeout(() => controller.abort(new Error('deadline')), 10);
    let result;
    try {
      await assert.rejects(async () => {
        result = await engine.executeStatistics({sql:'SELECT sum(i) FROM range(10000000000) t(i)'}, controller.signal);
        for await(const batch of result) void batch;
      });
    } finally {clearTimeout(timer); await result?.close();}
    assert.equal((await connection.query('SELECT 42 AS n')).getChild('n').get(0),42);
  } finally {await connection.close(); await db.terminate();}
});

test('explicit generation cancellation preserves previous snapshot and closes reader', async () => {
  const {c,calls} = compiler(); const controller = new AbortController(); let closed = false;
  const engine = {dialect:'duckdb', async executeStatistics() {
    return {async *[Symbol.asyncIterator]() {controller.abort(new Error('user cancelled')); yield tableFromArrays({id:[1n]}).batches[0];}, close() {closed = true;}};
  }};
  await assert.rejects(c.generateStatistics({}, engine, controller.signal), /user cancelled/);
  assert.equal(c.catalogId,'old');
  assert.equal(closed,true);
  assert.deepEqual(calls.at(-1), {op:'cancel',id:'analysis'});
});
