package wa

import (
	"context"
	"database/sql"
	"net/url"
	"path/filepath"
	"strings"
	"testing"

	"github.com/mattn/go-sqlite3"
)

func TestEveryConnectionToTheSessionIsSyncedAndCheckpointsReachTheDisk(t *testing.T) {
	home := t.TempDir()
	container, err := openSession(context.Background(), home, NewLogger("test", "error"))
	if err != nil {
		t.Fatal(err)
	}
	defer container.Close()
	dsn := (&url.URL{Scheme: "file", Path: filepath.Join(home, "session.db"), RawQuery: sessionDSN}).String()
	db, err := sql.Open(sessionDriver, dsn)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	db.SetMaxOpenConns(3)
	conns := make([]*sql.Conn, 3)
	for n := range conns {
		if conns[n], err = db.Conn(context.Background()); err != nil {
			t.Fatal(err)
		}
		defer conns[n].Close()
		for _, p := range sessionChecked {
			var got string
			if err := conns[n].QueryRowContext(context.Background(), "PRAGMA "+p.name).Scan(&got); err != nil {
				t.Fatal(err)
			}
			if !strings.EqualFold(got, p.value) {
				t.Errorf("connection %d: %s is %s, not %s", n, p.name, got, p.value)
			}
		}
	}
}

func TestASessionSettingThatDidntTakeIsRefused(t *testing.T) {
	db, err := sql.Open("sqlite3", filepath.Join(t.TempDir(), "session.db"))
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	conn, err := db.Conn(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Close()
	err = conn.Raw(func(raw any) error {
		// SQLite takes this without an error and leaves synchronous at NORMAL.
		return setPragmas(raw.(*sqlite3.SQLiteConn), []pragma{{"synchronous", "'FULL ff'"}}, []pragma{{"synchronous", "2"}})
	})
	if err == nil || !strings.Contains(err.Error(), "synchronous is") {
		t.Fatalf("a synchronous that didn't take: %v", err)
	}
}
