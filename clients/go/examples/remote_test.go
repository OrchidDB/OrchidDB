package main

import (
	"context"
	"database/sql"
	"encoding/json"
	orchiddb "github.com/OrchidDB/OrchidDB/clients/go"
	"os"
	"testing"
)

func TestLiveRemoteFederation(t *testing.T) {
	path := os.Getenv("ORCHIDDB_REMOTE_FIXTURE")
	if path == "" {
		t.Skip("live remote fixture required")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var fixture struct {
		Cases []struct {
			Name, Adapter, Endpoint string
			Options                 map[string]any             `json:"options"`
			Request                 map[string]json.RawMessage `json:"request"`
			Setup                   []string                   `json:"setup_sql"`
			Expected                json.RawMessage            `json:"expected_rows"`
		} `json:"cases"`
	}
	if err = json.Unmarshal(data, &fixture); err != nil {
		t.Fatal(err)
	}
	for _, f := range fixture.Cases {
		t.Run(f.Name, func(t *testing.T) {
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
			for _, statement := range f.Setup {
				if _, err = session.ExecContext(ctx, statement); err != nil {
					t.Fatal(err)
				}
			}
			options := map[string]any{"endpoint": f.Endpoint, "batch_size": 2}
			for k, v := range f.Options {
				options[k] = v
			}
			remote, err := orchiddb.NewRemoteEngine(f.Adapter, options, "")
			if err != nil {
				t.Fatal(err)
			}
			defer remote.Close()
			var query, language string
			var parameters map[string]any
			json.Unmarshal(f.Request["query"], &query)
			json.Unmarshal(f.Request["language"], &language)
			json.Unmarshal(f.Request["parameters"], &parameters)
			graphSchema := map[string]json.RawMessage{}
			for k, v := range f.Request {
				switch k {
				case "query", "language", "parameters", "authorization", "version", "dialect":
				default:
					graphSchema[k] = v
				}
			}
			var descriptors map[string]struct {
				Dialect string `json:"dialect"`
			}
			if err = json.Unmarshal(f.Request["engines"], &descriptors); err != nil {
				t.Fatal(err)
			}
			engines := map[string]orchiddb.Engine{}
			for name, descriptor := range descriptors {
				if descriptor.Dialect == f.Adapter {
					engines[name] = orchiddb.Engine{Remote: remote}
				} else {
					engines[name] = orchiddb.Engine{Dialect: descriptor.Dialect, Session: session}
				}
			}
			schema, _ := json.Marshal(graphSchema)
			c, err := orchiddb.New(session, schema, orchiddb.Config{Dialect: "duckdb", Engines: engines})
			if err != nil {
				t.Fatal(err)
			}
			defer c.Close()
			rows, err := c.Query(ctx, query, orchiddb.QueryOptions{Language: language, Parameters: parameters})
			if err != nil {
				t.Fatal(err)
			}
			defer rows.Close()
			columns, err := rows.Columns()
			if err != nil {
				t.Fatal(err)
			}
			actual := [][]any{}
			for rows.Next() {
				row := make([]any, len(columns))
				dest := make([]any, len(row))
				for i := range row {
					dest[i] = &row[i]
				}
				if err = rows.Scan(dest...); err != nil {
					t.Fatal(err)
				}
				actual = append(actual, row)
			}
			if err = rows.Err(); err != nil {
				t.Fatal(err)
			}
			rows.Close()
			got, _ := json.Marshal(actual)
			var want any
			json.Unmarshal(f.Expected, &want)
			expected, _ := json.Marshal(want)
			if string(got) != string(expected) {
				t.Fatalf("got %s want %s", got, expected)
			}
			if err = remote.ClearMetadataCache(); err != nil {
				t.Fatal(err)
			}
			var answer int
			if err = session.QueryRowContext(ctx, "SELECT 42").Scan(&answer); err != nil || answer != 42 {
				t.Fatalf("borrowed session unusable: %v", err)
			}
		})
	}
}
