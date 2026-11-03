package wa

import (
	"context"
	"database/sql"
	"errors"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/mattn/go-sqlite3"
	"golang.org/x/sys/unix"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
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

func TestTheSessionLockLastsUntilTheProcessExitsAndRefusesAnother(t *testing.T) {
	home := t.TempDir()
	if err := LockSession(home); err != nil {
		t.Fatal(err)
	}
	other, err := os.OpenFile(filepath.Join(home, "session.lock"), os.O_RDWR, 0)
	if err != nil {
		t.Fatal(err)
	}
	defer other.Close()
	for range 5 {
		// Nothing but LockSession keeps the lock's file, which the collector would close.
		runtime.GC()
		time.Sleep(10 * time.Millisecond)
		if err := unix.Flock(int(other.Fd()), unix.LOCK_EX|unix.LOCK_NB); !errors.Is(err, unix.EWOULDBLOCK) {
			t.Fatalf("another could take session.lock: %v", err)
		}
	}
	if err := LockSession(home); err == nil || !strings.Contains(err.Error(), "another jaw-engine is running on "+home) {
		t.Fatalf("a second lock on the folder: %v", err)
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

func TestASessionFromANewerWhatsmeowIsSaidAsOutdatedAndTheEngineWaitsForJawd(t *testing.T) {
	home := t.TempDir()
	container, err := openSession(context.Background(), home, NewLogger("test", "error"))
	if err != nil {
		t.Fatal(err)
	}
	_ = container.Close()
	dsn := (&url.URL{Scheme: "file", Path: filepath.Join(home, "session.db"), RawQuery: sessionDSN}).String()
	db, err := sql.Open(sessionDriver, dsn)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	// As a newer whatsmeow leaves it: a schema this one doesn't know, which older ones can't read.
	if _, err := db.Exec("UPDATE whatsmeow_version SET version = 1000, compat = 1000"); err != nil {
		t.Fatal(err)
	}
	toEngine, fromJawd := io.Pipe()
	toJawd, fromEngine := io.Pipe()
	ran := make(chan error, 1)
	go func() {
		ran <- Run(context.Background(), link.New(toEngine, fromEngine), home, "jaw-engine test", "v-test", NewLogger("test", "error"))
	}()
	jawd := link.New(toJawd, fromJawd)
	if frame := recv(t, jawd); frame.GetHello() == nil {
		t.Fatalf("the engine's hello: %v", frame)
	}
	if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: &enginepb.Hello{Protocol: link.Protocol, Program: "jawd test"}}}); err != nil {
		t.Fatal(err)
	}
	frame := recv(t, jawd)
	said := frame.GetEvent().GetConnection()
	if said.GetState() != enginepb.Connection_CLIENT_OUTDATED || !strings.Contains(said.GetReason(), "newer jaw-engine") || !strings.Contains(said.GetReason(), "v1000") {
		t.Fatalf("a session.db from a newer whatsmeow gave %v", frame)
	}
	command(t, jawd, 1, &enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}})
	if reply := recv(t, jawd); reply.GetReply().GetPong() == nil {
		t.Fatalf("a ping got %v", reply)
	}
	command(t, jawd, 2, &enginepb.Command{Kind: &enginepb.Command_LinkStart{LinkStart: &enginepb.LinkStart{}}})
	if reply := recv(t, jawd); reply.GetReply().GetFailure() == nil {
		t.Fatalf("linking got %v", reply)
	}
	select {
	case err := <-ran:
		t.Fatalf("the engine ended with %v while jawd was there", err)
	default:
	}
	_ = fromJawd.Close()
	select {
	case err := <-ran:
		if err != nil {
			t.Errorf("the engine ended with %v once jawd closed the link", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the engine didn't end once jawd closed the link")
	}
	var version, compat int
	if err := db.QueryRow("SELECT version, compat FROM whatsmeow_version").Scan(&version, &compat); err != nil {
		t.Fatal(err)
	}
	if version != 1000 || compat != 1000 {
		t.Fatalf("session.db's version is now %d, compatible down to %d", version, compat)
	}
}
