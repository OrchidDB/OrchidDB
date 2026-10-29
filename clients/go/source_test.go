package orchiddb

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

func TestSourceOpenErrorWithTypedNilCursor(t *testing.T) {
	want := errors.New("source unavailable")
	rows := &deferredSourceRows{open: func() (RowCursor, error) { return (*sql.Rows)(nil), want }}
	if rows.Next() || !errors.Is(rows.Err(), want) {
		t.Fatalf("source error lost: %v", rows.Err())
	}
	if err := rows.Close(); err != nil {
		t.Fatal(err)
	}
}

func sourceFixture(local bool) sourcePlan {
	col := exchangeColumn{Name: "id", DataType: json.RawMessage(`"int64"`), Nullable: true}
	nodes := []sourceNode{{Kind: "source_sql", Source: "db", Dialect: "postgres", SQL: "SELECT id FROM people", Columns: []exchangeColumn{col}}}
	if local {
		nodes = append(nodes, sourceNode{Kind: "local_project", Input: 0, Columns: []exchangeColumn{col}, Expressions: []sourceExpr{{Kind: "call", Function: "twice", Returns: "int64", Arguments: []sourceExpr{{Kind: "column", Index: 0}}}}})
	}
	return sourcePlan{Version: 1, Nodes: nodes, Result: len(nodes) - 1}
}
func TestSourceFirstNoCompletion(t *testing.T) {
	for _, local := range []bool{false, true} {
		t.Run(fmt.Sprintf("local=%v", local), func(t *testing.T) {
			calls := 0
			closed := 0
			c := &SourceConnection{config: SourceConfig{Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(_ context.Context, sql string, _ ...any) (RowCursor, error) {
				calls++
				if sql != "SELECT id FROM people" {
					t.Fatal(sql)
				}
				i := 0
				return &localRows{columns: []string{"id"}, next: func() ([]any, bool, error) { i++; return []any{int64(i)}, i <= 2, nil }, cleanup: func() error { closed++; return nil }}, nil
			}}}, Functions: map[string]LocalFunction{"twice": func(_ context.Context, args []any) (any, error) { return args[0].(int64) * 2, nil }}, Completion: func(context.Context, string, map[string]RowCursor) (RowCursor, error) {
				t.Fatal("DuckDB called")
				return nil, errors.New("unexpected engine")
			}}}
			rows, err := c.execute(context.Background(), sourceFixture(local))
			if err != nil {
				t.Fatal(err)
			}
			var got []int64
			for rows.Next() {
				var value int64
				if err := rows.Scan(&value); err != nil {
					t.Fatal(err)
				}
				got = append(got, value)
			}
			want := int64(1)
			if local {
				want = 2
			}
			if rows.Err() != nil || len(got) != 2 || got[0] != want || got[1] != want*2 || calls != 1 || closed != 1 {
				t.Fatalf("%v calls=%d closed=%d err=%v", got, calls, closed, rows.Err())
			}
		})
	}
}
func TestSourceFirstPreflight(t *testing.T) {
	calls := 0
	c := &SourceConnection{config: SourceConfig{Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(context.Context, string, ...any) (RowCursor, error) { calls++; return nil, nil }}}}}
	if _, err := c.execute(context.Background(), sourceFixture(true)); err == nil {
		t.Fatal("missing UDF accepted")
	}
	if calls != 0 {
		t.Fatal("source called before validation")
	}
}

func TestSourceConnectionWithNativeRuntime(t *testing.T) {
	library := os.Getenv("ORCHIDDB_NATIVE_LIBRARY")
	if library == "" {
		t.Skip("set ORCHIDDB_NATIVE_LIBRARY to test the real runtime")
	}
	schema := json.RawMessage(`{"sources":{"db":{"adapter":"postgres"}},
		"tables":[{"name":"people","source":"db","columns":[{"name":"id","data_type":"int64"}]}],
		"nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id"}}],
		"functions":[{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]}`)
	sourceCalls, functionCalls := 0, 0
	graph, err := NewSourceConnection(schema, SourceConfig{
		Library: library,
		Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(_ context.Context, sql string, _ ...any) (RowCursor, error) {
			sourceCalls++
			if strings.Contains(sql, "twice") {
				t.Fatal("local function sent to source")
			}
			i := 0
			return &localRows{columns: []string{"id"}, next: func() ([]any, bool, error) { i++; return []any{int64(i)}, i <= 2, nil }}, nil
		}}},
		Functions: map[string]LocalFunction{"twice": func(_ context.Context, args []any) (any, error) {
			functionCalls++
			return args[0].(int64) * 2, nil
		}},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer graph.Close()
	// A residual filter selects DuckDB automatically; missing bindings fail before effects.
	if _, err := graph.Query(context.Background(), "MATCH (p:Person) WHERE twice(p.id) > 2 RETURN p.id", QueryOptions{}); err == nil || !strings.Contains(err.Error(), "DuckDB") {
		t.Fatalf("expected missing completion binding: %v", err)
	}
	if sourceCalls != 0 || functionCalls != 0 {
		t.Fatal("callbacks ran before completion preflight")
	}
	rows, err := graph.Query(context.Background(), "MATCH (p:Person) RETURN p.id AS id", QueryOptions{})
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var got []int64
	for rows.Next() {
		var n int64
		if err := rows.Scan(&n); err != nil {
			t.Fatal(err)
		}
		got = append(got, n)
	}
	if rows.Err() != nil || len(got) != 2 || got[0] != 1 || got[1] != 2 || sourceCalls != 1 || functionCalls != 0 {
		t.Fatalf("values=%v source=%d function=%d error=%v", got, sourceCalls, functionCalls, rows.Err())
	}
	local, err := graph.Query(context.Background(), "RETURN twice($n) AS answer", QueryOptions{Parameters: map[string]any{"n": 21}})
	if err != nil {
		t.Fatal(err)
	}
	defer local.Close()
	var answer int64
	if !local.Next() {
		t.Fatalf("missing local result: %v", local.Err())
	}
	if err := local.Scan(&answer); err != nil {
		t.Fatal(err)
	}
	if answer != 42 || local.Next() || local.Err() != nil || sourceCalls != 1 || functionCalls != 1 {
		t.Fatalf("answer=%d source=%d function=%d error=%v", answer, sourceCalls, functionCalls, local.Err())
	}
}
func TestSourceRequestBinding(t *testing.T) {
	var template sourceTemplate
	if err := json.Unmarshal([]byte(`{"adapter":"test","parameters":1,"request":{"ids":[null]},"bindings":[{"pointer":"/ids/0","parameter":0}]}`), &template); err != nil {
		t.Fatal(err)
	}
	value, err := bindSourceRequest(template, []any{int64(9007199254740993)})
	if err != nil {
		t.Fatal(err)
	}
	if string(value) != `{"ids":[9007199254740993]}` {
		t.Fatal(string(value))
	}
}

