package wa

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"fmt"
	"net/url"
	"path/filepath"
	"strings"

	"github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow/store/sqlstore"
	waLog "go.mau.fi/whatsmeow/util/log"
)

// sessionDriver is mattn's driver with what its DSN can't set: on macOS a checkpoint whose syncs
// stop at the drive's cache can leave session.db malformed after a power cut, and a WAL that
// isn't cut back at each restart keeps its largest size for good.
const sessionDriver = "sqlite3-jaw-session"

// sessionDSN commits at FULL, so session.db's commits reach the drive before jaw.db's F_FULLFSYNC
// flushes the drive's cache, and begins every transaction IMMEDIATE, so one that reads and then
// writes waits for the lock rather than failing with SQLITE_BUSY_SNAPSHOT.
const sessionDSN = "_journal_mode=WAL&_synchronous=FULL&_foreign_keys=on&_busy_timeout=5000&_txlock=immediate"

type pragma struct{ name, value string }

var sessionSet = []pragma{{"checkpoint_fullfsync", "1"}, {"journal_size_limit", "16777216"}}

// sessionChecked is every setting a connection must have, its DSN's among them, read back since
// SQLite ignores a PRAGMA it doesn't know or a value it can't take.
var sessionChecked = append([]pragma{
	{"journal_mode", "wal"},
	{"synchronous", "2"},
	{"foreign_keys", "1"},
	{"busy_timeout", "5000"},
}, sessionSet...)

func init() {
	sql.Register(sessionDriver, &sqlite3.SQLiteDriver{ConnectHook: func(conn *sqlite3.SQLiteConn) error {
		return setPragmas(conn, sessionSet, sessionChecked)
	}})
}

// setPragmas sets `set` on conn, then fails unless each of `checked` reads as it should.
func setPragmas(conn *sqlite3.SQLiteConn, set, checked []pragma) error {
	for _, p := range set {
		if _, err := conn.Exec(fmt.Sprintf("PRAGMA %s = %s", p.name, p.value), nil); err != nil {
			return fmt.Errorf("can't set session.db's %s: %w", p.name, err)
		}
	}
	for _, p := range checked {
		got, err := readPragma(conn, p.name)
		if err != nil {
			return fmt.Errorf("can't read session.db's %s: %w", p.name, err)
		}
		if !strings.EqualFold(got, p.value) {
			return fmt.Errorf("session.db's %s is %s, not %s", p.name, got, p.value)
		}
	}
	return nil
}

func readPragma(conn *sqlite3.SQLiteConn, name string) (string, error) {
	rows, err := conn.Query("PRAGMA "+name, nil)
	if err != nil {
		return "", err
	}
	defer rows.Close()
	values := make([]driver.Value, 1)
	if err := rows.Next(values); err != nil {
		return "", err
	}
	if b, ok := values[0].([]byte); ok {
		return string(b), nil
	}
	return fmt.Sprint(values[0]), nil
}

// openSession opens whatsmeow's store in home/session.db through sessionDriver and brings its
// schema up to date.
func openSession(ctx context.Context, home string, log waLog.Logger) (*sqlstore.Container, error) {
	dsn := (&url.URL{Scheme: "file", Path: filepath.Join(home, "session.db"), RawQuery: sessionDSN}).String()
	db, err := sql.Open(sessionDriver, dsn)
	if err != nil {
		return nil, err
	}
	container := sqlstore.NewWithDB(db, "sqlite3", log)
	if err := container.Upgrade(ctx); err != nil {
		_ = container.Close()
		return nil, err
	}
	return container, nil
}
