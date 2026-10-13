package orchiddb

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"testing"
)

type testSession struct {
	calls int
	query string
	err   error
}

func (s *testSession) QueryContext(_ context.Context, query string, args ...any) (*sql.Rows, error) {
	s.calls++
	s.query = query
	if len(args) != 0 {
		panic("parameters must be compiled by the core")
	}
	return nil, s.err
}

func TestQuery(t *testing.T) {
	session := &testSession{err: errors.New("driver error")}
	c := &Connection{session: session, dialect: "duckdb", schema: map[string]json.RawMessage{"tables": json.RawMessage(`[]`)}}
	c.command = func(request any, result any) error {
		r := request.(map[string]any)
		if r["language"] != "cypher" || r["dialect"] != "duckdb" || r["query"] != "RETURN $name" {
			t.Fatalf("bad request: %v", r)
		}
		if r["parameters"].(map[string]any)["name"] != "O'Reilly" {
			t.Fatal("lost parameter")
		}
		return json.Unmarshal([]byte(`{"version":1,"dialect":"duckdb","sql":"SELECT 'O''Reilly'"}`), result)
	}
	_, err := c.Query(context.Background(), "RETURN $name", QueryOptions{Parameters: map[string]any{"name": "O'Reilly"}})
	if !errors.Is(err, session.err) || session.calls != 1 || session.query != "SELECT 'O''Reilly'" {
		t.Fatalf("execution: %v, %+v", err, session)
	}
	if len(c.schema) != 1 {
		t.Fatal("query mutated schema")
	}
}

func TestQueryFailuresDoNotExecute(t *testing.T) {
	for _, plan := range []string{
		`{"version":2,"dialect":"duckdb","sql":"SELECT 1"}`,
		`{"version":1,"dialect":"postgres","sql":"SELECT 1"}`,
		`{"version":1,"dialect":"duckdb","sql":" "}`,
		`{"version":1,"dialect":"duckdb","sql":"SELECT 1","transfers":[{}]}`,
	} {
		t.Run(plan, func(t *testing.T) {
			session := &testSession{}
			c := &Connection{session: session, dialect: "duckdb", command: func(_ any, r any) error { return json.Unmarshal([]byte(plan), r) }}
			if _, err := c.Query(context.Background(), "RETURN 1", QueryOptions{}); err == nil {
				t.Fatal("expected error")
			}
			if session.calls != 0 {
				t.Fatal("executed invalid plan")
			}
		})
	}
}

func TestCloseAndCancellation(t *testing.T) {
	session := &testSession{}
	c := &Connection{session: session, dialect: "duckdb", command: func(any, any) error { t.Fatal("unexpected compile"); return nil }}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := c.Query(ctx, "RETURN 1", QueryOptions{}); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if err := c.Close(); err != nil {
		t.Fatal(err)
	}
	_ = c.Close()
	if _, err := c.Query(context.Background(), "RETURN 1", QueryOptions{}); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestCancellationDuringCompilation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	session := &testSession{}
	c := &Connection{session: session, dialect: "duckdb", command: func(_ any, result any) error {
		cancel()
		return json.Unmarshal([]byte(`{"version":1,"dialect":"duckdb","sql":"SELECT 1"}`), result)
	}}
	if _, err := c.Query(ctx, "RETURN 1", QueryOptions{}); !errors.Is(err, context.Canceled) {
		t.Fatal(err)
	}
	if session.calls != 0 {
		t.Fatal("executed after cancellation")
	}
}

func TestNewValidation(t *testing.T) {
	var nilSession *testSession
	for _, session := range []Session{nil, nilSession} {
		if _, err := New(session, json.RawMessage(`{}`), Config{Dialect: "duckdb"}); err == nil {
			t.Fatal("accepted nil session")
		}
	}
	for _, config := range []Config{{Dialect: "unknown"}, {Dialect: "duckdb", Library: "/nonexistent/orchiddb-library"}} {
		if _, err := New(&testSession{}, json.RawMessage(`{}`), config); err == nil {
			t.Fatal("expected error")
		}
	}
	if _, err := New(&testSession{}, json.RawMessage(`{`), Config{Dialect: "duckdb"}); err == nil {
		t.Fatal("accepted invalid JSON")
	}
}
