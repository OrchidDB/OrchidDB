package orchiddb

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"testing"
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
	return sourcePlan{Version: 1, Engine: "none", Nodes: nodes, Result: len(nodes) - 1}
}
func TestSourceFirstNoCompletion(t *testing.T) {
	for _, engine := range []string{"none", "duckdb"} {
		t.Run(engine, func(t *testing.T) {
			calls := 0
			closed := 0
			c := &SourceConnection{config: SourceConfig{Engine: engine, Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(_ context.Context, sql string, _ ...any) (RowCursor, error) {
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
			rows, err := c.execute(context.Background(), sourceFixture(true))
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
			if rows.Err() != nil || len(got) != 2 || got[0] != 2 || got[1] != 4 || calls != 1 || closed != 1 {
				t.Fatalf("%v calls=%d closed=%d err=%v", got, calls, closed, rows.Err())
			}
		})
	}
}
func TestSourceFirstPreflight(t *testing.T) {
	calls := 0
	c := &SourceConnection{config: SourceConfig{Engine: "none", Sources: map[string]SQLSource{"db": {Dialect: "postgres", Query: func(context.Context, string, ...any) (RowCursor, error) { calls++; return nil, nil }}}}}
	if _, err := c.execute(context.Background(), sourceFixture(true)); err == nil {
		t.Fatal("missing UDF accepted")
	}
	if calls != 0 {
		t.Fatal("source called before validation")
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
