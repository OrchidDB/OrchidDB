/** Graph execution on caller-owned connections. */
import { Compiler as Runtime, queryFederated, RemoteEngine as NativeRemote } from './internal.js';
import type { CompileRequest, ExecutionEngine, ArrowResult, Language, Authorization, FederatedEngine, RemoteOptions } from './internal.js';
export { batches, asyncDuckDBEngine, permissionRelation, permissionScope, authorization } from './internal.js';
export type { ArrowResult, ExecutionEngine, Language, Dialect, EngineDialect, Column, Table, NodeMapping,
  EdgeMapping, FunctionSignature, Ontology, RdfMapping, RdfTermMapping, Authorization, PermissionScope,
  PermissionRelation, StatisticsRequest, StatisticsResult, RequestEngine, FederatedEngine, RemoteOptions } from './internal.js';
/** Engine implementors receive SQL work; callers submit query text to Connection.query. */
export type { CompiledQuery as SqlWork } from './internal.js';
export type Schema = Omit<CompileRequest, 'version' | 'dialect' | 'language' | 'query' | 'parameters' | 'authorization'>;
export interface QueryOptions { language?: Language; parameters?: Readonly<Record<string, unknown>>; authorization?: Authorization }
export class Connection {
  private readonly runtime: Runtime;
  private readonly schema: Schema;
  private closed = false;
  constructor(private readonly engine: ExecutionEngine, schema: Schema,
      private readonly engines?: ReadonlyMap<string, FederatedEngine>, library?: string) {
    this.runtime = new Runtime(library);
    this.schema = this.runtime.operationCommand({op:'validate_schema', schema});
  }
  private request(text: string, options: QueryOptions = {}): CompileRequest {
    if (this.closed) throw new Error('Connection is closed');
    if (typeof text !== 'string') throw new TypeError('Query must be text, separate from schema');
    return {...this.schema, version:1, dialect:this.engine.dialect, language:options.language ?? 'cypher',
      query:text, parameters:options.parameters, authorization:options.authorization};
  }
  async query(text: string, options: QueryOptions = {}): Promise<ArrowResult> {
    const request = this.request(text, options);
    if (!this.engines) return this.runtime.query(request, this.engine);
    return queryFederated(this.runtime, request, this.engines, async result => {
      const saved: import('apache-arrow').RecordBatch[] = [];
      for await (const batch of result) saved.push(batch);
      let closed = false;
      return {schema:result.schema, close() {closed=true;}, async *[Symbol.asyncIterator]() {
        if (closed) throw new Error('Result is closed');
        for (const batch of saved) { if (closed) break; yield batch; }
      }};
    });
  }
  generateStatistics(signal?: AbortSignal) { return this.runtime.generateStatistics(this.request('RETURN 1'), this.engine, signal); }
  clearStatistics() { this.runtime.clearStatistics(); }
  saveStatistics(path: string) { this.runtime.saveStatistics(path); }
  loadStatistics(path: string) { this.runtime.loadStatistics(path); }
  close() { this.runtime.close(); this.closed=true; }
}
export class RemoteEngine extends NativeRemote {
  constructor(adapter: 'quickwit' | 'elasticsearch', options: RemoteOptions, library?: string) {
    super(new Runtime(library), adapter, options);
  }
}
