package orchid_test
import rego.v1

default allow := false
allow if {
    input.context.identity.user == "alice"
    input.action.operation == "ExecuteQuery"
}
allow if {
    input.context.identity.user == "alice"
    input.action.operation == "SelectFromColumns"
    input.action.resource.table.tableName == "people"
}
batch contains i if {
    some i, resource in input.action.filterResources
    allow with input.action.resource as resource
}
rowFilters := [{"expression": "tenant = 'a'"}]
batchMasks contains {"index": i, "viewExpression": {"expression": "0"}} if {
    some i, resource in input.action.filterResources
    resource.column.columnName == "salary"
}
