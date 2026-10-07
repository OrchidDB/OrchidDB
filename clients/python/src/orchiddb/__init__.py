"""Connection-based graph query execution."""
from .execution import Connection, DuckDBEngine, PostgresEngine, ArrowEngine
from ._runtime import CompilationError as QueryError
from .permissions import Authorization, PermissionRelation, PermissionScope
from .remote import RemoteEngine
__all__ = ["Connection", "DuckDBEngine", "PostgresEngine", "ArrowEngine", "QueryError",
           "RemoteEngine", "Authorization", "PermissionRelation", "PermissionScope"]
