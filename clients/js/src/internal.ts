import koffi from 'koffi';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { RecordBatch, Schema } from 'apache-arrow';

export type Dialect = 'duckdb' | 'postgres' | 'starrocks';
export type EngineDialect = Dialect | 'quickwit' | 'elasticsearch' | (string & {});
export type Language = 'cypher' | 'gremlin' | 'sparql';
export interface Column { name: string; data_type: string; nullable?: boolean }
export interface Table { engine?: string; name: string; columns: readonly Column[] }
export interface Authorization { subject_type: string; subject_id: string }
export interface PermissionRelation {
  table: string; resource_type: string; permission: string;
  resource_type_column: string; permission_column: string; resource_id_column: string;
  subject_type_column: string; subject_relation_column: string; subject_id_column: string;
}
export interface PermissionScope { resource_column: string; relation: PermissionRelation }
export function permissionRelation(table: string, resource_type: string, permission: string,
  columns: Partial<Omit<PermissionRelation, 'table' | 'resource_type' | 'permission'>> = {}): PermissionRelation {
  return Object.freeze({table, resource_type, permission,
    resource_type_column: columns.resource_type_column ?? 'resource_type',
    permission_column: columns.permission_column ?? 'resource_rel',
    resource_id_column: columns.resource_id_column ?? 'resource_id',
    subject_type_column: columns.subject_type_column ?? 'subject_type',
    subject_relation_column: columns.subject_relation_column ?? 'subject_rel',
    subject_id_column: columns.subject_id_column ?? 'subject_id'});
}
export function permissionScope(resource_column: string, relation: PermissionRelation): PermissionScope {
  return Object.freeze({resource_column, relation});
}
export function authorization(subject_type: string, subject_id: string): Authorization {
  return Object.freeze({subject_type, subject_id});
}
export interface NodeMapping {
  label: string; table: string; id: string; properties?: Readonly<Record<string, string>>;
  permission_scopes?: readonly PermissionScope[];
}
export interface EdgeMapping extends NodeMapping { source: string; target: string; source_label: string; target_label: string }
export interface FunctionSignature { name: string; target: string; parameters: readonly string[]; returns: string; aggregate?: boolean }
export interface Ontology {
  classes?: readonly { iri: string; label: string; identity?: string }[];
  properties?: readonly { iri: string; label: string; property: string }[];
  relationships?: readonly { iri: string; label: string; source_label: string; target_label: string }[];
}
export type RdfTermMapping =
  | { kind: 'iri'; column: string }
  | { kind: 'template'; prefix: string; columns: readonly string[] }
  | { kind: 'blank'; scope: string; columns: readonly string[] }
  | { kind: 'literal'; column: string; datatype?: string; language?: string; language_column?: string }
  | { kind: 'constant'; value: string; datatype?: string; language?: string };
export interface RdfMapping {
  table: string; subject: RdfTermMapping; predicate: RdfTermMapping; object: RdfTermMapping;
  graph?: RdfTermMapping; dataset?: string; key?: readonly string[]; writable?: boolean;
}
/** Shared compilation metadata and optional generated statistics. */
export interface CompileRequest {
  version: 1; dialect: Dialect; language: Language; query: string;
  engines?: Readonly<Record<string, {dialect: EngineDialect}>>; execution_engine?: string;
  parameters?: Readonly<Record<string, unknown>>;
  authorization?: Authorization;
  tables: readonly Table[]; nodes?: readonly NodeMapping[]; edges?: readonly EdgeMapping[];
  functions?: readonly FunctionSignature[]; ontology?: Ontology;
  rdf?: readonly RdfMapping[]; dataset?: string;
  logical_sources?: readonly unknown[]; collection_sources?: readonly unknown[];
  representation_sources?: readonly unknown[]; statistics?: unknown;
  source_metadata?: readonly unknown[]; computed_relationships?: readonly unknown[]; search_indexes?: readonly unknown[];
}
export interface CompiledQuery { version: 1; dialect: Dialect; sql: string; fields: readonly string[]; readonly [diagnostic: string]: unknown }

