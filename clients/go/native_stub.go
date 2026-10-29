//go:build (!darwin && !linux) || (!amd64 && !arm64)

package orchiddb

import "errors"

func loadRuntime(string) (commandFunc, error) {
	return nil, errors.New("orchiddb: native runtime requires macOS or Linux on amd64 or arm64")
}
