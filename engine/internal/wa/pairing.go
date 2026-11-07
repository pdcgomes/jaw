package wa

import (
	"context"
	"sync"
	"sync/atomic"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// linkFinishing is how long a link the phone has paired waits for its client to be connected:
// once WhatsApp ends the pairing's connection, whatsmeow connects again with the new device within
// a second or two, and says nothing if it can't.
const linkFinishing = time.Minute

// linkLongest is how long a link the phone has paired has to finish, connected or not: whatsmeow
// says Connected only once the IQs after its login are answered, each in up to 75 s.
const linkLongest = 10 * time.Minute

// pairing is the link the phone has paired, while one is finishing. whatsmeow gives the device its
// ID and saves it before WhatsApp has the pairing confirmed, and the link finishes only once the
// client connects with that device (finished). Until then the device is half made: no connection
// event names the account, Logout is refused, and if WhatsApp refuses the device, or the client
// isn't connected linkFinishing after the phone paired, the engine clears the device and says why
// (linkFailed), so linking can start again.
type pairing struct {
	sync.Mutex
	// The client whose link is finishing, or nil; read without the lock (half).
	client atomic.Pointer[whatsmeow.Client]
	// What WhatsApp said of the account as it paired, said to jawd once the link has finished.
	linked *enginepb.Linked
	since  time.Time
	timer  *time.Timer
	// linkFinishing, but in the tests.
	finishing time.Duration
}

func (p *pairing) wait() time.Duration {
	if p.finishing > 0 {
		return p.finishing
	}
	return linkFinishing
}

// end forgets the link that was finishing, with p locked.
func (p *pairing) end() {
	p.client.Store(nil)
	p.linked = nil
	if p.timer != nil {
		p.timer.Stop()
		p.timer = nil
	}
}

// reach is how the engine has a client connect, disconnect and log out with WhatsApp, and asks
// whether it's connected: whatsmeow's own, or a fake in the tests, which never reach WhatsApp.
type reach interface {
	connect(*whatsmeow.Client) error
	disconnect(*whatsmeow.Client)
	logout(context.Context, *whatsmeow.Client) error
	connected(*whatsmeow.Client) bool
}

type whatsmeowReach struct{}

func (whatsmeowReach) connect(client *whatsmeow.Client) error { return client.Connect() }
func (whatsmeowReach) disconnect(client *whatsmeow.Client)    { client.Disconnect() }
func (whatsmeowReach) logout(ctx context.Context, client *whatsmeow.Client) error {
	return client.Logout(ctx)
}
func (whatsmeowReach) connected(client *whatsmeow.Client) bool { return client.IsConnected() }

// half says whether client's device is half made: the phone has paired it, and its link hasn't
// finished.
func (e *Engine) half(client *whatsmeow.Client) bool {
	return client != nil && e.pairing.client.Load() == client
}

// paired notes that the phone has paired client's device, as whatsmeow's PairSuccess says once
// it has saved the device with its ID: the link finishes once the client connects with it.
func (e *Engine) paired(client *whatsmeow.Client, v *events.PairSuccess) {
	linked := &enginepb.Linked{Jid: v.ID.String(), Lid: v.LID.String(), Platform: v.Platform, BusinessName: v.BusinessName}
	// Logged in already, the client has said Connected before PairSuccess, and the link is done.
	if client.IsLoggedIn() {
		e.done(client, linked)
		return
	}
	p := &e.pairing
	p.Lock()
	defer p.Unlock()
	p.end()
	p.client.Store(client)
	p.linked, p.since = linked, time.Now()
	p.timer = time.AfterFunc(p.wait(), func() { e.unfinished(client) })
}

// finished ends client's link, if one was finishing, as the client's first Connected since the
// phone paired says it has, and tells jawd the account just before that Connected; false when the
// link failed first, and the Connected isn't to be said, since the device has gone.
func (e *Engine) finished(client *whatsmeow.Client) bool {
	p := &e.pairing
	p.Lock()
	if !e.half(client) {
		p.Unlock()
		return client == nil || client != e.retired.Load()
	}
	linked := p.linked
	p.end()
	p.Unlock()
	e.done(client, linked)
	return true
}

func (e *Engine) done(client *whatsmeow.Client, linked *enginepb.Linked) {
	linked.PushName = client.Store.PushName
	e.step(&enginepb.Link{Step: &enginepb.Link_Done{Done: linked}})
}

// unfinished is client's link out of time: failed, unless the client is connected, its IQs still
// to be answered, and less than linkLongest has passed since the phone paired.
func (e *Engine) unfinished(client *whatsmeow.Client) {
	connected := e.reach.connected(client)
	p := &e.pairing
	p.Lock()
	if !e.half(client) {
		p.Unlock()
		return
	}
	if connected && time.Since(p.since) < linkLongest {
		p.timer = time.AfterFunc(p.wait(), func() { e.unfinished(client) })
		p.Unlock()
		return
	}
	p.Unlock()
	e.linkFailed(client, "WhatsApp didn't finish linking jaw; link again")
}

// linkFailed ends client's link as failed, if one was finishing: the engine clears the device it
// left and says why, so linking starts afresh. False if no link of client's was finishing.
func (e *Engine) linkFailed(client *whatsmeow.Client, why string) bool {
	if !e.abandon(client) {
		return false
	}
	e.step(&enginepb.Link{Step: &enginepb.Link_Failed{Failed: why}})
	return true
}

// abandon ends client's link, if one was finishing, and clears its half-made device; false if no
// link of client's was finishing. The client is retired under the pairing's lock, so a Connected
// it says meanwhile finds its link gone (finished).
func (e *Engine) abandon(client *whatsmeow.Client) bool {
	p := &e.pairing
	p.Lock()
	if !e.half(client) {
		p.Unlock()
		return false
	}
	p.end()
	retired := e.retire(client)
	p.Unlock()
	if retired {
		e.forget(client)
	}
	return true
}

// clear gives the engine a fresh device in place of the one a link that failed as it paired left,
// with its ID or deleted by whatsmeow.
func (e *Engine) clear(client *whatsmeow.Client) {
	if e.retire(client) {
		e.forget(client)
	}
}

// forget disconnects a retired client before renewing: whatsmeow hasn't deleted its device, so
// its client could still connect with it, and save it again, once it's gone from session.db.
func (e *Engine) forget(client *whatsmeow.Client) {
	client.Disconnect()
	e.renew(client)
}
