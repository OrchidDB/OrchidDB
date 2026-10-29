package orchiddb

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func TestFederationValidatesEveryRouteBeforeSQL(t *testing.T) {
	session := &testSession{}
	c := &Connection{session: session, dialect: "duckdb", engines: map[string]Engine{"db": {Dialect: "duckdb", Session: session}}}
	plan := json.RawMessage(`{"version":1,"dialect":"duckdb","execution_engine":"db","sql":"FINAL","transfers":[{"source_engine":"db","source_dialect":"duckdb","sql":"SOURCE","target_relation":"r","columns":[],"request":{"engine":"missing","input_columns":[],"template":{"adapter":"custom_search"}}}]}`)
	if _, err := c.federate(context.Background(), plan); err == nil || !strings.Contains(err.Error(), "missing engine") {
		t.Fatalf("route not rejected: %v", err)
	}
	if session.calls != 0 {
		t.Fatal("SQL executed before route validation")
	}
}
func TestFederationBindsClosedRequestsTogetherAndUsesCurrentPlan(t *testing.T) {
	calls := 0
	remote := &testRequestEngine{command: func(v any, result any) error {
		r := v.(map[string]any)
		if r["op"] != "execute" {
			t.Fatal(r)
		}
		var requests []any
		if err := json.Unmarshal(r["requests"].(json.RawMessage), &requests); err != nil {
			t.Fatal(err)
		}
		if len(requests) != 2 {
			t.Fatalf("requests were not batched: %v", requests)
		}
		calls++
		return json.Unmarshal([]byte(`{"rows":[["9007199254740993"],["9007199254740995"]]}`), result)
	}}
	c := &Connection{session: &testSession{}, dialect: "duckdb", engines: map[string]Engine{"search": {Request: remote}}}
	plan := json.RawMessage(`{"version":1,"dialect":"duckdb","execution_engine":"db","sql":"UNBOUND","transfers":[{"source_engine":"search","source_dialect":"custom_search","sql":"","target_relation":"r","columns":[{"name":"id","data_type":"int64","nullable":false}],"request":{"engine":"search","input_columns":[],"template":{"adapter":"custom_search"}}}]}`)
	c.command = func(v any, result any) error {
		r := v.(map[string]any)
		switch r["op"] {
		case "bind_operation":
			if string(r["rows"].(json.RawMessage)) != "[[]]" {
				t.Fatal("closed request opened SQL")
			}
			return json.Unmarshal([]byte(`{"engine":"search","requests":[{"a":1},{"a":2}]}`), result)
		case "bind":
			if !strings.Contains(string(r["rows"].(json.RawMessage)), "9007199254740993") {
				t.Fatal("lost integer precision")
			}
			return json.Unmarshal([]byte(`{"version":1,"dialect":"duckdb","execution_engine":"db","sql":"BOUND","transfers":[]}`), result)
		default:
			t.Fatal(r)
			return nil
		}
	}
	sql, err := c.federate(context.Background(), plan)
	if err != nil || sql != "BOUND" || calls != 1 {
		t.Fatalf("sql=%s requests=%d err=%v", sql, calls, err)
	}
}

// The extension owns transport; this test supplies a deterministic implementation.
type testRequestEngine struct{ command commandFunc }

func (r *testRequestEngine) Adapter() string { return "custom_search" }
func (r *testRequestEngine) ExecuteRequests(ctx context.Context, requests json.RawMessage, columns json.RawMessage) (json.RawMessage, error) {
	var result struct {
		Rows json.RawMessage `json:"rows"`
	}
	err := r.command(map[string]any{"op": "execute", "requests": requests, "columns": columns}, &result)
	return result.Rows, err
}
