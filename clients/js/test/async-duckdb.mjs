import { Worker } from 'node:worker_threads';
import { createRequire } from 'node:module';
import * as duckdb from '@duckdb/duckdb-wasm';
const require = createRequire(import.meta.url);

export async function openAsyncDatabase() {
  const worker = new Worker(`
    const { parentPort, workerData } = require('node:worker_threads');
    globalThis.postMessage = (value, transfer) => parentPort.postMessage(value, transfer);
    require(workerData);
    parentPort.on('message', data => globalThis.onmessage({data}));
  `, {eval:true, execArgv:[], workerData:require.resolve('@duckdb/duckdb-wasm/dist/duckdb-node-eh.worker.cjs')});
  const handlers = new Map();
  const adapter = {
    postMessage(value, transfer) { worker.postMessage(value, transfer); },
    addEventListener(event, callback) {
      const fn = event === 'message' ? data => callback({data}) : callback;
      handlers.set(callback, fn); worker.on(event === 'close' ? 'exit' : event, fn);
    },
    removeEventListener(event, callback) {worker.off(event === 'close' ? 'exit' : event, handlers.get(callback));},
    terminate() {return worker.terminate();},
  };
  const db = new duckdb.AsyncDuckDB(new duckdb.VoidLogger(), adapter);
  try { await db.instantiate(require.resolve('@duckdb/duckdb-wasm/dist/duckdb-eh.wasm')); }
  catch (error) {await worker.terminate(); throw error;}
  return db;
}
