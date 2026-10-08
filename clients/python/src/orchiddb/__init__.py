"""Connection-based graph query execution."""
from .catalog import Catalog, CatalogAuth, Credential, CypherEdge
from .execution import Connection, DuckDBEngine, PostgresEngine, ArrowEngine
from ._runtime import CompilationError as QueryError
from .permissions import Authorization, PermissionRelation, PermissionScope
from .remote import RemoteEngine
from .starrocks import StarRocksEngine
__all__ = ["Catalog", "CatalogAuth", "Credential", "CypherEdge", "Connection", "StarRocksEngine", "DuckDBEngine", "PostgresEngine", "ArrowEngine", "QueryError",
           "RemoteEngine", "Authorization", "PermissionRelation", "PermissionScope"]
