//go:build cgo && (darwin || linux)

package orchiddb

import (
	"encoding/json"
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
 else if (strstr(p,"failure")) response = "{\"ok\":false,\"error\":\"compiler failure\"}";
 outstanding++;
 return strdup(response);
}`)
	command, err := loadRuntime(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, request := range []string{"malformed", "failure", "null_pointer"} {
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
