package orchiddb

import (
	"context"
	"encoding/json"
	"errors"
	"sync"
)

// PermissionCheck describes one resource decision. Implement StreamAuthorizer
// with the application's SpiceDB client; caching and expiry remain caller-owned.
type PermissionCheck struct {
	ResourceType string `json:"resource_type"`
	ResourceID   string `json:"resource_id"`
	Permission   string `json:"permission"`
}
type StreamAuthorizer func(context.Context, Authorization, []PermissionCheck) ([]bool, error)
type StreamConfig struct {
	Kafka                  map[string]string
	Topic, Source, Library string
	Authorizer             StreamAuthorizer
	AuthTimeoutMS          uint64
}

// StreamConnection owns a Kafka connection. Its results contain base64 Arrow IPC.
// A subscription produces replacement frames; persist offsets only after the
// complete batch is processed. Use separate connections for concurrent work.
type StreamConnection struct {
	command commandFunc
	auth    StreamAuthorizer
	mu      sync.Mutex
	id      string
	busy    bool
}

func NewStream(schema json.RawMessage, config StreamConfig) (*StreamConnection, error) {
	if config.Authorizer == nil {
		return nil, errors.New("orchiddb: authorizer callback required")
	}
	command, err := loadRuntime(config.Library)
	if err != nil {
		return nil, err
	}
	timeout := config.AuthTimeoutMS
	if timeout == 0 {
		timeout = 30000
	}
	var opened struct {
		ID string `json:"id"`
	}
	err = command(map[string]any{"op": "stream_open", "schema": schema, "kafka": config.Kafka, "topic": config.Topic, "source": config.Source, "auth_timeout_ms": timeout}, &opened)
	if err != nil {
		return nil, err
	}
	return &StreamConnection{command: command, auth: config.Authorizer, id: opened.ID}, nil
}
func (c *StreamConnection) call(op string, values map[string]any, result any) error {
	c.mu.Lock()
	id := c.id
	c.mu.Unlock()
	if id == "" {
		return ErrClosed
	}
	if values == nil {
		values = map[string]any{}
	}
	values["op"], values["id"] = op, id
	return c.command(values, result)
}
func (c *StreamConnection) run(ctx context.Context, action string, values map[string]any) (json.RawMessage, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	c.mu.Lock()
	if c.busy {
		c.mu.Unlock()
		return nil, errors.New("orchiddb: stream connection busy")
	}
	c.busy = true
	c.mu.Unlock()
	defer func() { c.mu.Lock(); c.busy = false; c.mu.Unlock() }()
	values["action"] = action
	var ignored json.RawMessage
	if err := c.call("stream_start", values, &ignored); err != nil {
		return nil, err
	}
	for {
		if err := ctx.Err(); err != nil {
			c.Close()
			return nil, err
		}
		var event struct {
			State   string            `json:"state"`
			Result  json.RawMessage   `json:"result"`
			Subject Authorization     `json:"subject"`
			Checks  []PermissionCheck `json:"checks"`
			Nonce   string            `json:"nonce"`
		}
		if err := c.call("stream_poll", nil, &event); err != nil {
			return nil, err
		}
		if event.State == "done" {
			return event.Result, nil
		}
		if event.State == "authorize" {
			decisions, err := c.auth(ctx, event.Subject, event.Checks)
			if err != nil {
				c.Close()
				return nil, err
			}
			if len(decisions) != len(event.Checks) {
				c.Close()
				return nil, errors.New("orchiddb: authorizer must return one bool per check")
			}
			if err = c.call("stream_authorize", map[string]any{"nonce": event.Nonce, "decisions": decisions}, &ignored); err != nil {
				c.Close()
				return nil, err
			}
		}
	}
}
func (c *StreamConnection) Insert(ctx context.Context, subject Authorization, change any) (json.RawMessage, error) {
	return c.run(ctx, "insert", map[string]any{"subject": subject, "change": change})
}
func (c *StreamConnection) InsertBatch(ctx context.Context, subject Authorization, changes any) (json.RawMessage, error) {
	return c.run(ctx, "insert_batch", map[string]any{"subject": subject, "changes": changes})
}
func (c *StreamConnection) Query(ctx context.Context, text string, subject Authorization, parameters map[string]any) (json.RawMessage, error) {
	if parameters == nil {
		parameters = map[string]any{}
	}
	return c.run(ctx, "query", map[string]any{"subject": subject, "text": text, "parameters": parameters})
}
func (c *StreamConnection) QueryHybrid(ctx context.Context, text string, subject Authorization, parameters map[string]any, snapshot any) (json.RawMessage, error) {
	if parameters == nil {
		parameters = map[string]any{}
	}
	return c.run(ctx, "hybrid", map[string]any{"subject": subject, "text": text, "parameters": parameters, "snapshot": snapshot})
}
func (c *StreamConnection) Subscribe(ctx context.Context, text string, subject Authorization, parameters map[string]any, offsets map[int32]int64) error {
	if parameters == nil {
		parameters = map[string]any{}
	}
	_, err := c.run(ctx, "subscribe", map[string]any{"subject": subject, "text": text, "parameters": parameters, "offsets": offsets})
	return err
}
func (c *StreamConnection) NextBatch(ctx context.Context, maxRecords int, waitMS int) (json.RawMessage, error) {
	return c.run(ctx, "next", map[string]any{"max_records": maxRecords, "wait_ms": waitMS})
}
func (c *StreamConnection) Unsubscribe(ctx context.Context) error {
	_, err := c.run(ctx, "unsubscribe", map[string]any{})
	return err
}

// Close cancels pending work. Context cancellation also closes the connection.
func (c *StreamConnection) Close() error {
	c.mu.Lock()
	id := c.id
	c.id = ""
	c.mu.Unlock()
	if id == "" {
		return nil
	}
	var ignored json.RawMessage
	return c.command(map[string]any{"op": "stream_close", "id": id}, &ignored)
}
