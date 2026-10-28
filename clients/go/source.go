package orchiddb

import (
	"bytes"
	"context"
	"encoding/gob"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"reflect"
	"strconv"
	"strings"
	"sync/atomic"
)

// RowCursor is the common cursor contract of database/sql and local UDF results.
// Closing it releases query resources, never the application-owned session.
type RowCursor interface {
	Columns() ([]string, error)
	Next() bool
	Scan(...any) error
	Err() error
	Close() error
}
type SourceColumn = exchangeColumn

type SQLSource struct {
	Dialect        string
	Query          func(context.Context, string, ...any) (RowCursor, error)
	Requests       func(context.Context, []json.RawMessage, []SourceColumn) (RowCursor, error)
	ConcurrencyKey string
	Concurrent     bool
}
type LocalFunction func(context.Context, []any) (any, error)
type DuckDBCompletion func(context.Context, string, map[string]RowCursor) (RowCursor, error)
type SourceConfig struct {
	Engine    string
	Library   string
	Sources   map[string]SQLSource
	Functions map[string]LocalFunction
	// Completion is invoked only when a plan has a residual DuckDB operation.
	Completion DuckDBCompletion
}
type SourceConnection struct {
	schema  map[string]json.RawMessage
	config  SourceConfig
	command commandFunc
	closed  atomic.Bool
}

func NewSourceConnection(schema json.RawMessage, config SourceConfig) (*SourceConnection, error) {
	if config.Engine != "none" && config.Engine != "duckdb" {
		return nil, errors.New("engine must be none or duckdb; register Trino as a source")
	}
	command, err := loadRuntime(config.Library)
	if err != nil {
		return nil, err
	}
	var validated map[string]json.RawMessage
	if err = command(map[string]any{"op": "validate_schema", "schema": schema}, &validated); err != nil {
		return nil, err
	}
	sources := make(map[string]SQLSource, len(config.Sources))
	for k, v := range config.Sources {
		sources[k] = v
	}
	config.Sources = sources
	functions := make(map[string]LocalFunction, len(config.Functions))
	for k, v := range config.Functions {
		functions[k] = v
	}
	config.Functions = functions
	return &SourceConnection{schema: validated, config: config, command: command}, nil
}
func (c *SourceConnection) Close() error { c.closed.Store(true); return nil }

type sourceExpr struct {
	Kind      string          `json:"kind"`
	Index     int             `json:"index"`
	IPC       []int           `json:"ipc"`
	JSON      json.RawMessage `json:"json"`
	DataType  string          `json:"data_type"`
	Input     *sourceExpr     `json:"input"`
	Function  string          `json:"function"`
	Arguments []sourceExpr    `json:"arguments"`
	Returns   string          `json:"returns"`
}
type sourceTemplate struct {
	Adapter    string          `json:"adapter"`
	Parameters int             `json:"parameters"`
	Request    json.RawMessage `json:"request"`
	Bindings   []struct {
		Pointer   string `json:"pointer"`
		Parameter int    `json:"parameter"`
		Encoding  string `json:"encoding"`
	} `json:"bindings"`
}
type sourceNode struct {
	Template    sourceTemplate   `json:"template"`
	Kind        string           `json:"kind"`
	Source      string           `json:"source"`
	Dialect     string           `json:"dialect"`
	SQL         string           `json:"sql"`
	Input       int              `json:"input"`
	OneRow      bool             `json:"one_row"`
	Columns     []exchangeColumn `json:"columns"`
	Expressions []sourceExpr     `json:"expressions"`
	Inputs      []struct {
		Name string `json:"name"`
		Node int    `json:"node"`
	} `json:"inputs"`
}
type sourcePlan struct {
	Version int          `json:"version"`
	Engine  string       `json:"engine"`
	Nodes   []sourceNode `json:"nodes"`
	Result  int          `json:"result"`
}

