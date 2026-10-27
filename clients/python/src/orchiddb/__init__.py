"""Connection-based graph query execution."""
from .catalog import Catalog, CatalogAuth, Credential, CypherEdge
from .execution import Connection, DuckDBEngine, LazyDuckDBEngine, PostgresEngine, ArrowEngine
from ._runtime import CompilationError as QueryError
from .permissions import Authorization, PermissionRelation, PermissionScope
from .remote import RemoteEngine
from .starrocks import StarRocksEngine
from .trino import TrinoEngine, TrinoSource
__all__ = ["Catalog", "CatalogAuth", "Credential", "CypherEdge", "Connection", "StarRocksEngine", "TrinoEngine", "TrinoSource", "DuckDBEngine", "LazyDuckDBEngine", "PostgresEngine", "ArrowEngine", "QueryError",
           "RemoteEngine", "Authorization", "PermissionRelation", "PermissionScope"]

from .streaming import StreamConnection
__all__.append("StreamConnection")
