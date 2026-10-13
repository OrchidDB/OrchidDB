package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"log"

	orchiddb "github.com/OrchidDB/OrchidDB/clients/go"
	_ "github.com/marcboeker/go-duckdb"
)

var schema = json.RawMessage(`{"tables":[{"name":"people","columns":[{"name":"id","data_type":"int64","nullable":false},{"name":"name","data_type":"string"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}]}`)

func run() error {
	ctx := context.Background()
	db, err := sql.Open("duckdb", "")
	if err != nil {
		return err
	}
	defer db.Close()
	session, err := db.Conn(ctx)
	if err != nil {
		return err
	}
	defer session.Close()
	if _, err = session.ExecContext(ctx, "CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'Ada'), (2, 'Grace')"); err != nil {
		return err
	}
	graph, err := orchiddb.New(session, schema, orchiddb.Config{Dialect: "duckdb"})
	if err != nil {
		return err
	}
	defer graph.Close()
	rows, err := graph.Query(ctx, "MATCH (p:Person) WHERE p.name = $name RETURN p.name AS name", orchiddb.QueryOptions{Parameters: map[string]any{"name": "Ada"}})
	if err != nil {
		return err
	}
	defer rows.Close()
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			return err
		}
		fmt.Println(name)
	}
	return rows.Err()
}
func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}
