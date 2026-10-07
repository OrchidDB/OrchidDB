"""Provider-neutral request helpers for pushing effective grants into SQL."""
from dataclasses import dataclass


@dataclass(frozen=True)
class Authorization:
    subject_type: str
    subject_id: str

    def to_dict(self):
        return {"subject_type": self.subject_type, "subject_id": self.subject_id}


@dataclass(frozen=True)
class PermissionRelation:
    table: str
    resource_type: str
    permission: str
    resource_type_column: str = "resource_type"
    permission_column: str = "resource_rel"
    resource_id_column: str = "resource_id"
    subject_type_column: str = "subject_type"
    subject_relation_column: str = "subject_rel"
    subject_id_column: str = "subject_id"

    @classmethod
    def flat(cls, table, resource_type, permission, **columns):
        """Build a flat relation mapping; pass column-name overrides as keywords."""
        return cls(table, resource_type, permission, **columns)

    def to_dict(self):
        return {
            "table": self.table,
            "resource_type": self.resource_type,
            "permission": self.permission,
            "resource_type_column": self.resource_type_column,
            "permission_column": self.permission_column,
            "resource_id_column": self.resource_id_column,
            "subject_type_column": self.subject_type_column,
            "subject_relation_column": self.subject_relation_column,
            "subject_id_column": self.subject_id_column,
        }


@dataclass(frozen=True)
class PermissionScope:
    resource_column: str
    relation: PermissionRelation

    def to_dict(self):
        return {"resource_column": self.resource_column, "relation": self.relation.to_dict()}
