//go:build (darwin || linux) && (amd64 || arm64)

package orchiddb

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"runtime"
	"strings"
	"sync"
	"unsafe"

	"github.com/ebitengine/purego"
)

type nativeRuntime struct {
	handle  uintptr
	command func(string) *byte
	release func(*byte)
}

// Keep native code mapped for the process lifetime: the Rust runtime owns
// worker threads that can outlive any individual connection.
func openRuntime(path string) (*nativeRuntime, error) {
	handle, err := purego.Dlopen(path, purego.RTLD_NOW|purego.RTLD_LOCAL)
	if err != nil {
		return nil, err
	}
	rt := &nativeRuntime{handle: handle}
	var abi func() uint32
	for _, symbol := range []struct {
		name   string
		target any
	}{
		{"orchiddb_abi_version", &abi},
		{"orchiddb_execution_command", &rt.command},
		{"orchiddb_string_free", &rt.release},
	} {
		address, err := purego.Dlsym(handle, symbol.name)
		if err != nil {
			return nil, fmt.Errorf("resolve %s: %w", symbol.name, err)
		}
		purego.RegisterFunc(symbol.target, address)
	}
	if abi() != 2 {
		return nil, errors.New("incompatible OrchidDB ABI; expected 2")
	}
	return rt, nil
}

// nativeBytes borrows the ABI's NUL-terminated response until release is called.
func nativeBytes(pointer *byte) []byte {
	length := 0
	for *(*byte)(unsafe.Add(unsafe.Pointer(pointer), length)) != 0 {
		length++
	}
	return unsafe.Slice(pointer, length)
}

var runtimeCache = struct {
	sync.Mutex
	handles map[string]*nativeRuntime
}{handles: make(map[string]*nativeRuntime)}

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
		var err error
		rt, err = openRuntime(path)
		if err != nil {
			return nil, fmt.Errorf("orchiddb: load native runtime: %w", err)
		}
		runtimeCache.handles[path] = rt
	}
	return func(request any, result any) error {
		data, err := json.Marshal(request)
		if err != nil {
			return fmt.Errorf("orchiddb: encode request: %w", err)
		}
		// purego keeps a NUL-terminated copy alive for this synchronous call.
		// The native ABI copies the request before dispatching to its workers.
		output := rt.command(string(data))
		if output == nil {
			return errors.New("orchiddb: native runtime returned null")
		}
		defer rt.release(output)
		var envelope struct {
			OK     bool            `json:"ok"`
			Error  string          `json:"error"`
			Result json.RawMessage `json:"result"`
		}
		if err := json.Unmarshal(nativeBytes(output), &envelope); err != nil {
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
