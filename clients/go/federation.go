package orchiddb

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"sync"
	"time"
)

// Engine borrows either a SQL session or a RemoteEngine (exactly one).
// Dialect is required for SQL sessions; remote adapters supply their own dialect.
type Engine struct {
	Dialect string
	Session Session
	Remote  *RemoteEngine
}

// RemoteEngine uses the shared typed HTTP transport, including batched Weaviate
// function calls. Options include endpoint, authentication and batch_size.
// Native HTTP calls use request_timeout_ms; contexts are checked between calls.
type RemoteEngine struct {
	mu          sync.Mutex
	command     commandFunc
	id, adapter string
}

func NewRemoteEngine(adapter string, options map[string]any, library string) (*RemoteEngine, error) {
	command, err := loadRemoteRuntime(library)
	if err != nil {
		return nil, err
	}
	var opened struct {
		ID string `json:"id"`
	}
	if err = command(map[string]any{"op": "open", "adapter": adapter, "options": options}, &opened); err != nil {
		return nil, err
	}
	if opened.ID == "" {
		return nil, errors.New("orchiddb: missing remote handle")
	}
	return &RemoteEngine{command: command, id: opened.ID, adapter: adapter}, nil
}
func (r *RemoteEngine) call(op string, values map[string]any, result any) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.id == "" {
		return ErrClosed
	}
	values["op"], values["id"] = op, r.id
	return r.command(values, result)
}
func (r *RemoteEngine) ClearMetadataCache() error {
	var v json.RawMessage
	return r.call("clear_metadata_cache", map[string]any{}, &v)
}
func (r *RemoteEngine) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.id == "" {
		return nil
	}
	var v json.RawMessage
	if err := r.command(map[string]any{"op": "close", "id": r.id}, &v); err != nil {
		return err
	}
	r.id = ""
	return nil
}
func (r *RemoteEngine) execute(ctx context.Context, requests json.RawMessage, columns []exchangeColumn) (json.RawMessage, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	var result struct {
		Rows json.RawMessage `json:"rows"`
	}
	err := r.call("execute", map[string]any{"requests": requests, "columns": columns, "format": "rows"}, &result)
	if err != nil {
		return nil, err
	}
	if err = ctx.Err(); err != nil {
		return nil, err
	}
	return result.Rows, nil
}

type exchangeColumn struct {
	Name     string          `json:"name"`
	DataType json.RawMessage `json:"data_type"`
	Nullable bool            `json:"nullable"`
}
type dependentOperation struct {
	Engine       string           `json:"engine"`
	InputColumns []exchangeColumn `json:"input_columns"`
	Template     struct {
		Adapter string `json:"adapter"`
		Dialect string `json:"dialect"`
	} `json:"template"`
}
type transfer struct {
	SourceEngine  string              `json:"source_engine"`
	SourceDialect string              `json:"source_dialect"`
	SQL           string              `json:"sql"`
	Relation      string              `json:"target_relation"`
	Columns       []exchangeColumn    `json:"columns"`
	Request       *dependentOperation `json:"request"`
	Operation     *dependentOperation `json:"operation"`
}
type executionPlan struct {
	Version         int        `json:"version"`
	Dialect         string     `json:"dialect"`
	SQL             string     `json:"sql"`
	ExecutionEngine string     `json:"execution_engine"`
	Transfers       []transfer `json:"transfers"`
}

