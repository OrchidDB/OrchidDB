"""Database-free compilation; results stay in the caller's Arrow engine."""
from .compiler import Compiler, CompiledQuery, CompilationError
from .permissions import Authorization, PermissionRelation, PermissionScope
from .execution import DuckDBEngine, PostgresEngine, ArrowEngine, Graph
__all__ = ["Compiler", "CompiledQuery", "CompilationError", "DuckDBEngine", "PostgresEngine", "query_federated", "ArrowEngine", "Graph",
           "RemoteEngine", "Authorization", "PermissionRelation", "PermissionScope"]

from .federation import query_federated

from .remote import RemoteEngine
