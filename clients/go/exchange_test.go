package orchiddb

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"encoding/json"
	"fmt"
	"io"
	"sync"
	"testing"
	"time"
)

type exactDecimal struct{ value string }

func (d *exactDecimal) String() string { return d.value }

type exchangeDriver struct{}
type exchangeConn struct{}
type exchangeRows struct{ read bool }

func (exchangeDriver) Open(string) (driver.Conn, error)  { return exchangeConn{}, nil }
func (exchangeConn) Prepare(string) (driver.Stmt, error) { return nil, fmt.Errorf("unused") }
func (exchangeConn) Close() error                        { return nil }
func (exchangeConn) Begin() (driver.Tx, error)           { return nil, fmt.Errorf("unused") }
func (exchangeConn) QueryContext(context.Context, string, []driver.NamedValue) (driver.Rows, error) {
	return &exchangeRows{}, nil
}
func (*exchangeRows) Columns() []string { return []string{"id", "amount", "payload", "day"} }
func (*exchangeRows) Close() error      { return nil }
func (r *exchangeRows) Next(values []driver.Value) error {
	if r.read {
		return io.EOF
	}
	r.read = true
	values[0] = int64(9007199254740993)
	values[1] = exactDecimal{"12345678901234567890.1234"}
	values[2] = []byte{0, 255, 1}
	values[3] = time.Date(2026, 10, 14, 0, 0, 0, 0, time.UTC)
	return nil
}

var registerExchange sync.Once

func TestExchangePreservesIntegerDecimalBinaryAndDate(t *testing.T) {
	registerExchange.Do(func() { sql.Register("orchid_exchange_test", exchangeDriver{}) })
	db, err := sql.Open("orchid_exchange_test", "")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	columns := []exchangeColumn{{Name: "id", DataType: json.RawMessage(`"int64"`)}, {Name: "amount", DataType: json.RawMessage(`"decimal128:38:4"`)}, {Name: "payload", DataType: json.RawMessage(`"binary"`)}, {Name: "day", DataType: json.RawMessage(`"date32"`)}}
	got, err := collectRows(context.Background(), db, "SELECT input", columns)
	if err != nil {
		t.Fatal(err)
	}
	want := `[[9007199254740993,"12345678901234567890.1234","AP8B","2026-10-14"]]`
	if string(got) != want {
		t.Fatalf("exchange lost type/precision: %s", got)
	}
}
