package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"os"
	"testing"

	orchiddb "github.com/OrchidDB/OrchidDB/clients/go"
)

func TestNativeDuckDB(t *testing.T) {
	if os.Getenv("ORCHIDDB_NATIVE_LIBRARY") == "" {
		t.Skip("set ORCHIDDB_NATIVE_LIBRARY to run native integration")
	}
	if err := run(); err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	db, err := sql.Open("duckdb", "")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	session, err := db.Conn(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer session.Close()
	if _, err = session.ExecContext(ctx, "CREATE TABLE people(id BIGINT, name VARCHAR)"); err != nil {
		t.Fatal(err)
	}
	tx, err := session.BeginTx(ctx, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer tx.Rollback()
	if _, err = tx.ExecContext(ctx, "INSERT INTO people VALUES (1, 'O''Reilly')"); err != nil {
		t.Fatal(err)
	}
	graph, err := orchiddb.New(tx, schema, orchiddb.Config{Dialect: "duckdb"})
	if err != nil {
		t.Fatal(err)
	}
	for _, query := range []struct{ language, text string }{
		{"cypher", "MATCH (p:Person) WHERE p.name = $name RETURN p.name AS name"},
	} {
		options := orchiddb.QueryOptions{Language: query.language}
		if query.language == "cypher" {
			options.Parameters = map[string]any{"name": "O'Reilly"}
		}
		rows, err := graph.Query(ctx, query.text, options)
		if err != nil {
			t.Fatal(err)
		}
		var name string
		if !rows.Next() {
			t.Fatalf("missing uncommitted row: %v", rows.Err())
		}
		if err = rows.Scan(&name); err != nil || name != "O'Reilly" {
			t.Fatalf("%s: %v", name, err)
		}
		if rows.Next() {
			t.Fatal("unexpected extra row")
		}
		if err = rows.Err(); err != nil {
			t.Fatal(err)
		}
		rows.Close()
	}
	if _, err = graph.Query(ctx, "this is not valid graph syntax", orchiddb.QueryOptions{}); err == nil {
		t.Fatal("accepted invalid query")
	}
	if err = graph.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err = graph.Query(ctx, "RETURN 1", orchiddb.QueryOptions{}); !errors.Is(err, orchiddb.ErrClosed) {
		t.Fatal(err)
	}
	if err = tx.Rollback(); err != nil {
		t.Fatalf("client closed transaction: %v", err)
	}
	var count int
	if err = session.QueryRowContext(ctx, "SELECT count(*) FROM people").Scan(&count); err != nil || count != 0 {
		t.Fatalf("transaction was committed or session closed: %d %v", count, err)
	}
	for _, bad := range []string{`null`, `[]`, `{"query":"RETURN 1"}`, `{"dialect":"duckdb"}`, `{"parameters":{}}`} {
		if c, err := orchiddb.New(session, json.RawMessage(bad), orchiddb.Config{Dialect: "duckdb"}); err == nil {
			c.Close()
			t.Fatalf("accepted schema %s", bad)
		}
	}
}
