//go:build spike

// Package spike measures what whatsmeow's session store costs each incoming message, on mattn's
// SQLite driver and on modernc's pure-Go one, since whatsmeow handles messages one at a time and
// that cost is part of how fast a backlog drains.
//
//	go test -tags spike -run x -bench . -benchtime 20000x ./internal/spike
package spike

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"net/url"
	"path/filepath"
	"testing"
	"time"

	_ "github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow/proto/waAdv"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	_ "modernc.org/sqlite"
)

// device opens a whatsmeow store on `dialect`'s driver with jaw-engine's settings, and saves a
// device so its sessions and buffered events have somewhere to go.
func device(b *testing.B, dialect, query string) *store.Device {
	b.Helper()
	ctx := context.Background()
	dsn := (&url.URL{Scheme: "file", Path: filepath.Join(b.TempDir(), "session.db"), RawQuery: query}).String()
	container, err := sqlstore.New(ctx, dialect, dsn, nil)
	if err != nil {
		b.Fatal(err)
	}
	d := container.NewDevice()
	d.ID = &types.JID{User: "351910000001", Device: 12, Server: types.DefaultUserServer}
	// What pairing would have filled in; saving a device needs its signatures.
	d.Account = &waAdv.ADVSignedDeviceIdentity{
		Details:             []byte{1},
		AccountSignature:    make([]byte, 64),
		AccountSignatureKey: make([]byte, 32),
		DeviceSignature:     make([]byte, 64),
	}
	if err := d.Save(ctx); err != nil {
		b.Fatal(err)
	}
	return d
}

// message does to the store what whatsmeow does for each message it decrypts with the
// decrypted-event buffer on: in one transaction, look for the message in the buffer, load and save
// the sender's session, buffer the plaintext; then, once jawd has it, clear the plaintext.
func message(b *testing.B, d *store.Device, n int) {
	ctx := context.Background()
	var id [8]byte
	binary.BigEndian.PutUint64(id[:], uint64(n))
	hash := sha256.Sum256(id[:])
	address := "351912345678.0:1"
	session := make([]byte, 1200)
	plaintext := make([]byte, 600)
	err := d.EventBuffer.DoDecryptionTxn(ctx, func(ctx context.Context) error {
		if _, err := d.EventBuffer.GetBufferedEvent(ctx, hash); err != nil {
			return err
		}
		if _, err := d.Sessions.GetSession(ctx, address); err != nil {
			return err
		}
		if err := d.Sessions.PutSession(ctx, address, session); err != nil {
			return err
		}
		return d.EventBuffer.PutBufferedEvent(ctx, hash, plaintext, time.Now())
	})
	if err != nil {
		b.Fatal(err)
	}
	if err := d.EventBuffer.ClearBufferedEventPlaintext(ctx, hash); err != nil {
		b.Fatal(err)
	}
}

func BenchmarkMessageStore(b *testing.B) {
	drivers := []struct{ name, dialect, query string }{
		{"mattn", "sqlite3", "_foreign_keys=on&_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000"},
		{"modernc", "sqlite", "_pragma=foreign_keys(1)&_pragma=journal_mode(WAL)&_pragma=synchronous(NORMAL)&_pragma=busy_timeout(5000)"},
	}
	for _, driver := range drivers {
		b.Run(driver.name, func(b *testing.B) {
			d := device(b, driver.dialect, driver.query)
			b.ResetTimer()
			for n := range b.N {
				message(b, d, n)
			}
		})
	}
}