func (c *Connection) federate(ctx context.Context, raw json.RawMessage) (string, error) {
	var plan executionPlan
	if err := json.Unmarshal(raw, &plan); err != nil {
		return "", err
	}
	if plan.ExecutionEngine == "" {
		return "", errors.New("orchiddb: missing execution engine")
	}
	target := plan.ExecutionEngine
	engine := func(name, dialect string, remote bool) (Engine, error) {
		e, ok := c.engines[name]
		if name == target {
			if ok && (e.Remote != nil || e.Dialect != c.dialect || e.Session == nil) {
				return e, errors.New("orchiddb: execution engine must use the primary SQL session dialect")
			}
			e = Engine{Dialect: c.dialect, Session: c.session}
			ok = true
		}
		if !ok {
			return e, fmt.Errorf("orchiddb: missing engine %q", name)
		}
		if e.Remote != nil {
			if e.Session != nil || !remote || e.Remote.adapter != dialect {
				return e, fmt.Errorf("orchiddb: engine %q capability/dialect mismatch", name)
			}
		} else if e.Session == nil || (reflect.ValueOf(e.Session).Kind() == reflect.Ptr && reflect.ValueOf(e.Session).IsNil()) || remote || e.Dialect != dialect {
			return e, fmt.Errorf("orchiddb: engine %q capability/dialect mismatch", name)
		}
		return e, nil
	}
	// Validate all routes before acquiring a SQL cursor or making an HTTP request.
	for _, t := range plan.Transfers {
		op := t.Request
		if op == nil {
			op = t.Operation
		}
		if op == nil || t.SQL != "" {
			if _, err := engine(t.SourceEngine, t.SourceDialect, false); err != nil {
				return "", err
			}
		}
		if op != nil {
			d := op.Template.Dialect
			if t.Request != nil {
				d = op.Template.Adapter
			}
			if _, err := engine(op.Engine, d, t.Request != nil); err != nil {
				return "", err
			}
		}
	}
	for len(plan.Transfers) > 0 {
		if err := ctx.Err(); err != nil {
			return "", err
		}
		t := plan.Transfers[0]
		op := t.Request
		if op == nil {
			op = t.Operation
		}
		var rows json.RawMessage
		if op == nil {
			e, err := engine(t.SourceEngine, t.SourceDialect, false)
			if err != nil {
				return "", err
			}
			rows, err = collectRows(ctx, e.Session, t.SQL, t.Columns)
			if err != nil {
				return "", err
			}
		} else {
			input := json.RawMessage(`[[]]`)
			if t.SQL != "" {
				e, err := engine(t.SourceEngine, t.SourceDialect, false)
				if err != nil {
					return "", err
				}
				input, err = collectRows(ctx, e.Session, t.SQL, op.InputColumns)
				if err != nil {
					return "", err
				}
			}
			var bound struct {
				Engine   string          `json:"engine"`
				Dialect  string          `json:"dialect"`
				Requests json.RawMessage `json:"requests"`
				SQL      []string        `json:"sql"`
			}
			if err := c.command(map[string]any{"op": "bind_operation", "plan": raw, "relation": t.Relation, "rows": input}, &bound); err != nil {
				return "", err
			}
			if t.Request != nil {
				e, err := engine(bound.Engine, op.Template.Adapter, true)
				if err != nil {
					return "", err
				}
				rows, err = e.Remote.execute(ctx, bound.Requests, t.Columns)
				if err != nil {
					return "", err
				}
			} else {
				e, err := engine(bound.Engine, bound.Dialect, false)
				if err != nil {
					return "", err
				}
				all := []json.RawMessage{}
				for _, sql := range bound.SQL {
					part, err := collectRows(ctx, e.Session, sql, t.Columns)
					if err != nil {
						return "", err
					}
					var r []json.RawMessage
					if err = json.Unmarshal(part, &r); err != nil {
						return "", err
					}
					all = append(all, r...)
				}
				rows, err = json.Marshal(all)
				if err != nil {
					return "", err
				}
			}
		}
		var next json.RawMessage
		if err := c.command(map[string]any{"op": "bind", "plan": raw, "relation": t.Relation, "rows": rows}, &next); err != nil {
			return "", err
		}
		var updated executionPlan
		if err := json.Unmarshal(next, &updated); err != nil {
			return "", err
		}
		if updated.Version != 1 || updated.Dialect != c.dialect || updated.ExecutionEngine != target || len(updated.Transfers) >= len(plan.Transfers) {
			return "", errors.New("orchiddb: invalid bound execution plan")
		}
		raw, plan = next, updated
	}
	if strings.TrimSpace(plan.SQL) == "" {
		return "", errors.New("orchiddb: missing final SQL")
	}
	return plan.SQL, nil
}

// Close each source cursor before binding or executing another operation on a
// borrowed session. Raw JSON retains integers/decimals across dependent plans.
func collectRows(ctx context.Context, session Session, sql string, columns []exchangeColumn) (json.RawMessage, error) {
	rows, err := session.QueryContext(ctx, sql)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	names, err := rows.Columns()
	if err != nil {
		return nil, err
	}
	if len(names) != len(columns) {
		return nil, errors.New("orchiddb: exchange column count mismatch")
	}
	all := [][]any{}
	for rows.Next() {
		values := make([]any, len(names))
		dest := make([]any, len(names))
		for i := range values {
			dest[i] = &values[i]
		}
		if err = rows.Scan(dest...); err != nil {
			return nil, err
		}
		for i, value := range values {
			switch v := value.(type) {
			case []byte:
				var kind string
				_ = json.Unmarshal(columns[i].DataType, &kind)
				if kind == "binary" || kind == "large_binary" {
					values[i] = base64.StdEncoding.EncodeToString(v)
				} else {
					values[i] = string(v)
				}
			case time.Time:
				var kind string
				_ = json.Unmarshal(columns[i].DataType, &kind)
				if kind == "date32" || kind == "date" {
					values[i] = v.Format("2006-01-02")
				} else {
					values[i] = v.Format(time.RFC3339Nano)
				}
			case fmt.Stringer:
				values[i] = v.String()
			default:
				// Some database/sql drivers return decimal structs whose String
				// method has a pointer receiver. Preserve their exact text.
				if value != nil && reflect.TypeOf(value).Kind() == reflect.Struct {
					pointer := reflect.New(reflect.TypeOf(value))
					pointer.Elem().Set(reflect.ValueOf(value))
					if text, ok := pointer.Interface().(fmt.Stringer); ok {
						values[i] = text.String()
					}
				}
			}
		}
		all = append(all, values)
	}
	if err = rows.Err(); err != nil {
		return nil, err
	}
	if err = rows.Close(); err != nil {
		return nil, err
	}
	return json.Marshal(all)
}
