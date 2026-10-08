package io.orchiddb;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.util.Objects;

public final class CatalogAuth {
  private static final ObjectMapper JSON = new ObjectMapper();
  private final ObjectNode configuration;

  private CatalogAuth(ObjectNode configuration) { this.configuration = configuration; }

  public static CatalogAuth bearer(String token) { return bearer(Credential.value(token)); }
  public static CatalogAuth bearer(Credential token) {
    var config = JSON.createObjectNode().put("type", "bearer");
    config.set("token", JSON.valueToTree(token.configuration()));
    return new CatalogAuth(config);
  }
  public static CatalogAuth clientCredentials(String clientId, String clientSecret) {
    return clientCredentials(clientId, Credential.value(clientSecret));
  }
  public static CatalogAuth clientCredentials(String clientId, Credential clientSecret) {
    var config = JSON.createObjectNode().put("type", "client_credentials")
        .put("client_id", Objects.requireNonNull(clientId)).put("scope", "PRINCIPAL_ROLE:ALL");
    config.set("client_secret", JSON.valueToTree(clientSecret.configuration()));
    return new CatalogAuth(config);
  }
  public static CatalogAuth tokenExchange(Credential subjectToken) {
    var config = JSON.createObjectNode().put("type", "token_exchange").put("scope", "PRINCIPAL_ROLE:ALL");
    config.set("subject_token", JSON.valueToTree(subjectToken.configuration()));
    return new CatalogAuth(config);
  }
  public CatalogAuth tokenEndpoint(String endpoint) { return option("token_endpoint", endpoint); }
  public CatalogAuth issuer(String issuer) {
    if (!configuration.path("type").asText().equals("client_credentials"))
      throw new IllegalStateException("OIDC discovery requires client credentials");
    return option("issuer", issuer);
  }
  public CatalogAuth scope(String scope) { return option("scope", scope); }
  private CatalogAuth option(String name, String value) {
    if (configuration.path("type").asText().equals("bearer"))
      throw new IllegalStateException("Bearer authentication has no OAuth options");
    return new CatalogAuth(configuration.deepCopy().put(name, Objects.requireNonNull(value)));
  }
  ObjectNode configuration() { return configuration.deepCopy(); }
  @Override public String toString() { return "CatalogAuth(" + configuration.path("type").asText() + ")"; }
}
