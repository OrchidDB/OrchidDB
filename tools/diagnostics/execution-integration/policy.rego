package trino
import rego.v1

default allow := false
allow if input.context.identity.user in {"admin", "alice", "bob"}

rowFilters contains {"expression": "tenant = 'alice'"} if {
    input.context.identity.user == "alice"
    input.action.resource.table.tableName == "documents"
}
rowFilters contains {"expression": "tenant = 'bob'"} if {
    input.context.identity.user == "bob"
    input.action.resource.table.tableName == "documents"
}
columnMask := {"expression": "CAST('masked' AS VARCHAR)"} if {
    input.context.identity.user != "admin"
    input.action.resource.column.tableName == "documents"
    input.action.resource.column.columnName == "secret"
}

# May enter the query engine, but cannot select table columns.
allow if {
    input.context.identity.user == "restricted"
    input.action.operation != "SelectFromColumns"
}
