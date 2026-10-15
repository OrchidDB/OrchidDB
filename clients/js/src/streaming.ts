import {Compiler} from './internal.js';
import type {Authorization} from './internal.js';
export interface PermissionCheck {resource_type: string; resource_id: string; permission: string}
/** Implement using your SpiceDB client; any caching and expiry belong here. */
export type StreamAuthorizer = (subject: Authorization, checks: readonly PermissionCheck[]) => readonly boolean[] | Promise<readonly boolean[]>;
export interface StreamOptions {
  kafka: Record<string,string>; topic: string; source: string; authorizer: StreamAuthorizer;
  library?: string; auth_timeout_ms?: number;
}
export interface StreamQueryOptions {subject: Authorization; parameters?: Record<string,unknown>; snapshot?: unknown}
/** A single independent Kafka reader/writer. Results carry base64 Arrow IPC.
 * Close cancels pending reads. Use separate connections for concurrent operations.
 * Persist a batch's offsets only after processing every replacement frame.
 */
export class StreamConnection {
  private runtime: Compiler;
  private id: string | undefined;
  private busy = false;
  private authorize: StreamAuthorizer;
  constructor(schema: unknown, options: StreamOptions) {
    if (typeof options.authorizer !== 'function') throw new TypeError('authorizer callback required');
    this.runtime = new Compiler(options.library);
    this.authorize = options.authorizer;
    this.id = this.runtime.operationCommand({op:'stream_open',schema,kafka:options.kafka,
      topic:options.topic,source:options.source,auth_timeout_ms:options.auth_timeout_ms}).id;
  }
  private command(op: string, values: Record<string,unknown> = {}): any {
    if (this.id === undefined) throw new Error('Stream connection is closed');
    return this.runtime.operationCommand({op,id:this.id,...values});
  }
  private async run(action: string, values: Record<string,unknown> = {}): Promise<any> {
    if (this.busy) throw new Error('Stream connection is busy');
    this.busy = true;
    try {
      this.command('stream_start',{action,...values});
      for (;;) {
        const event = this.command('stream_poll');
        if (event.state === 'done') return event.result;
        if (event.state === 'authorize') {
          try {
            const decisions = await this.authorize(event.subject,event.checks);
            if (!Array.isArray(decisions) || decisions.length !== event.checks.length || decisions.some(d => typeof d !== 'boolean'))
              throw new Error('authorizer must return one boolean per check');
            this.command('stream_authorize',{nonce:event.nonce,decisions});
          } catch (e) {this.close();throw e;}
        }
        // Allow application cancellation and other tasks while polling Kafka.
        await new Promise<void>(resolve => setImmediate(resolve));
      }
    } finally {this.busy = false;}
  }
  insert(change: unknown, subject: Authorization) {return this.run('insert',{change,subject});}
  insertBatch(changes: readonly unknown[], subject: Authorization) {return this.run('insert_batch',{changes,subject});}
  query(text: string, options: StreamQueryOptions) {return this.run(options.snapshot === undefined ? 'query' : 'hybrid',{text,parameters:{},...options});}
  subscribe(text: string, offsets: Record<string,number | bigint>, options: StreamQueryOptions) {return this.run('subscribe',{text,offsets,parameters:{},...options});}
  nextBatch(maxRecords = 1, waitMs = 100) {return this.run('next',{max_records:maxRecords,wait_ms:waitMs});}
  unsubscribe() {return this.run('unsubscribe');}
  close(): void {if(this.id !== undefined) {this.command('stream_close');this.id=undefined;this.runtime.close();}}
}