/** A caller-owned lease. close() releases result resources, never the parent engine. */
export interface ArrowResult extends AsyncIterable<RecordBatch> {
  readonly schema: Schema;
  close(): void | Promise<void>;
}
/** No driver dependency: implement with an Arrow-capable database/ADBC/Flight client. */
export interface ExecutionEngine {
  readonly id: string;
  readonly dialect: Dialect;
  execute(query: CompiledQuery): Promise<ArrowResult>;
  /** Enforce deadline and cancellation during acquisition and reads. */
  executeStatistics?(task: StatisticsRequest, signal: AbortSignal): Promise<ArrowResult>;
}

export interface StatisticsRequest {
  id: string; source: string; kind: string; sql: string; dialect: Dialect;
  max_rows: number; max_bytes: number; timeout_ms: number;
}
export interface StatisticsResult { snapshot: unknown; catalog_id: string; report: unknown }

export class Compiler {
  private readonly library: ReturnType<typeof koffi.load>;
  private readonly compileNative: ReturnType<ReturnType<typeof koffi.load>['func']>;
  private readonly freeNative: ReturnType<ReturnType<typeof koffi.load>['func']>;
  private readonly statisticsNative: ReturnType<ReturnType<typeof koffi.load>['func']>;
  private catalogId?: string;
  statisticsSnapshot?: unknown;
  statisticsReport?: unknown;
  readonly coreRevision: string;
  readonly version: string;
  constructor(path?: string) {
    const explicit = path ?? process.env.ORCHIDDB_NATIVE_LIBRARY;
    path = explicit ?? defaultLibrary();
    const manifest = explicit ? undefined : JSON.parse(readFileSync(join(dirname(path), 'manifest.json'), 'utf8'));
    if (manifest && createHash('sha256').update(readFileSync(path)).digest('hex') !== manifest.sha256) throw new Error('Native compiler checksum mismatch');
    this.library = koffi.load(path);
    const abi = this.library.func('uint32_t orchiddb_abi_version(void)')();
    if (abi !== 2) { this.library.unload(); throw new Error(`Unsupported OrchidDB ABI ${abi}`); }
    this.compileNative = this.library.func('void *orchiddb_execution_command(const char *request)');
    this.statisticsNative = this.library.func('void *orchiddb_statistics_json(const char *request)');
    this.freeNative = this.library.func('void orchiddb_string_free(void *value)');
    this.coreRevision = this.library.func('const char *orchiddb_core_revision(void)')();
    this.version = this.library.func('const char *orchiddb_version(void)')();
    if (manifest && (manifest.abi_version !== abi || manifest.version !== this.version || manifest.core_revision !== this.coreRevision)) {
      this.library.unload(); throw new Error('Native compiler manifest mismatch');
    }
  }
  /** Synchronous CPU work. Use a worker thread for latency-sensitive Node servers. */
  private callNative(fn: typeof this.compileNative, request: unknown, allowUnsigned = false): any {
    const encoded = encodeRequest(request, new Set(), allowUnsigned);
    if (!encoded || encoded.includes('\u0000')) throw new Error('Request cannot contain NUL');
    const pointer = fn(encoded);
    if (!pointer) throw new Error('Native compiler returned a null response');
    let response: any;
    try { response = parseResponse(koffi.decode(pointer, 'char', -1) as string); }
    finally { this.freeNative(pointer); }
    if (response?.ok !== true) throw new Error(response?.error ?? 'Invalid compiler response');
    return response.result;
  }
  /** Compiler protocol operations may return request batches instead of query plans. */
  operationCommand(command: unknown): any { return this.callNative(this.compileNative, command, true); }
  remoteCommand(command: unknown): any {
    const fn = this.library.func('void *orchiddb_remote_json(const char *request)');
    return this.callNative(fn, command, true);
  }
  private async arrowPayload(source: ArrowResult): Promise<{ipc: string} | {rows: never[]}> {
    const arrow = await import('apache-arrow');
    const collected: RecordBatch[] = [];
    for await (const batch of source) collected.push(localArrowBatch(batch, arrow));
    return collected.length
      ? {ipc: Buffer.from(arrow.tableToIPC(new arrow.Table(collected), 'stream')).toString('base64')}
      : {rows: []};
  }
  async bindArrow(plan: CompiledQuery, relation: string, source: ArrowResult): Promise<CompiledQuery> {
    return this.operationCommand({op: 'bind', plan, relation, ...await this.arrowPayload(source)});
  }
  async bindOperationArrow(plan: CompiledQuery, relation: string, source: ArrowResult): Promise<any> {
    return this.operationCommand({op: 'bind_operation', plan, relation, ...await this.arrowPayload(source)});
  }
  statisticsCommand(command: unknown): any { return this.callNative(this.statisticsNative, command); }
  compile(request: CompileRequest): CompiledQuery {
    const result = this.catalogId
      ? this.statisticsCommand({ op: 'compile', catalog_id: this.catalogId, request })
      : this.callNative(this.compileNative, request);
    if (result?.version !== 1 || result?.dialect !== request.dialect || typeof result?.sql !== 'string' ||
        !Array.isArray(result?.fields) || !result.fields.every((f: unknown) => typeof f === 'string')) {
      throw new Error('Invalid compiled query response');
    }
    return Object.freeze({ ...result, fields: Object.freeze([...result.fields]) });
  }
  private retain(result: StatisticsResult): void {
    const old = this.catalogId;
    this.catalogId = result.catalog_id;
    this.statisticsSnapshot = result.snapshot;
    this.statisticsReport = result.report;
    if (old) this.statisticsCommand({ op: 'release', catalog_id: old });
  }
  clearStatistics(): void {
    if (this.catalogId) this.statisticsCommand({ op: 'release', catalog_id: this.catalogId });
    this.catalogId = undefined;
    this.statisticsSnapshot = this.statisticsReport = undefined;
  }
  saveStatistics(path: string): void {
    if (!this.catalogId) throw new Error('No statistics generated');
    writeFileSync(path, encodeRequest(this.statisticsSnapshot));
  }
  loadStatistics(path: string): void {
    const snapshot = JSON.parse(readFileSync(path, 'utf8'));
    const result = this.statisticsCommand({ op: 'install', snapshot });
    this.retain({ ...result, snapshot, report: snapshot.report });
  }
  close(): void { this.clearStatistics(); }
  async generateStatistics(request: CompileRequest, engine: ExecutionEngine, signal?: AbortSignal): Promise<StatisticsResult> {
    signal?.throwIfAborted();
    const arrow = await import('apache-arrow');
    const { Table, tableToIPC } = arrow;
    let state = this.statisticsCommand({ op: 'begin', request });
    const id = state.id;
    try {
      while (state.request) {
        const task: StatisticsRequest = state.request;
        const submit = { op: 'submit', id, request_id: task.id };
        signal?.throwIfAborted();
        const controller = new AbortController();
        const requestSignal = signal ? AbortSignal.any([signal, controller.signal]) : controller.signal;
        const timeout = setTimeout(() => controller.abort(new Error('Statistics request timed out')), task.timeout_ms);
        let result: ArrowResult | undefined;
        try {
          if (task.dialect !== engine.dialect) throw new Error('Statistics and engine dialects differ');
          if (!engine.executeStatistics) throw new Error('Adapter does not provide bounded statistics execution');
          result = await engine.executeStatistics(task, requestSignal);
          let rows = 0, bytes = 0;
          for await (let batch of result) {
            requestSignal.throwIfAborted();
            batch = localArrowBatch(batch, arrow);
            let offset = 0;
            while (offset < batch.numRows && rows < task.max_rows) {
              requestSignal.throwIfAborted();
              let count = Math.min(batch.numRows - offset, task.max_rows - rows);
              const limit = Math.min(1024 * 1024, task.max_bytes - bytes);
              let ipc: Uint8Array;
              while (true) {
                ipc = tableToIPC(new Table(batch.slice(offset, offset + count)), 'stream');
                if (ipc.byteLength <= limit) break;
                if (count <= 1) throw new Error('Statistics transport byte budget reached');
                count = Math.max(1, Math.floor(count / 2));
              }
              this.statisticsCommand({ ...submit, ipc: Buffer.from(ipc).toString('base64'), done: false });
              rows += count; offset += count; bytes += ipc.byteLength;
            }
            if (rows >= task.max_rows) break;
          }
          await result.close(); result = undefined;
          state = this.statisticsCommand({ ...submit, rows: [] });
        } catch (error) {
          signal?.throwIfAborted();
          state = this.statisticsCommand({ ...submit, error: String(error) });
        } finally {
          clearTimeout(timeout);
          controller.abort();
          await result?.close();
        }
      }
      signal?.throwIfAborted();
      const result: StatisticsResult = this.statisticsCommand({ op: 'finish', id });
      this.retain(result);
      return result;
    } catch (error) {
      this.statisticsCommand({ op: 'cancel', id });
      throw error;
    }
  }
  async query(request: CompileRequest, engine: ExecutionEngine): Promise<ArrowResult> {
    if (request.dialect !== engine.dialect) throw new Error('Compiler and engine SQL dialects differ');
    const plan = this.compile(request);
    if ((plan.transfers as unknown[] | undefined)?.length) throw new Error("Use queryFederated for a multi-engine plan");
    return engine.execute(plan);
  }
}
function defaultLibrary(): string {
  const suffix = process.platform === 'win32' ? 'orchiddb_compiler.dll' :
    process.platform === 'darwin' ? 'liborchiddb_compiler.dylib' : 'liborchiddb_compiler.so';
  const path = fileURLToPath(new URL(`../native/${process.platform}-${process.arch}/${suffix}`, import.meta.url));
  if (!existsSync(path)) throw new Error('No packaged native compiler for this platform; set ORCHIDDB_NATIVE_LIBRARY');
  return path;
}

