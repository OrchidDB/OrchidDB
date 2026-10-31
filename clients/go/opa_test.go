package orchiddb

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"os/exec"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

func TestOPANativeRowFiltersMasksAndDenial(t *testing.T) {
	library := os.Getenv("ORCHIDDB_NATIVE_LIBRARY")
	if library == "" {
		t.Skip("set ORCHIDDB_NATIVE_LIBRARY to test the real runtime")
	}
	var denied atomic.Bool
	var calls atomic.Int64
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		var input struct {
			Input struct {
				Context struct{ Identity struct{ User string } }
			}
		}
		if err := json.NewDecoder(r.Body).Decode(&input); err != nil {
			t.Error(err)
		}
		if input.Input.Context.Identity.User != "alice" {
			t.Errorf("untrusted identity: %s", input.Input.Context.Identity.User)
		}
		result := `{"result":true}`
		switch r.URL.Path {
		case "/rows":
			result = `{"result":[{"expression":"tenant = 'a'"}]}`
		case "/mask":
			result = `{"result":{"expression":"0"}}`
		default:
			if denied.Load() {
				result = `{"result":false}`
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(result))
	}))
	defer server.Close()
	config := &OPAConfig{PolicyURI: server.URL + "/allow", RowFiltersURI: server.URL + "/rows", ColumnMaskingURI: server.URL + "/mask", User: "alice"}
	schema := json.RawMessage(`{"tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"salary","data_type":"int64"},{"name":"tenant","data_type":"string"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"salary":"salary"}}]}`)
	session := &testSession{}
	conn, err := New(session, schema, Config{Dialect: "duckdb", Library: library, OPA: config})
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	config.User = "admin" // The connection owns a copy of trusted configuration.
	_, err = conn.Query(context.Background(), "MATCH (p:Person) RETURN p.salary AS salary", QueryOptions{Parameters: map[string]any{"user": "admin"}})
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(session.query, "tenant") || !strings.Contains(session.query, "0") {
		t.Fatalf("missing policy in SQL: %s", session.query)
	}
	duckdb, err := exec.LookPath("duckdb")
	if err != nil {
		t.Fatal("DuckDB CLI required for OPA row enforcement test")
	}
	sql := "CREATE TABLE people(id BIGINT, salary BIGINT, tenant VARCHAR); INSERT INTO people VALUES (1,10,'a'),(2,20,'b'),(3,30,'a'); " + session.query
	out, err := exec.Command(duckdb, "-json", ":memory:", sql).CombinedOutput()
	if err != nil {
		t.Fatalf("SQL execution: %v: %s", err, out)
	}
	var rows []map[string]any
	if err = json.Unmarshal(out, &rows); err != nil {
		t.Fatal(err)
	}
	if len(rows) != 2 || rows[0]["salary"] != float64(0) || rows[1]["salary"] != float64(0) {
		t.Fatalf("policy result: %s", out)
	}
	// Shared native policy and HTTP pool remain safe under concurrent queries.
	var queries sync.WaitGroup
	for i := 0; i < 8; i++ {
		queries.Add(1)
		go func() {
			defer queries.Done()
			var plan any
			if err := compileOPA(conn.command, conn.opa, map[string]any{
				"version": 1, "dialect": "duckdb", "language": "cypher", "query": "RETURN 1 AS x",
			}, &plan); err != nil {
				t.Error(err)
			}
		}()
	}
	queries.Wait()
	denied.Store(true)
	before := session.calls
	for i := 0; i < 2; i++ {
		if _, err = conn.Query(context.Background(), "MATCH (p:Person) RETURN p.salary", QueryOptions{}); err == nil {
			t.Fatal("denial accepted")
		}
	}
	if session.calls != before {
		t.Fatal("denied query reached database")
	}
	if err = conn.Close(); err != nil {
		t.Fatal(err)
	}
	if err = conn.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err = conn.Query(context.Background(), "RETURN 1", QueryOptions{}); err != ErrClosed {
		t.Fatalf("closed connection: %v", err)
	}
	var result any
	if err = compileOPA(conn.command, conn.opa, map[string]any{"version": 1, "dialect": "duckdb", "query": "RETURN 1"}, &result); err == nil {
		t.Fatal("closed native policy accepted")
	}
	// Source connections enforce the same policy before opening a source.
	config.User = "alice"
	source, err := NewSourceConnection(schema, SourceConfig{Library: library, OPA: config})
	if err != nil {
		t.Fatal(err)
	}
	defer source.Close()
	if _, err = source.Query(context.Background(), "MATCH (p:Person) RETURN p.salary", QueryOptions{}); err == nil || !strings.Contains(err.Error(), "access denied") {
		t.Fatalf("source policy: %v", err)
	}
	if calls.Load() < 6 {
		t.Fatal("missing OPA requests")
	}
}
