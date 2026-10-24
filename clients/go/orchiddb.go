// Package orchiddb executes graph queries on application-owned database/sql sessions.
package orchiddb

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"sync/atomic"
)

// Session is implemented by *sql.DB, *sql.Conn, and *sql.Tx. Use a Conn or Tx
// when queries must share session state or an existing transaction.
type Session interface {
	QueryContext(context.Context, string, ...any) (*sql.Rows, error)
}

// Config selects the session's SQL dialect and optional native runtime path.
// Library defaults to ORCHIDDB_NATIVE_LIBRARY, then the platform library name.
type Config struct {
	Dialect string
	Library string
	// Engines supplies additional SQL or remote sessions for federation. The
	// primary session passed to New remains the execution engine.
	Engines map[string]Engine
}

// QueryOptions supplies query inputs separately from the registered schema.
// Language defaults to cypher; use sparql for SPARQL queries.
type QueryOptions struct {
	Language      string
	Parameters    map[string]any
	Authorization *Authorization
}

// Authorization identifies the principal used by schema permission scopes.
type Authorization struct {
	SubjectType string `json:"subject_type"`
	SubjectID   string `json:"subject_id"`
}

var ErrClosed = errors.New("orchiddb: connection is closed")
var ErrUnsupportedPlan = errors.New("orchiddb: multi-engine execution requires Config.Engines")

type commandFunc func(any, any) error

// Connection retains an immutable graph schema and borrows a database session.
// Concurrent queries are subject to the session and driver's concurrency rules.
type Connection struct {
	session Session
	dialect string
	schema  map[string]json.RawMessage
	command commandFunc
	engines map[string]Engine
	closed  atomic.Bool
}

// New validates and copies a JSON graph schema through the shared native runtime.
// It never opens, closes, or commits the supplied database session.
func New(session Session, schema json.RawMessage, config Config) (*Connection, error) {
	if session == nil || (reflect.ValueOf(session).Kind() == reflect.Ptr && reflect.ValueOf(session).IsNil()) {
		return nil, errors.New("orchiddb: session is required")
	}
	switch config.Dialect {
	case "duckdb", "postgres", "starrocks", "trino":
	default:
		return nil, fmt.Errorf("orchiddb: unsupported dialect %q", config.Dialect)
	}
	if !json.Valid(schema) {
		return nil, errors.New("orchiddb: schema must be valid JSON")
	}
	command, err := loadRuntime(config.Library)
	if err != nil {
		return nil, err
	}
	var validated map[string]json.RawMessage
	if err := command(map[string]any{"op": "validate_schema", "schema": schema}, &validated); err != nil {
		return nil, err
	}
	if validated == nil {
		return nil, errors.New("orchiddb: runtime returned an invalid schema")
	}
	engines := make(map[string]Engine, len(config.Engines))
	for name, engine := range config.Engines {
		engines[name] = engine
	}
	return &Connection{session: session, dialect: config.Dialect, schema: validated, command: command, engines: engines}, nil
}

// Query executes graph text and returns driver-native rows. The caller must close
// rows and check rows.Err(). Parameters are encoded by the shared compiler, never
// interpolated by this client. Context cancellation applies to database execution;
// an in-progress native compilation cannot be interrupted.
func (c *Connection) Query(ctx context.Context, text string, options QueryOptions) (*sql.Rows, error) {
	if c.closed.Load() {
		return nil, ErrClosed
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	request := make(map[string]any, len(c.schema)+6)
	for key, value := range c.schema {
		request[key] = value
	}
	language := options.Language
	if language == "" {
		language = "cypher"
	}
	parameters := options.Parameters
	if parameters == nil {
		parameters = map[string]any{}
	}
	request["version"], request["dialect"] = 1, c.dialect
	request["query"], request["language"], request["parameters"] = text, language, parameters
	if options.Authorization != nil {
		request["authorization"] = options.Authorization
	}
	var raw json.RawMessage
	var plan struct {
		Version   int               `json:"version"`
		Dialect   string            `json:"dialect"`
		SQL       string            `json:"sql"`
		Transfers []json.RawMessage `json:"transfers"`
	}
	if err := c.command(request, &raw); err != nil {
		return nil, err
	}
	if err := json.Unmarshal(raw, &plan); err != nil {
		return nil, err
	}
	if plan.Version != 1 || plan.Dialect != c.dialect || strings.TrimSpace(plan.SQL) == "" {
		return nil, errors.New("orchiddb: invalid execution plan from native runtime")
	}
	if len(plan.Transfers) != 0 {
		if len(c.engines) == 0 {
			return nil, ErrUnsupportedPlan
		}
		sql, err := c.federate(ctx, raw)
		if err != nil {
			return nil, err
		}
		plan.SQL = sql
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	return c.session.QueryContext(ctx, plan.SQL)
}

// Close prevents new queries. It does not close outstanding rows or the borrowed
// session. Queries already in progress may finish. Close is safe to repeat.
func (c *Connection) Close() error { c.closed.Store(true); return nil }