/** Ensures early cancellation and exceptions release the caller-provided result lease. */
export async function* batches(result: ArrowResult): AsyncGenerator<RecordBatch> {
  try { yield* result; } finally { await result.close(); }
}

/** JSON numbers are emitted directly for bigint so signed int64 values reach Rust intact.
 * Reject rounded Numbers instead of compiling a different parameter value. */
function encodeRequest(value: unknown, ancestors = new Set<object>(), allowUnsigned = false): string {
  if (value === null) return 'null';
  if (typeof value === 'bigint') {
    if (value < -(1n << 63n) || value > (1n << (allowUnsigned ? 64n : 63n)) - 1n)
      throw new RangeError('Integer parameter exceeds 64-bit range');
    return value.toString();
  }
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) throw new TypeError('Numbers must be finite');
    if (Number.isInteger(value) && !Number.isSafeInteger(value))
      throw new RangeError('Unsafe integer Number; pass an exact bigint instead');
    return JSON.stringify(value);
  }
  if (typeof value === 'string' || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value !== 'object') throw new TypeError('Request must contain JSON values or 64-bit bigint');
  if (ancestors.has(value)) throw new TypeError('Cyclic request');
  ancestors.add(value);
  try {
    if (Array.isArray(value)) return '[' + Array.from(value, item => encodeRequest(item, ancestors, allowUnsigned)).join(',') + ']';
    const proto = Object.getPrototypeOf(value);
    if (proto !== Object.prototype && proto !== null) throw new TypeError('Request objects must be plain objects');
    return '{' + Object.entries(value).filter(([, item]) => item !== undefined)
      .map(([key, item]) => JSON.stringify(key) + ':' + encodeRequest(item, ancestors, allowUnsigned)).join(',') + '}';
  } finally { ancestors.delete(value); }
}

