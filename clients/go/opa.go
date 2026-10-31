package orchiddb

import "errors"

// OPAConfig binds trusted application identity to optional Open Policy Agent
// authorization. New copies this configuration. Nil disables OPA. Each connection
// reuses a pooled HTTP client with a shared concurrency limit; decisions are not
// cached across queries. Zero limits select defaults (10s, 16 requests, 256 items,
// 1 MiB). Close the connection to release its native policy resources.
type OPAConfig struct {
	PolicyURI             string            `json:"policy_uri"`
	User                  string            `json:"user"`
	Groups                []string          `json:"groups,omitempty"`
	BatchedURI            string            `json:"batched_uri,omitempty"`
	RowFiltersURI         string            `json:"row_filters_uri,omitempty"`
	ColumnMaskingURI      string            `json:"column_masking_uri,omitempty"`
	BatchColumnMaskingURI string            `json:"batch_column_masking_uri,omitempty"`
	TimeoutMillis         uint64            `json:"timeout_ms,omitempty"`
	MaxConcurrentRequests uint64            `json:"max_concurrent_requests,omitempty"`
	MaxBatchSize          uint64            `json:"max_batch_size,omitempty"`
	MaxResponseBytes      uint64            `json:"max_response_bytes,omitempty"`
	DefaultCatalog        string            `json:"default_catalog,omitempty"`
	DefaultSchema         string            `json:"default_schema,omitempty"`
	Context               map[string]string `json:"context,omitempty"`
}

func createOPA(command commandFunc, config *OPAConfig) (uint64, error) {
	if config == nil {
		return 0, nil
	}
	var handle uint64
	if err := command(map[string]any{"op": "opa_create", "config": config}, &handle); err != nil {
		return 0, err
	}
	if handle == 0 {
		return 0, errors.New("orchiddb: runtime returned an invalid OPA policy")
	}
	return handle, nil
}
func compileOPA(command commandFunc, handle uint64, request any, result any) error {
	if handle != 0 {
		request = map[string]any{"op": "opa_compile", "handle": handle, "request": request}
	}
	return command(request, result)
}
func closeOPA(command commandFunc, handle uint64) error {
	if handle == 0 {
		return nil
	}
	var result any
	return command(map[string]any{"op": "opa_close", "handle": handle}, &result)
}
