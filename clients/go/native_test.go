//go:build (darwin || linux) && (amd64 || arm64)

package orchiddb

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func fixtureLibrary(t *testing.T, source string) string {
	t.Helper()
	cc, err := exec.LookPath("cc")
	if err != nil {
		t.Skip("C compiler unavailable")
	}
	dir := t.TempDir()
	input := filepath.Join(dir, "fixture.c")
	output := filepath.Join(dir, "fixture.so")
	if err := os.WriteFile(input, []byte(source), 0600); err != nil {
		t.Fatal(err)
	}
	flags := []string{"-shared", "-fPIC"}
	if runtime.GOOS == "darwin" {
		flags = []string{"-dynamiclib"}
	}
	flags = append(flags, "-o", output, input)
	if out, err := exec.Command(cc, flags...).CombinedOutput(); err != nil {
		t.Fatalf("compile fixture: %v\n%s", err, out)
	}
	return output
}

func TestNativeOwnershipAndErrors(t *testing.T) {
	path := fixtureLibrary(t, `
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
static int outstanding = 0;
uint32_t orchiddb_abi_version(void) { return 2; }
void orchiddb_string_free(char *p) { outstanding--; free(p); }
char *orchiddb_execution_command(const char *p) {
 if (strstr(p,"null_pointer")) return NULL;
 const char *response = "{\"ok\":true,\"result\":{}}";
 if (outstanding) response = "{\"ok\":false,\"error\":\"leaked response\"}";
 else if (strstr(p,"malformed")) response = "{";
 else if (strstr(p,"wrong_result")) response = "{\"ok\":true,\"result\":[]} ";
 else if (strstr(p,"failure")) response = "{\"ok\":false,\"error\":\"compiler failure\"}";
 outstanding++;
 return strdup(response);
}`)
	command, err := loadRuntime(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, request := range []string{"malformed", "failure", "null_pointer", "wrong_result"} {
		var result map[string]any
		if err := command(request, &result); err == nil {
			t.Fatalf("accepted %s", request)
		}
		if err := command("success", &result); err != nil {
			t.Fatalf("response not freed: %v", err)
		}
	}
	var result map[string]any
	if err := command(make(chan int), &result); err == nil {
		t.Fatal("accepted unserializable input")
	}
	if _, err := New(&testSession{}, json.RawMessage(`{}`), Config{Dialect: "duckdb", Library: path}); err != nil {
		t.Fatal(err)
	}
}

func TestNativeABI(t *testing.T) {
	path := fixtureLibrary(t, `
#include <stdint.h>
uint32_t orchiddb_abi_version(void) { return 1; }
char *orchiddb_execution_command(const char *p) { return 0; }
void orchiddb_string_free(char *p) {}
`)
	if _, err := loadRuntime(path); err == nil || !strings.Contains(err.Error(), "ABI") {
		t.Fatalf("expected ABI error: %v", err)
	}
	if _, err := loadRuntime("bad\x00path"); err == nil {
		t.Fatal("accepted NUL path")
	}
}

func TestNativeMissingSymbol(t *testing.T) {
	path := fixtureLibrary(t, `
#include <stdint.h>
uint32_t orchiddb_abi_version(void) { return 2; }
`)
	if _, err := loadRuntime(path); err == nil || !strings.Contains(err.Error(), "orchiddb_execution_command") {
		t.Fatalf("expected missing-symbol error, got %v", err)
	}
}

// This test deliberately uses no database driver so it exercises the real Rust
// ABI even when the test binary is built with CGO_ENABLED=0.
func TestNativeRuntime(t *testing.T) {
	library := os.Getenv("ORCHIDDB_NATIVE_LIBRARY")
	if library == "" {
		t.Skip("set ORCHIDDB_NATIVE_LIBRARY to test the real runtime")
	}
	session := &testSession{err: errors.New("execution reached borrowed session")}
	graph, err := New(session, json.RawMessage(`{}`), Config{Dialect: "duckdb", Library: library})
	if err != nil {
		t.Fatal(err)
	}
	defer graph.Close()
	value := strings.Repeat("Orchid 🌸 O'Reilly ", 256)
	_, err = graph.Query(context.Background(), "RETURN $value AS name", QueryOptions{Parameters: map[string]any{"value": value}})
	if !errors.Is(err, session.err) || session.calls != 1 || !strings.Contains(session.query, "🌸") {
		t.Fatalf("native query did not reach session: %v (calls=%d)", err, session.calls)
	}
	if _, err := New(session, json.RawMessage(`{"query":"RETURN 1"}`), Config{Dialect: "duckdb", Library: library}); err == nil {
		t.Fatal("runtime accepted query text in schema")
	}
	// Exercise the cached function bindings concurrently while Go collects memory.
	for i := 0; i < 4; i++ {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			t.Parallel()
			command, err := loadRuntime(library)
			if err != nil {
				t.Fatal(err)
			}
			for j := 0; j < 8; j++ {
				runtime.GC()
				var schema map[string]json.RawMessage
				if err := command(map[string]any{"op": "validate_schema", "schema": map[string]any{}}, &schema); err != nil {
					t.Fatal(err)
				}
			}
		})
	}
}
