//go:build !cgo || (!darwin && !linux)

package orchiddb

import "errors"

func loadRuntime(string) (commandFunc, error) {
	return nil, errors.New("orchiddb: native runtime requires cgo on macOS or Linux")
}