/** Adapt a worker-backed DuckDB-WASM connection without owning its lifetime.
 * The session must execute off the JS event loop so deadlines can interrupt it. */
export function asyncDuckDBEngine(connection: {
  send(sql: string, allowStreamResult: boolean): Promise<AsyncIterable<RecordBatch> & {
    schema: Schema; cancel(): void | Promise<void>;
  }>;
  cancelSent(): void | Promise<unknown>;
}): ExecutionEngine {
  let active = false;
  async function executeSQL(sql: string, signal?: AbortSignal): Promise<ArrowResult> {
    signal?.throwIfAborted();
    if (active) throw new Error('Connection already has an active result');
    active = true;
    let reader: Awaited<ReturnType<typeof connection.send>> | undefined;
    let closed = false;
    const abort = () => { void Promise.resolve(connection.cancelSent()).catch(() => {}); };
    signal?.addEventListener('abort', abort, { once: true });
    const close = async () => {
      if (closed) return;
      closed = true;
      signal?.removeEventListener('abort', abort);
      try { await reader?.cancel(); await connection.cancelSent(); }
      finally { active = false; }
    };
    try {
      reader = await connection.send(sql, true);
      signal?.throwIfAborted();
      return {
        schema: reader.schema,
        async *[Symbol.asyncIterator]() {
          if (closed) throw new Error('Result is closed');
          for await (const batch of reader!) { signal?.throwIfAborted(); yield batch; }
        },
        close,
      };
    } catch (error) { await close(); throw error; }
  }
  return {
    id: 'duckdb-async', dialect: 'duckdb',
    execute: query => executeSQL(query.sql),
    executeStatistics: (task, signal) => executeSQL(task.sql, signal),
  };
}

