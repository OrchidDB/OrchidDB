export interface CredentialConfig { source: 'value' | 'env' | 'file'; value: string }
export type CatalogAuthConfig =
  | {type: 'bearer'; token: CredentialConfig}
  | {type: 'client_credentials'; client_id: string; client_secret: CredentialConfig;
      token_endpoint?: string; issuer?: string; scope: string}
  | {type: 'token_exchange'; subject_token: CredentialConfig; token_endpoint?: string; scope: string};
export interface OAuthOptions { tokenEndpoint?: string; scope?: string }
export class Credential {
  readonly #configuration: CredentialConfig;
  private constructor(source: CredentialConfig['source'], value: string) {
    if (!value) throw new TypeError('Credential cannot be empty');
    this.#configuration = {source, value};
  }
  static value(value: string) { return new Credential('value', value); }
  static env(name: string) { return new Credential('env', name); }
  static file(path: string) { return new Credential('file', path); }
  configuration(): CredentialConfig { return {...this.#configuration}; }
  toString() { return 'Credential([redacted])'; }
}
export class CatalogAuth {
  readonly #configuration: CatalogAuthConfig;
  private constructor(configuration: CatalogAuthConfig) { this.#configuration = configuration; }
  private static credential(value: string | Credential): CredentialConfig {
    return (typeof value === 'string' ? Credential.value(value) : value).configuration();
  }
  static bearer(token: string | Credential) {
    return new CatalogAuth({type:'bearer', token:this.credential(token)});
  }
  static clientCredentials(clientId: string, clientSecret: string | Credential,
      options: OAuthOptions & {issuer?: string} = {}) {
    return new CatalogAuth({type:'client_credentials', client_id:clientId, client_secret:this.credential(clientSecret),
      token_endpoint:options.tokenEndpoint, issuer:options.issuer, scope:options.scope ?? 'PRINCIPAL_ROLE:ALL'});
  }
  static tokenExchange(subjectToken: string | Credential, options: OAuthOptions = {}) {
    return new CatalogAuth({type:'token_exchange', subject_token:this.credential(subjectToken),
      token_endpoint:options.tokenEndpoint, scope:options.scope ?? 'PRINCIPAL_ROLE:ALL'});
  }
  configuration(): CatalogAuthConfig { return structuredClone(this.#configuration); }
}