func (c *SourceConnection) Query(ctx context.Context, text string, options QueryOptions) (RowCursor, error) {
	if c.closed.Load() {
		return nil, ErrClosed
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	request := map[string]any{}
	for k, v := range c.schema {
		request[k] = v
	}
	language := options.Language
	if language == "" {
		language = "cypher"
	}
	request["version"] = 1
	request["dialect"] = "duckdb"
	request["engine"] = c.config.Engine
	request["query"] = text
	request["language"] = language
	if options.Parameters != nil {
		request["parameters"] = options.Parameters
	}
	if options.Authorization != nil {
		request["authorization"] = options.Authorization
	}
	var work struct {
		Version int        `json:"version"`
		Plan    sourcePlan `json:"execution_plan"`
	}
	if err := c.command(request, &work); err != nil {
		return nil, err
	}
	if work.Version != 2 {
		return nil, errors.New("expected source-plan protocol")
	}
	return c.execute(ctx, work.Plan)
}
func (c *SourceConnection) checkExpr(e sourceExpr, width int) error {
	switch e.Kind {
	case "column":
		if e.Index < 0 || e.Index >= width {
			return errors.New("column out of bounds")
		}
	case "literal":
		if len(e.JSON) == 0 {
			return errors.New("literal requires an Arrow-capable client")
		}
		_, err := decodeSourceLiteral(e)
		return err
	case "cast", "graph_element":
		if e.Input == nil {
			return errors.New("missing cast argument")
		}
		return c.checkExpr(*e.Input, width)
	case "call":
		if c.config.Functions[e.Function] == nil {
			return fmt.Errorf("missing local function %s", e.Function)
		}
		for _, a := range e.Arguments {
			if err := c.checkExpr(a, width); err != nil {
				return err
			}
		}
	default:
		return errors.New("unsupported local expression")
	}
	return nil
}
func decodeSourceLiteral(e sourceExpr) (any, error) {
	if string(e.JSON) == "null" {
		return nil, nil
	}
	switch e.DataType {
	case "int64":
		var v int64
		err := json.Unmarshal(e.JSON, &v)
		return v, err
	case "float64":
		var v float64
		err := json.Unmarshal(e.JSON, &v)
		return v, err
	case "string":
		var v string
		err := json.Unmarshal(e.JSON, &v)
		return v, err
	case "boolean":
		var v bool
		err := json.Unmarshal(e.JSON, &v)
		return v, err
	default:
		return nil, fmt.Errorf("unsupported row literal type %s; use a typed source", e.DataType)
	}
}
func (c *SourceConnection) eval(ctx context.Context, e sourceExpr, row []any) (any, error) {
	switch e.Kind {
	case "column":
		return row[e.Index], nil
	case "literal":
		return decodeSourceLiteral(e)
	case "cast":
		return c.eval(ctx, *e.Input, row)
	case "graph_element":
		value, err := c.eval(ctx, *e.Input, row)
		if err != nil {
			return nil, err
		}
		var decoded []any
		if err = c.command(map[string]any{"op": "decode_graph_values", "data_type": e.DataType, "values": []any{value}}, &decoded); err != nil {
			return nil, err
		}
		if len(decoded) != 1 {
			return nil, errors.New("invalid graph decoder row count")
		}
		return decoded[0], nil
	case "call":
		args := make([]any, len(e.Arguments))
		for i, a := range e.Arguments {
			v, err := c.eval(ctx, a, row)
			if err != nil {
				return nil, err
			}
			args[i] = v
		}
		value, err := c.config.Functions[e.Function](ctx, args)
		if err != nil {
			return nil, err
		}
		if err = validateLocalValue(value, e.Returns); err != nil {
			return nil, fmt.Errorf("local function %s: %w", e.Function, err)
		}
		return value, nil
	}
	return nil, errors.New("unsupported local expression")
}
func (c *SourceConnection) execute(ctx context.Context, plan sourcePlan) (RowCursor, error) {
	if plan.Version != 1 || plan.Result < 0 || plan.Result >= len(plan.Nodes) {
		return nil, errors.New("invalid source plan")
	}
	for i, n := range plan.Nodes {
		switch n.Kind {
		case "source_sql":
			s, ok := c.config.Sources[n.Source]
			if !ok || s.Dialect != n.Dialect || s.Query == nil {
				return nil, fmt.Errorf("missing SQL source %s", n.Source)
			}
		case "source_request":
			source, ok := c.config.Sources[n.Source]
			if !ok || source.Dialect != n.Template.Adapter || source.Requests == nil {
				return nil, fmt.Errorf("missing request source %s", n.Source)
			}
			if (n.Input >= 0 && n.Input >= i) || (n.Input < 0 && n.Template.Parameters > 0) {
				return nil, errors.New("invalid request dependency")
			}
		case "local_project":
			if n.Input < 0 || n.Input >= i {
				return nil, errors.New("invalid plan dependency")
			}
			for _, e := range n.Expressions {
				if err := c.checkExpr(e, len(plan.Nodes[n.Input].Columns)); err != nil {
					return nil, err
				}
			}
		case "duckdb":
			if c.config.Engine != "duckdb" || c.config.Completion == nil {
				return nil, errors.New("DuckDB completion binding required")
			}
			for _, r := range n.Inputs {
				if r.Node < 0 || r.Node >= i {
					return nil, errors.New("invalid completion dependency")
				}
			}
		case "input":
		default:
			return nil, fmt.Errorf("unsupported source-plan operation %s", n.Kind)
		}
	}
	outputs := make([]RowCursor, 0, len(plan.Nodes))
	for _, n := range plan.Nodes {
		n := n
		names := make([]string, len(n.Columns))
		for i, col := range n.Columns {
			names[i] = col.Name
		}
		var cursor RowCursor
		switch n.Kind {
		case "source_sql":
			cursor = &deferredSourceRows{columns: names, open: func() (RowCursor, error) { return c.config.Sources[n.Source].Query(ctx, n.SQL) }}
		case "duckdb":
			cursor = &deferredSourceRows{columns: names, open: func() (RowCursor, error) {
				inputs := map[string]RowCursor{}
				for _, r := range n.Inputs {
					inputs[r.Name] = outputs[r.Node]
				}
				return c.config.Completion(ctx, n.SQL, inputs)
			}}
		case "source_request":
			var input RowCursor
			if n.Input >= 0 {
				input = outputs[n.Input]
			}
			var current RowCursor
			once := false
			cursor = &localRows{columns: names, next: func() ([]any, bool, error) {
				for {
					if current != nil && current.Next() {
						row, err := readSourceRow(current)
						return row, err == nil, err
					}
					if current != nil {
						err := current.Err()
						closeErr := current.Close()
						current = nil
						if err != nil {
							return nil, false, err
						}
						if closeErr != nil {
							return nil, false, closeErr
						}
					}
					var requests []json.RawMessage
					if input == nil {
						if once {
							return nil, false, nil
						}
						once = true
						payload, err := bindSourceRequest(n.Template, nil)
						if err != nil {
							return nil, false, err
						}
						requests = append(requests, payload)
					} else {
						for count := 0; count < 64 && input.Next(); count++ {
							row, err := readSourceRow(input)
							if err != nil {
								return nil, false, err
							}
							payload, err := bindSourceRequest(n.Template, row)
							if err != nil {
								return nil, false, err
							}
							requests = append(requests, payload)
						}
						if len(requests) == 0 {
							return nil, false, input.Err()
						}
					}
					var err error
					current, err = c.config.Sources[n.Source].Requests(ctx, requests, n.Columns)
					if err != nil {
						return nil, false, err
					}
				}
			}, cleanup: func() error {
				if current != nil {
					return current.Close()
				}
				return nil
			}}
		case "input":
			once := n.OneRow
			cursor = &localRows{columns: names, next: func() ([]any, bool, error) {
				if !once {
					return nil, false, nil
				}
				once = false
				return []any{}, true, nil
			}}
		case "local_project":
			input := outputs[n.Input]
			cursor = &localRows{columns: names, next: func() ([]any, bool, error) {
				if err := ctx.Err(); err != nil {
					return nil, false, err
				}
				if !input.Next() {
					return nil, false, input.Err()
				}
				columns, err := input.Columns()
				if err != nil {
					return nil, false, err
				}
				row := make([]any, len(columns))
				dest := make([]any, len(columns))
				for i := range row {
					dest[i] = &row[i]
				}
				if err = input.Scan(dest...); err != nil {
					return nil, false, err
				}
				result := make([]any, len(n.Expressions))
				for i, e := range n.Expressions {
					result[i], err = c.eval(ctx, e, row)
					if err != nil {
						return nil, false, err
					}
				}
				return result, true, nil
			}}
		}
		id := len(outputs)
		reentry := false
		for _, later := range plan.Nodes[id+1:] {
			if n.Kind == "duckdb" && later.Kind == "duckdb" {
				reentry = true
			}
			source := c.config.Sources[n.Source]
			if n.Source != "" && later.Source != "" && !source.Concurrent && (n.Source == later.Source || (source.ConcurrencyKey != "" && source.ConcurrencyKey == c.config.Sources[later.Source].ConcurrencyKey)) {
				reentry = true
			}
		}
		if reentry {
			cursor = spoolSourceRows(cursor)
		}
		outputs = append(outputs, cursor)
	}
	return &ownedSourceRows{RowCursor: outputs[plan.Result], all: outputs}, nil
}

type deferredSourceRows struct {
	columns []string
	open    func() (RowCursor, error)
	rows    RowCursor
	err     error
	closed  bool
}

func (r *deferredSourceRows) Columns() ([]string, error) {
	return append([]string(nil), r.columns...), nil
}
func (r *deferredSourceRows) Next() bool {
	if r.closed || r.err != nil {
		return false
	}
	if r.rows == nil {
		r.rows, r.err = r.open()
		// database/sql returns a typed nil *Rows on failure. Clear that
		// interface before cleanup so the original source error is retained.
		if r.rows != nil && reflect.ValueOf(r.rows).Kind() == reflect.Ptr && reflect.ValueOf(r.rows).IsNil() {
			r.rows = nil
		}
		if r.err == nil && r.rows == nil {
			r.err = errors.New("source returned a nil cursor")
		}
		if r.err != nil {
			return false
		}
	}
	return r.rows.Next()
}
func (r *deferredSourceRows) Scan(v ...any) error {
	if r.rows == nil || r.closed {
		return errors.New("cursor not positioned")
	}
	return r.rows.Scan(v...)
}
func (r *deferredSourceRows) Err() error {
	if r.err != nil {
		return r.err
	}
	if r.rows != nil {
		return r.rows.Err()
	}
	return nil
}
func (r *deferredSourceRows) Close() error {
	if r.closed {
		return nil
	}
	r.closed = true
	if r.rows != nil {
		return r.rows.Close()
	}
	return nil
}

type localRows struct {
	cleanup func() error
	columns []string
	next    func() ([]any, bool, error)
	row     []any
	err     error
	closed  bool
}

func (r *localRows) Columns() ([]string, error) { return append([]string(nil), r.columns...), nil }
func (r *localRows) Next() bool {
	if r.closed || r.err != nil {
		return false
	}
	var ok bool
	r.row, ok, r.err = r.next()
	return ok && r.err == nil
}
func (r *localRows) Scan(dest ...any) error {
	if r.closed || r.row == nil || len(dest) != len(r.row) {
		return errors.New("cursor not positioned or scan width mismatch")
	}
	for i, d := range dest {
		if scanner, ok := d.(interface{ Scan(any) error }); ok {
			if err := scanner.Scan(r.row[i]); err != nil {
				return err
			}
			continue
		}
		v := reflect.ValueOf(d)
		if v.Kind() != reflect.Ptr || v.IsNil() {
			return errors.New("scan destination must be a pointer")
		}
		if r.row[i] == nil {
			switch v.Elem().Kind() {
			case reflect.Interface, reflect.Ptr, reflect.Slice, reflect.Map:
				v.Elem().SetZero()
			default:
				return fmt.Errorf("cannot scan NULL into %T", d)
			}
			continue
		}
		value := reflect.ValueOf(r.row[i])
		if !value.Type().AssignableTo(v.Elem().Type()) {
			return fmt.Errorf("cannot scan %T into %T", r.row[i], d)
		}
		v.Elem().Set(value)
	}
	return nil
}
func (r *localRows) Err() error { return r.err }
func (r *localRows) Close() error {
	if r.closed {
		return nil
	}
	r.closed = true
	r.row = nil
	if r.cleanup != nil {
		return r.cleanup()
	}
	return nil
}

type ownedSourceRows struct {
	RowCursor
	all    []RowCursor
	closed bool
	err    error
}

func (r *ownedSourceRows) Next() bool {
	if r.closed {
		return false
	}
	if r.RowCursor.Next() {
		return true
	}
	r.err = r.RowCursor.Err()
	_ = r.Close()
	return false
}
func (r *ownedSourceRows) Err() error {
	if r.err != nil {
		return r.err
	}
	return r.RowCursor.Err()
}
func (r *ownedSourceRows) Close() error {
	if r.closed {
		return nil
	}
	r.closed = true
	var errs []error
	for i := len(r.all) - 1; i >= 0; i-- {
		if err := r.all[i].Close(); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

func readSourceRow(rows RowCursor) ([]any, error) {
	columns, err := rows.Columns()
	if err != nil {
		return nil, err
	}
	values := make([]any, len(columns))
	dest := make([]any, len(columns))
	for i := range values {
		dest[i] = &values[i]
	}
	err = rows.Scan(dest...)
	return values, err
}
func bindSourceRequest(template sourceTemplate, row []any) (json.RawMessage, error) {
	if len(row) != template.Parameters {
		return nil, errors.New("request parameter count mismatch")
	}
	var request any
	decoder := json.NewDecoder(bytes.NewReader(template.Request))
	decoder.UseNumber()
	if err := decoder.Decode(&request); err != nil {
		return nil, err
	}
	for _, binding := range template.Bindings {
		if binding.Parameter < 0 || binding.Parameter >= len(row) {
			return nil, errors.New("request parameter out of bounds")
		}
		value := row[binding.Parameter]
		if binding.Encoding == "sql_null" {
			value = value == nil
		}
		if binding.Pointer == "" {
			request = value
			continue
		}
		parts := strings.Split(strings.TrimPrefix(binding.Pointer, "/"), "/")
		for i := range parts {
			parts[i] = strings.ReplaceAll(strings.ReplaceAll(parts[i], "~1", "/"), "~0", "~")
		}
		var set func(any, []string) (any, error)
		set = func(target any, path []string) (any, error) {
			if len(path) == 0 {
				return value, nil
			}
			switch t := target.(type) {
			case map[string]any:
				old, ok := t[path[0]]
				if !ok {
					return nil, errors.New("invalid request pointer")
				}
				v, err := set(old, path[1:])
				t[path[0]] = v
				return t, err
			case []any:
				i, err := strconv.Atoi(path[0])
				if err != nil || i < 0 || i >= len(t) {
					return nil, errors.New("invalid request index")
				}
				v, err := set(t[i], path[1:])
				t[i] = v
				return t, err
			default:
				return nil, errors.New("invalid request pointer")
			}
		}
		var err error
		request, err = set(request, parts)
		if err != nil {
			return nil, err
		}
	}
	return json.Marshal(request)
}
func spoolSourceRows(input RowCursor) RowCursor {
	columns, _ := input.Columns()
	var file *os.File
	var decoder *gob.Decoder
	return &localRows{columns: columns, next: func() ([]any, bool, error) {
		if file == nil {
			var err error
			file, err = os.CreateTemp("", "orchid-rows-*")
			if err != nil {
				return nil, false, err
			}
			encoder := gob.NewEncoder(file)
			for input.Next() {
				row, err := readSourceRow(input)
				if err != nil {
					return nil, false, err
				}
				if err = encoder.Encode(row); err != nil {
					return nil, false, err
				}
			}
			if err = input.Err(); err != nil {
				return nil, false, err
			}
			if err = input.Close(); err != nil {
				return nil, false, err
			}
			if _, err = file.Seek(0, 0); err != nil {
				return nil, false, err
			}
			decoder = gob.NewDecoder(file)
		}
		var row []any
		err := decoder.Decode(&row)
		if err == io.EOF {
			return nil, false, nil
		}
		return row, err == nil, err
	}, cleanup: func() error {
		err := input.Close()
		if file != nil {
			name := file.Name()
			err = errors.Join(err, file.Close(), os.Remove(name))
		}
		return err
	}}
}

func (n *sourceNode) UnmarshalJSON(data []byte) error {
	type plain sourceNode
	value := struct {
		*plain
		Input *int `json:"input"`
	}{plain: (*plain)(n)}
	if err := json.Unmarshal(data, &value); err != nil {
		return err
	}
	n.Input = -1
	if value.Input != nil {
		n.Input = *value.Input
	}
	return nil
}

func validateLocalValue(value any, kind string) error {
	if value == nil {
		return nil
	}
	valid := true
	switch kind {
	case "int64":
		_, valid = value.(int64)
	case "int32":
		_, valid = value.(int32)
	case "float64":
		_, valid = value.(float64)
	case "float32":
		_, valid = value.(float32)
	case "boolean":
		_, valid = value.(bool)
	case "string":
		_, valid = value.(string)
	case "binary":
		_, valid = value.([]byte)
	default:
		if strings.HasPrefix(kind, "list<") && strings.HasSuffix(kind, ">") {
			list := reflect.ValueOf(value)
			if list.Kind() != reflect.Slice && list.Kind() != reflect.Array {
				return fmt.Errorf("expected %s, got %T", kind, value)
			}
			for i := 0; i < list.Len(); i++ {
				if err := validateLocalValue(list.Index(i).Interface(), kind[5:len(kind)-1]); err != nil {
					return err
				}
			}
		}
	}
	if !valid {
		return fmt.Errorf("expected %s, got %T", kind, value)
	}
	return nil
}
