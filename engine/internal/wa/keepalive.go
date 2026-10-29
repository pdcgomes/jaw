package wa

import (
	"fmt"
	"sync"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// told is the connection state the engine told jawd last, and when. whatsmeow dispatches a
// keepalive's timeout and its restore each on a goroutine of its own, as it does Connected and
// Disconnected, so either can reach the engine after a later word on the connection. Every state is
// said under told's lock, so jawd hears them in the order they're noted, and the keepalive's only
// when they're newer than the state said last (timedOut, restored).
type told struct {
	sync.Mutex
	state enginepb.Connection_State
	at    time.Time
	// Of the keepalive timeout said last: the last ping that got through before it, and how many
	// have failed since.
	lastPing time.Time
	failed   int
	// The least time between whatsmeow's pings, and whether a client is connected: whatsmeow's own,
	// but in the tests.
	apart     time.Duration
	connected func(*whatsmeow.Client) bool
}

// connected says whether client is connected to WhatsApp and logged in.
func connected(client *whatsmeow.Client) bool {
	return client.IsConnected() && client.IsLoggedIn()
}

// tell says c to jawd as the connection state said last.
func (e *Engine) tell(c *enginepb.Connection) {
	e.told.Lock()
	defer e.told.Unlock()
	e.tellLocked(c)
}

// tellLocked says c to jawd and notes it as the connection state said last, with told locked.
func (e *Engine) tellLocked(c *enginepb.Connection) {
	e.told.state, e.told.at = c.State, time.Now()
	e.send(&enginepb.Event{Kind: &enginepb.Event_Connection{Connection: c}})
}

// timedOut says whatsmeow's keepalive pings have timed out, unless that's older than the state
// said last. A timeout carries the last ping that got through, and whatsmeow's pings are at least
// apart apart, so the nth failure since can't have come sooner than n of them after it; and after
// a timeout said last, one is newer when it follows a later ping, or more have failed since the
// same one.
func (e *Engine) timedOut(v *events.KeepAliveTimeout) {
	c := e.state(enginepb.Connection_KEEPALIVE_TIMEOUT, fmt.Sprintf("%d keepalives failed", v.ErrorCount))
	e.told.Lock()
	defer e.told.Unlock()
	t := &e.told
	newer := !v.LastSuccess.Add(time.Duration(v.ErrorCount) * t.apart).Before(t.at)
	if t.state == enginepb.Connection_KEEPALIVE_TIMEOUT {
		newer = v.LastSuccess.After(t.lastPing) || v.LastSuccess.Equal(t.lastPing) && v.ErrorCount > t.failed
	}
	if !newer {
		return
	}
	e.tellLocked(c)
	t.lastPing, t.failed = v.LastSuccess, v.ErrorCount
}

// restored says whatsmeow's keepalive pings work again, while the client is connected and the
// state said last is their timeout. A restore carries nothing, so one older than a later state is
// known by that state, and one older than the timeout said last by its coming before the next ping
// after that timeout could have got through. Whether the client is connected is asked before told
// is locked, so no state waits on told while whatsmeow holds its socket's lock (Disconnect does,
// for up to 5 seconds); a connection that ends after the asking is said after the restore.
func (e *Engine) restored() {
	if !e.told.connected(e.cli()) {
		return
	}
	c := e.state(enginepb.Connection_KEEPALIVE_RESTORED, "")
	e.told.Lock()
	defer e.told.Unlock()
	t := &e.told
	next := t.lastPing.Add(time.Duration(t.failed+1) * t.apart)
	if t.state != enginepb.Connection_KEEPALIVE_TIMEOUT || time.Now().Before(next) {
		return
	}
	e.tellLocked(c)
}
