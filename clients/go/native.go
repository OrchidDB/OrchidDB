//go:build cgo && (darwin || linux)

package orchiddb

/*
#cgo linux LDFLAGS: -ldl
#include <dlfcn.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct {
 void *handle;
 uint32_t (*abi)(void);
 char *(*command)(const char *);
 void (*release)(char *);
} orchid_runtime;

static orchid_runtime *orchid_open(const char *path, const char **error) {
 void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
 if (!handle) { *error = dlerror(); return NULL; }
 // Keep native code mapped: the runtime owns process-wide worker threads.
 orchid_runtime *rt = calloc(1, sizeof(orchid_runtime));
 if (!rt) { *error = "native runtime allocation failed"; return NULL; }
 rt->handle = handle;
 rt->abi = (uint32_t (*)(void))dlsym(handle, "orchiddb_abi_version");
 rt->command = (char *(*)(const char *))dlsym(handle, "orchiddb_execution_command");
 rt->release = (void (*)(char *))dlsym(handle, "orchiddb_string_free");
 if (!rt->abi || !rt->command || !rt->release) {
  *error = "missing required OrchidDB ABI symbols"; free(rt); return NULL;
 }
 if (rt->abi() != 2) { *error = "incompatible OrchidDB ABI; expected 2"; free(rt); return NULL; }
 return rt;
}
static char *orchid_command(orchid_runtime *rt, const char *input) { return rt->command(input); }
static void orchid_release(orchid_runtime *rt, char *output) { rt->release(output); }
*/
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"runtime"
	"strings"
	"sync"
	"unsafe"
)

var runtimeCache = struct {
	sync.Mutex
	handles map[string]*C.orchid_runtime
}{handles: make(map[string]*C.orchid_runtime)}

func loadRuntime(path string) (commandFunc, error) {
	if path == "" {
		path = os.Getenv("ORCHIDDB_NATIVE_LIBRARY")
	}
	if path == "" {
		path = "liborchiddb_compiler.so"
		if runtime.GOOS == "darwin" {
			path = "liborchiddb_compiler.dylib"
		}
	}
	if strings.ContainsRune(path, 0) {
		return nil, errors.New("orchiddb: library path contains NUL")
	}
	runtimeCache.Lock()
	defer runtimeCache.Unlock()
	rt := runtimeCache.handles[path]
	if rt == nil {
		input := C.CString(path)
		defer C.free(unsafe.Pointer(input))
		var message *C.char
		rt = C.orchid_open(input, &message)
		if rt == nil {
			return nil, fmt.Errorf("orchiddb: load native runtime: %s", C.GoString(message))
		}
		runtimeCache.handles[path] = rt
	}
	return func(request any, result any) error {
		data, err := json.Marshal(request)
		if err != nil {
			return fmt.Errorf("orchiddb: encode request: %w", err)
		}
		input := C.CString(string(data))
		defer C.free(unsafe.Pointer(input))
		output := C.orchid_command(rt, input)
		if output == nil {
			return errors.New("orchiddb: native runtime returned null")
		}
		defer C.orchid_release(rt, output)
		var envelope struct {
			OK     bool            `json:"ok"`
			Error  string          `json:"error"`
			Result json.RawMessage `json:"result"`
		}
		if err := json.Unmarshal([]byte(C.GoString(output)), &envelope); err != nil {
			return fmt.Errorf("orchiddb: decode response: %w", err)
		}
		if !envelope.OK {
			return fmt.Errorf("orchiddb: %s", envelope.Error)
		}
		if err := json.Unmarshal(envelope.Result, result); err != nil {
			return fmt.Errorf("orchiddb: decode result: %w", err)
		}
		return nil
	}, nil
}