// Arrow's CJS and ESM exports have distinct class identities. Recreate metadata
// and wrappers in the local module while retaining the original typed buffers.
function localArrowBatch(batch: any, a: typeof import('apache-arrow')): RecordBatch {
  if (batch instanceof a.RecordBatch) return batch;
  const field = (f: any): import('apache-arrow').Field => new a.Field(f.name, type(f.type), f.nullable, f.metadata);
  const type = (t: any): import('apache-arrow').DataType => {
    switch (t.typeId) {
      case a.Type.Null: return new a.Null();
      case a.Type.Bool: return new a.Bool();
      case a.Type.Int: return new a.Int(t.isSigned, t.bitWidth);
      case a.Type.Float: return new a.Float(t.precision);
      case a.Type.Utf8: return new a.Utf8();
      case a.Type.LargeUtf8: return new a.LargeUtf8();
      case a.Type.Binary: return new a.Binary();
      case a.Type.LargeBinary: return new a.LargeBinary();
      case a.Type.FixedSizeBinary: return new a.FixedSizeBinary(t.byteWidth);
      case a.Type.Decimal: return new a.Decimal(t.scale, t.precision, t.bitWidth);
      case a.Type.Date: return new a.Date_(t.unit);
      case a.Type.Time: return new a.Time(t.unit, t.bitWidth);
      case a.Type.Timestamp: return new a.Timestamp(t.unit, t.timezone);
      case a.Type.Interval: return new a.Interval(t.unit);
      case a.Type.Duration: return new a.Duration(t.unit);
      case a.Type.List: return new a.List(field(t.children[0]));
      case a.Type.FixedSizeList: return new a.FixedSizeList(t.listSize, field(t.children[0]));
      case a.Type.Struct: return new a.Struct(t.children.map(field));
      case a.Type.Map: return new a.Map_(field(t.children[0]), t.keysSorted);
      case a.Type.Union: return new a.Union(t.mode, t.typeIds, t.children.map(field));
      case a.Type.Dictionary: return new a.Dictionary(type(t.dictionary), type(t.indices) as import('apache-arrow').Int32, t.id, t.isOrdered);
      default: throw new Error(`Unsupported Arrow statistics type ${t.typeId}`);
    }
  };
  const data = (d: any): import('apache-arrow').Data => new a.Data(type(d.type), d.offset, d.length, d.nullCount,
    d.buffers, d.children.map(data), d.dictionary ? new a.Vector(d.dictionary.data.map(data)) : undefined);
  return new a.RecordBatch(new a.Schema(batch.schema.fields.map(field), batch.schema.metadata), data(batch.data) as import('apache-arrow').Data<import('apache-arrow').Struct>);
}

