package orchiddb

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"strings"
	"testing"
)

func TestLiveStreamAuthorization(t *testing.T) {
	path := os.Getenv("ORCHIDDB_STREAM_FIXTURE")
	if path == "" {
		t.Skip("live Kafka/SpiceDB fixture required")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var f struct {
		Kafka       map[string]string `json:"kafka"`
		Topic       string            `json:"topic_prefix"`
		Source      string            `json:"source"`
		AuthURL     string            `json:"auth_url"`
		Schema      json.RawMessage   `json:"schema"`
		Subject     Authorization     `json:"subject"`
		Query       string            `json:"query"`
		StreamQuery string            `json:"stream_query"`
		Offsets     map[int32]int64   `json:"offsets"`
	}
	if err = json.Unmarshal(data, &f); err != nil {
		t.Fatal(err)
	}
	auth := func(ctx context.Context, subject Authorization, checks []PermissionCheck) ([]bool, error) {
		body, _ := json.Marshal(map[string]any{"subject": subject, "checks": checks})
		req, err := http.NewRequestWithContext(ctx, "POST", f.AuthURL, bytes.NewReader(body))
		if err != nil {
			return nil, err
		}
		req.Header.Set("Content-Type", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return nil, err
		}
		defer resp.Body.Close()
		if resp.StatusCode != 200 {
			return nil, fmt.Errorf("auth HTTP %d", resp.StatusCode)
		}
		var decisions []bool
		err = json.NewDecoder(resp.Body).Decode(&decisions)
		return decisions, err
	}
	c, err := NewStream(f.Schema, StreamConfig{Kafka: f.Kafka, Topic: f.Topic + "-go", Source: f.Source, Authorizer: auth})
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()
	ctx := context.Background()
	change := func(key string, version int) any {
		return map[string]any{"table": "docs", "key": key, "version": version, "data": map[string]any{"name": fmt.Sprintf("%s-%d", key, version)}}
	}
	if _, err = c.Insert(ctx, f.Subject, change("blocked", 1)); err == nil || !strings.Contains(err.Error(), "denied") {
		t.Fatalf("expected denial: %v", err)
	}
	if _, err = c.Insert(ctx, f.Subject, change("a", 1)); err != nil {
		t.Fatal(err)
	}
	if _, err = c.InsertBatch(ctx, f.Subject, []any{change("b", 1), change("hidden", 1), change("a", 2)}); err != nil {
		t.Fatal(err)
	}
	query, err := c.Query(ctx, f.Query, f.Subject, nil)
	if err != nil {
		t.Fatal(err)
	}
	hybrid, err := c.QueryHybrid(ctx, f.Query, f.Subject, nil, map[string]any{"source": f.Source, "offsets": f.Offsets, "changes": []any{change("a", 1), change("b", 1)}})
	if err != nil {
		t.Fatal(err)
	}
	if err = c.Subscribe(ctx, f.StreamQuery, f.Subject, nil, f.Offsets); err != nil {
		t.Fatal(err)
	}
	var frames []json.RawMessage
	var offsets map[int32]int64
	for i := 0; i < 20; i++ {
		raw, err := c.NextBatch(ctx, 16, 100)
		if err != nil {
			t.Fatal(err)
		}
		var batch struct {
			Frames  []json.RawMessage `json:"frames"`
			Offsets map[int32]int64   `json:"offsets"`
		}
		if err = json.Unmarshal(raw, &batch); err != nil {
			t.Fatal(err)
		}
		frames = append(frames, batch.Frames...)
		offsets = batch.Offsets
		if offsets[0] == 4 {
			break
		}
	}
	if len(frames) != 3 || offsets[0] != 4 {
		t.Fatalf("frames=%d offsets=%v", len(frames), offsets)
	}
	if err = c.Unsubscribe(ctx); err != nil {
		t.Fatal(err)
	}
	report, _ := json.Marshal(map[string]any{"query": query, "hybrid": hybrid, "frames": frames, "offsets": offsets})
	if err = os.WriteFile("/tmp/orchid-stream-go-result.json", report, 0600); err != nil {
		t.Fatal(err)
	}
	// Cancellation terminates the handle rather than retaining an invisible consumer.
	cancelled, cancel := context.WithCancel(ctx)
	cancel()
	if _, err = c.Query(cancelled, f.Query, f.Subject, nil); err != context.Canceled {
		t.Fatalf("cancel: %v", err)
	}
}
