from dataclasses import dataclass, field
from typing import Any, Mapping
from ._runtime import _Runtime


@dataclass(frozen=True, repr=False)
class Credential:
    source: str
    value: str

    @classmethod
    def literal(cls, value):
        return cls("value", value)

    @classmethod
    def env(cls, name):
        return cls("env", name)

    @classmethod
    def file(cls, path):
        return cls("file", str(path))

    def __repr__(self):
        return "Credential([redacted])"

    def _configuration(self):
        return dict(source=self.source, value=self.value)


@dataclass(frozen=True, repr=False)
class CatalogAuth:
    _config: Mapping[str, Any]

    @staticmethod
    def _credential(value):
        return (value if isinstance(value, Credential) else Credential.literal(value))._configuration()

    @classmethod
    def bearer(cls, token):
        return cls(dict(type="bearer", token=cls._credential(token)))

    @classmethod
    def client_credentials(cls, client_id, client_secret, *, token_endpoint=None,
                           issuer=None, scope="PRINCIPAL_ROLE:ALL"):
        return cls(dict(type="client_credentials", client_id=client_id,
                        client_secret=cls._credential(client_secret), token_endpoint=token_endpoint,
                        issuer=issuer, scope=scope))

    @classmethod
    def token_exchange(cls, subject_token, *, token_endpoint=None, scope="PRINCIPAL_ROLE:ALL"):
        return cls(dict(type="token_exchange", subject_token=cls._credential(subject_token),
                        token_endpoint=token_endpoint, scope=scope))

    def __repr__(self):
        return f"CatalogAuth({self._config['type']})"


@dataclass(frozen=True)
class CypherEdge:
    name: str
    source: str
    target: str
    cypher: str
    description: str
    target_column: str = "target"
    properties: Mapping[str, Any] = field(default_factory=dict)
    parameters: tuple = ()

    def _definition(self):
        return dict(kind="cypher_relationship", name=self.name, source=self.source,
                    target=self.target, cypher=self.cypher, description=self.description,
                    parameters=list(self.parameters),
                    returns=dict(target=self.target_column, properties=dict(self.properties)))


class Catalog:
    def __init__(self, endpoint, *, scope, graph, token_env="ORCHID_CATALOG_TOKEN",
                 revision=None, library=None, auth=None):
        self._reference = dict(endpoint=endpoint, scope=scope, graph=graph, token_env=token_env)
        if revision is not None:
            self._with_revision(revision)
        self._library = library
        if auth is not None and not isinstance(auth, CatalogAuth):
            raise TypeError("auth must be a CatalogAuth")
        self._auth = auth

    def __repr__(self):
        return f"Catalog(scope={self._reference['scope']!r}, graph={self._reference['graph']!r})"

    def at_revision(self, revision):
        return Catalog(**self._reference, library=self._library, auth=self._auth)._with_revision(revision)

    def _with_revision(self, revision):
        if not isinstance(revision, int) or isinstance(revision, bool) or revision < 1:
            raise ValueError("revision must be a positive integer")
        self._reference["revision"] = revision
        return self

    def _schema(self):
        reference = dict(self._reference)
        if self._auth is not None:
            reference["auth"] = dict(self._auth._config)
        return {"catalog": reference}

    def _command(self, action, **values):
        with _Runtime(self._library) as runtime:
            return runtime.operation_command(dict(op="catalog", catalog=self._schema()["catalog"],
                                                  action=action, **values))

    def discover(self, search=None):
        return self._command("discover", search=search)

    def edges(self, search=None):
        return self._command("edges", search=search)["objects"]

    def object(self, id):
        return self._command("object", id=id)

    def principals(self):
        return self._command("principals")

    def principal(self, id):
        return self._command("principal", id=id)

    def register_principal(self, id, *, roles=(), admin=False, tenant=None, enabled=True, expected_version=0):
        return self._command("register_principal", id=id, expected_version=expected_version, enabled=enabled,
                             principal=dict(subject=id, roles=list(roles), admin=admin, tenant=tenant))

    def grants(self):
        return self._command("grants")

    def set_grants(self, *, discover=(), execute=(), expected_version=0):
        return self._command("set_grants", expected_version=expected_version,
                             definition=dict(discover=list(discover), execute=list(execute)))

    def register_edge(self, id, edge: CypherEdge, *, expected_version=0):
        return self._command("register_edge", id=id, definition=edge._definition(),
                             expected_version=expected_version)

    def draft(self):
        return self._command("draft")

    def publish(self, *, expected_revision, graph_version, object_versions):
        return self._command("publish", publication=dict(expected_revision=expected_revision,
                             graph_version=graph_version, object_versions=dict(object_versions)))

    def register_graph(self, objects, *, description, expected_version=0, execution_connector=None):
        return self._command("register_graph", expected_version=expected_version,
                             definition=dict(objects=list(objects), description=description,
                                             execution_connector=execution_connector))