func TestLocalRowsPreserveNull(t *testing.T) {
	rows := &localRows{row: []any{nil}}
	var value int64
	if rows.Scan(&value) == nil {
		t.Fatal("NULL silently converted to integer zero")
	}
	var nullable any
	if err := rows.Scan(&nullable); err != nil || nullable != nil {
		t.Fatalf("NULL lost: %v %v", nullable, err)
	}
}

type privateDriverRow struct{ values []any }

func (r privateDriverRow) Len() int                { return len(r.values) }
func (r privateDriverRow) Value(i int) interface{} { return r.values[i] }
func TestGraphDescriptorUsesDriverRowFields(t *testing.T) {
	value := graphDescriptor(privateDriverRow{values: []any{"opaque é", privateDriverRow{values: []any{int64(9007199254740993)}}}})
	encoded, err := json.Marshal(value)
	if err != nil || string(encoded) != `["opaque é",[9007199254740993]]` {
		t.Fatalf("lost ROW fields: %s %v", encoded, err)
	}
	if graphDescriptor(privateDriverRow{}) != nil {
		t.Fatal("null ROW lost")
	}
}

func TestSourceColumnsPreserveEmptyCompletionInputs(t *testing.T) {
	opened := 0
	col := exchangeColumn{Name: "value", DataType: json.RawMessage(`"list:timestamptz"`), Nullable: true}
	plan := sourcePlan{Version: 1, Result: 1, Nodes: []sourceNode{
		{Kind: "source_sql", Source: "db", Dialect: "postgres", SQL: "SELECT value FROM empty", Columns: []exchangeColumn{col}},
		{Kind: "duckdb", SQL: "SELECT * FROM incoming", Columns: []exchangeColumn{col}},
	}}
	plan.Nodes[1].Inputs = append(plan.Nodes[1].Inputs, struct {
		Name string `json:"name"`
		Node int    `json:"node"`
	}{"incoming", 0})
	c := &SourceConnection{config: SourceConfig{Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(context.Context, string, ...any) (RowCursor, error) {
		opened++
		return &localRows{columns: []string{"value"}, next: func() ([]any, bool, error) { return nil, false, nil }}, nil
	}}}, Completion: func(_ context.Context, _ string, inputs map[string]RowCursor) (RowCursor, error) {
		cols, err := SourceColumns(inputs["incoming"])
		if err != nil || len(cols) != 1 || string(cols[0].DataType) != `"list:timestamptz"` || !cols[0].Nullable || opened != 0 {
			t.Fatalf("empty input schema lost or opened early: %+v, %v, opened=%d", cols, err, opened)
		}
		return inputs["incoming"], nil
	}}}
	rows, err := c.execute(context.Background(), plan)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	cols, err := SourceColumns(rows)
	if err != nil || cols[0].Name != "value" {
		t.Fatal(cols, err)
	}
	cols[0].DataType[1] = 'X'
	original, _ := SourceColumns(rows)
	if string(original[0].DataType) != `"list:timestamptz"` {
		t.Fatal("schema aliased caller memory")
	}
	if rows.Next() || rows.Err() != nil || opened != 1 {
		t.Fatal("empty cursor failed", rows.Err(), opened)
	}
}

func TestSourceSpoolPreservesTemporalAndNestedValues(t *testing.T) {
	stamp := time.Date(2026, 10, 29, 1, 2, 3, 123456000, time.UTC)
	expected := []any{stamp, []any{nil, stamp}, map[string]any{"exact": json.Number("9007199254740993")}}
	first := true
	input := &localRows{columns: []string{"stamp", "list", "map"}, next: func() ([]any, bool, error) {
		if !first {
			return nil, false, nil
		}
		first = false
		return expected, true, nil
	}}
	rows := spoolSourceRows(input)
	defer rows.Close()
	if !rows.Next() {
		t.Fatal(rows.Err())
	}
	actual, err := readSourceRow(rows)
	if err != nil || !reflect.DeepEqual(actual, expected) {
		t.Fatalf("spool changed typed values: %#v, %v", actual, err)
	}
	if rows.Next() || rows.Err() != nil {
		t.Fatal(rows.Err())
	}
}