export interface TransferColumn { name: string; data_type: string; nullable: boolean }
export interface DependentOperation {
  engine: string; input_columns: readonly TransferColumn[];
  template: {adapter?: string; dialect?: string; readonly [key: string]: unknown};
}
export interface SqlTransfer {
  source_engine: string; source_dialect: EngineDialect; sql: string;
  target_relation: string; columns: readonly TransferColumn[];
  operation?: DependentOperation; request?: DependentOperation;
}
/** Custom request adapters can participate without implementing SQL execution. */
export interface RequestEngine {
  readonly id: string;
  readonly dialect: EngineDialect;
  executeRequests(requests: readonly unknown[], columns: readonly TransferColumn[]): Promise<ArrowResult>;
}
export type FederatedEngine = ExecutionEngine | RequestEngine;
export interface RemoteOptions {
  endpoint: string;
  authentication?: {type: 'none'} | {type: 'basic'; username: string; password: string}
    | {type: 'bearer' | 'api_key'; token: string};
  page_size?: number; max_rows?: number; max_request_bytes?: number; max_response_bytes?: number;
  request_timeout_ms?: number; connect_timeout_ms?: number;
  scroll_keep_alive?: string; batch_size?: number;
}
/** Caller-owned HTTP session backed by the shared native transport.
 * Network calls block the calling thread; use a worker for latency-sensitive servers. */
export class RemoteEngine implements RequestEngine {
  private session?: string | number;
  readonly id: string;
  constructor(private readonly compiler: Compiler,
      readonly dialect: 'quickwit' | 'elasticsearch', options: RemoteOptions) {
    const result = compiler.remoteCommand({op: 'open', adapter: dialect, options});
    this.session = result.id;
    this.id = String(result.id);
  }
  private command(op: string, values: Record<string, unknown> = {}): any {
    if (this.session === undefined) throw new Error('Remote engine is closed');
    return this.compiler.remoteCommand({op, id: this.session, ...values});
  }
  async executeRequests(requests: readonly unknown[], columns: readonly TransferColumn[]): Promise<ArrowResult> {
    const arrow = await import('apache-arrow');
    const result = this.command('execute', {requests, columns, format: 'ipc'});
    const table = arrow.tableFromIPC(Buffer.from(result.ipc, 'base64'));
    let closed = false;
    return {schema: table.schema, close() {closed = true;}, async *[Symbol.asyncIterator]() {
      if (closed) throw new Error('Result is closed');
      for (const batch of table.batches) {if (closed) throw new Error('Result is closed'); yield batch;}
    }};
  }
  clearMetadataCache(): void {this.command('clear_metadata_cache');}
  close(): void {
    if (this.session !== undefined) {this.command('close'); this.session = undefined;}
  }
}

