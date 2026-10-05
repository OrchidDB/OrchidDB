//! cargo run --example portable_function_catalog
fn main() {
    use orchiddb::ir::functions::portable::capabilities;
    println!(
        "| Function | DuckDB SQL | PostgreSQL SQL | DuckDB scope / reason | PostgreSQL scope / reason |\n| --- | --- | --- | --- | --- |"
    );
    for function in capabilities() {
        let support = |available| {
            if function.preparation {
                "lowered*"
            } else if available {
                "mapped*"
            } else {
                "native"
            }
        };
        println!(
            "| `{}` | {} | {} | {} | {} |",
            function.name,
            support(function.duckdb),
            support(function.postgres),
            function.duckdb_note,
            function.postgres_note
        );
    }
    println!(
        "\n*Lowered functions resolve Arrow schema/query metadata or become casts during compilation. Runtime mappings are specific to arity, argument types, and options. Calls without a matching mapping execute natively; SQL-only compilation rejects them. Every listed function has a native implementation. Aliases resolve to the same identity."
    );
}
