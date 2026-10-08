import { CatalogAuth } from './catalog-auth.js';
export { CatalogAuth, Credential } from './catalog-auth.js';
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
  constructor(private readonly engine: ExecutionEngine, schema: Schema | Catalog,
      private readonly engines?: ReadonlyMap<string, FederatedEngine>, library?: string) {
    this.runtime = new Runtime(library ?? (schema instanceof Catalog ? schema.library : undefined));
    this.schema = this.runtime.operationCommand({op:'validate_schema', schema: schema instanceof Catalog ? schema.configuration() : schema});
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
  constructor(adapter: 'quickwit' | 'elasticsearch' | 'weaviate', options: RemoteOptions, library?: string) {
    super(new Runtime(library), adapter, options);
  }
}

export interface CatalogOptions {
  scope: string;
  graph: string;
  tokenEnv?: string;
  auth?: CatalogAuth;
  revision?: number;
  library?: string;
}
export interface CypherEdge {
  name: string;
  source: string;
  target: string;
  cypher: string;
  description: string;
  parameters?: readonly {name: string; schema: unknown; default?: unknown}[];
  targetColumn?: string;
  properties?: Readonly<Record<string, unknown>>;
  returns?: {target: string; properties?: Readonly<Record<string, unknown>>};
}
export interface CatalogRecord {
  version: number;
  definition: Readonly<Record<string, unknown>>;
}
export interface CatalogObject {
  id: string;
  version: number;
  definition: Readonly<Record<string, unknown>>;
}
export interface CatalogDiscovery {
  revision: number;
  description: string;
  objects: CatalogObject[];
}
export class Catalog {
  private readonly options: Readonly<CatalogOptions>;
  constructor(private readonly endpoint: string, options: CatalogOptions) {
    if (options.revision !== undefined && (!Number.isSafeInteger(options.revision) || options.revision < 1)) {
      throw new RangeError('revision must be a positive safe integer');
    }
    this.options = Object.freeze({...options});
  }
  get library(): string | undefined { return this.options.library; }
  atRevision(revision: number): Catalog { return new Catalog(this.endpoint, {...this.options, revision}); }
  configuration(): Schema {
    return {catalog: {endpoint:this.endpoint, scope:this.options.scope, graph:this.options.graph,
      token_env:this.options.tokenEnv ?? 'ORCHID_CATALOG_TOKEN', revision:this.options.revision, auth:this.options.auth?.configuration()}};
  }
  private command<T>(action: string, values: Record<string, unknown> = {}): T {
    const runtime = new Runtime(this.options.library);
    try { return runtime.operationCommand({op:'catalog', catalog:this.configuration().catalog, action, ...values}); }
    finally { runtime.close(); }
  }
  discover(search?: string): CatalogDiscovery { return this.command('discover', {search}); }
  edges(search?: string): CatalogObject[] { return this.command<CatalogDiscovery>('edges', {search}).objects; }
  object(id: string): CatalogRecord { return this.command('object', {id}); }
  principals(): unknown { return this.command('principals'); }
  principal(id: string): CatalogRecord { return this.command('principal', {id}); }
  registerPrincipal(id: string, options: {roles: readonly string[]; admin?: boolean; tenant?: string;
      enabled?: boolean; expectedVersion?: number}): {version: number; client_id: string; client_secret: string} {
    return this.command('register_principal', {id, expected_version:options.expectedVersion ?? 0,
      enabled:options.enabled ?? true, principal:{subject:id, roles:options.roles, admin:options.admin ?? false, tenant:options.tenant}});
  }
  grants(): CatalogRecord { return this.command('grants'); }
  setGrants(discover: readonly string[], execute: readonly string[], expectedVersion = 0): CatalogRecord {
    return this.command('set_grants', {expected_version:expectedVersion, definition:{discover,execute}});
  }
  registerEdge(id: string, edge: CypherEdge, expectedVersion = 0): CatalogRecord {
    return this.command('register_edge', {id, expected_version:expectedVersion,
      definition:{kind:'cypher_relationship', name:edge.name, source:edge.source, target:edge.target,
        cypher:edge.cypher, description:edge.description, parameters:edge.parameters ?? [],
        returns:{target:edge.returns?.target ?? edge.targetColumn ?? 'target',
          properties:edge.returns?.properties ?? edge.properties ?? {}}}});
  }
  draft(): CatalogRecord { return this.command('draft'); }
  registerGraph(objects: readonly string[], description: string, expectedVersion = 0,
      executionConnector?: string): {version: number} {
    return this.command('register_graph', {expected_version:expectedVersion,
      definition:{objects, description, execution_connector:executionConnector}});
  }
  publish(publication: {expectedRevision: number; graphVersion: number; objectVersions: Readonly<Record<string, number>>}): unknown {
    return this.command('publish', {publication:{expected_revision:publication.expectedRevision,
      graph_version:publication.graphVersion, object_versions:publication.objectVersions}});
  }
}