export async function queryFederated<T>(compiler: Compiler, request: CompileRequest,
    engines: ReadonlyMap<string, FederatedEngine>, consume: (result: ArrowResult) => Promise<T>): Promise<T> {
  let plan = compiler.compile(request);
  const targetId = plan.execution_engine as string | undefined;
  if (!targetId) throw new Error('Federated execution requires execution_engine');
  const engine = (id: string, dialect?: EngineDialect): FederatedEngine => {
    const found = engines.get(id);
    if (!found || (dialect !== undefined && found.dialect !== dialect))
      throw new Error(`Missing engine or dialect mismatch: ${id}`);
    return found;
  };
  const sqlEngine = (id: string, dialect?: EngineDialect): ExecutionEngine => {
    const found = engine(id, dialect);
    if (!('execute' in found)) throw new Error(`Engine ${id} does not support SQL execution`);
    return found;
  };
  const requestEngine = (id: string, dialect?: EngineDialect): RequestEngine => {
    const found = engine(id, dialect);
    if (!('executeRequests' in found)) throw new Error(`Engine ${id} does not support request execution`);
    return found;
  };
  const target = sqlEngine(targetId, plan.dialect);
  for (const t of (plan.transfers ?? []) as readonly SqlTransfer[]) {
    const op = t.request ?? t.operation;
    if (!op || t.sql) sqlEngine(t.source_engine, t.source_dialect);
    if (op) {
      if (t.request) requestEngine(op.engine, op.template.adapter);
      else sqlEngine(op.engine, op.template.dialect);
    }
  }
  const query = (sql: string, dialect: EngineDialect, columns: readonly TransferColumn[]): CompiledQuery => ({
    version: 1, sql, dialect: dialect as Dialect, fields: columns.map(c => c.name), field_types: columns.map(c => c.data_type),
  });
  // Bind from the current plan: exchanges may rewrite later operation inputs.
  while ((plan.transfers as readonly SqlTransfer[] | undefined)?.length) {
    const t = (plan.transfers as readonly SqlTransfer[])[0];
    const op = t.request ?? t.operation;
    if (op) {
      let bound: any;
      if (t.sql) {
        const source = await sqlEngine(t.source_engine).execute(query(t.sql, t.source_dialect, op.input_columns));
        try {bound = await compiler.bindOperationArrow(plan, t.target_relation, source);}
        finally {await source.close();}
      } else bound = compiler.operationCommand({op: 'bind_operation', plan, relation: t.target_relation, rows: [[]]});
      if (bound.requests) {
        const source = await requestEngine(bound.engine).executeRequests(bound.requests, t.columns);
        try {plan = await compiler.bindArrow(plan, t.target_relation, source);}
        finally {await source.close();}
      } else {
        const batches: RecordBatch[] = [];
        let schema: Schema | undefined;
        for (const sql of bound.sql) {
          const source = await sqlEngine(bound.engine).execute(query(sql, bound.dialect, t.columns));
          try {schema = source.schema; for await (const batch of source) batches.push(batch);}
          finally {await source.close();}
        }
        if (schema) plan = await compiler.bindArrow(plan, t.target_relation, {
          schema, close() {}, async *[Symbol.asyncIterator]() {yield* batches;},
        });
        else plan = compiler.operationCommand({op: 'bind', plan, relation: t.target_relation, rows: []});
      }
    } else {
      const source = await sqlEngine(t.source_engine).execute(query(t.sql, t.source_dialect, t.columns));
      try {plan = await compiler.bindArrow(plan, t.target_relation, source);}
      finally {await source.close();}
    }
  }
  const result = await target.execute(plan);
  try {return await consume(result);} finally {await result.close();}
}

/** Native request payloads may contain exact uint64 keys. JSON.parse alone rounds them. */
function parseResponse(text: string): any {
  JSON.parse(text); // Validate the complete grammar before recovering exact integers.
  const tokens = text.match(/"(?:[^"\\]|\\.)*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\]:,]/g) ?? [];
  let index = 0;
  function value(): any {
    const token = tokens[index++];
    if (token === '[') {
      const result = [];
      while (tokens[index] !== ']') {result.push(value()); if (tokens[index] !== ',') break; index++;}
      if (tokens[index++] !== ']') throw new Error('Invalid native JSON array');
      return result;
    }
    if (token === '{') {
      const result: Record<string, unknown> = {};
      while (tokens[index] !== '}') {
        const key = JSON.parse(tokens[index++]);
        if (typeof key !== 'string' || tokens[index++] !== ':') throw new Error('Invalid native JSON object');
        Object.defineProperty(result, key, {value: value(), enumerable: true, writable: true, configurable: true});
        if (tokens[index] !== ',') break; index++;
      }
      if (tokens[index++] !== '}') throw new Error('Invalid native JSON object');
      return result;
    }
    if (/^-?\d+$/.test(token) && !Number.isSafeInteger(Number(token))) return BigInt(token);
    return JSON.parse(token);
  }
  const result = value();
  if (index !== tokens.length) throw new Error('Invalid native JSON response');
  return result;
}
