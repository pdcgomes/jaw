package wa

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// connectedEngine is a linked engine whose clients count as connected while up says so, whose
// keepalive pings are at least apart apart, and which has said its client connected.
func connectedEngine(t *testing.T, apart time.Duration) (*Engine, *link.Link, *atomic.Bool, func()) {
	t.Helper()
	e, jawd, closeLink := linkedEngine(t)
	up := &atomic.Bool{}
	up.Store(true)
	e.told.apart = apart
	e.told.connected = func(*whatsmeow.Client) bool { return up.Load() }
	go e.handleFrom(e.cli(), connects(e.cli(), time.Now()))
	saidNext(t, jawd, enginepb.Connection_CONNECTED)
	return e, jawd, up, closeLink
}

// connects is whatsmeow's Connected for client's connection that began at began, which whatsmeow
// notes before it dispatches Connected.
func connects(client *whatsmeow.Client, began time.Time) *events.Connected {
	client.LastSuccessfulConnect = began
	return &events.Connected{}
}

// saidNext checks the engine's next word to jawd is the connection state want, and returns it.
func saidNext(t *testing.T, jawd *link.Link, want enginepb.Connection_State) *enginepb.Connection {
	t.Helper()
	frame := recv(t, jawd)
	said := frame.GetEvent().GetConnection()
	if said.GetState() != want {
		t.Fatalf("the engine said %v, not %v", frame, want)
	}
	return said
}

// failedPings is whatsmeow's word that `failed` pings have failed since lastPing got through.
func failedPings(failed int, lastPing time.Time) *events.KeepAliveTimeout {
	return &events.KeepAliveTimeout{ErrorCount: failed, LastSuccess: lastPing}
}

// saysNothing fails the test if client's evt goes to jawd.
func saysNothing(t *testing.T, e *Engine, client *whatsmeow.Client, evt any, what string) {
	t.Helper()
	if !returnsAlone(func() { e.handleFrom(client, evt) }) {
		t.Fatalf("%s went to jawd", what)
	}
}

func TestARestoreFromTheRunningClientSaysItsKeepaliveWorksAgain(t *testing.T) {
	const apart = 50 * time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	go e.handleFrom(client, failedPings(1, time.Now()))
	if said := saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT); said.GetReason() != "1 keepalives failed" {
		t.Fatalf("a timeout's reason: %q", said.GetReason())
	}
	// Once the next ping could have got through.
	time.Sleep(3 * apart)
	go e.handleFrom(client, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}

func TestARestoreFromAClientReplacedAfterALogoutSaysNothingWhereTheRunningClientsDoes(t *testing.T) {
	const apart = 50 * time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	old := e.cli()
	go e.handleFrom(old, failedPings(1, time.Now()))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	fresh := loggedOutByThePhone(t, e, jawd, old)
	time.Sleep(3 * apart)
	saysNothing(t, e, fresh, &events.KeepAliveRestored{}, "a restore after the logout said since the timeout")
	saysNothing(t, e, old, &events.KeepAliveRestored{}, "the replaced client's restore")
	// The phone links jaw again, and the fresh client's keepalive times out.
	paired(t, fresh.Store, 13)
	go e.handleFrom(fresh, connects(fresh, time.Now()))
	saidNext(t, jawd, enginepb.Connection_CONNECTED)
	go e.handleFrom(fresh, failedPings(1, time.Now()))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	time.Sleep(3 * apart)
	saysNothing(t, e, old, &events.KeepAliveRestored{}, "the replaced client's restore, where the running client's would be said")
	go e.handleFrom(fresh, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}

func TestAKeepalivesWordOlderThanALaterConnectionStateSaysNothing(t *testing.T) {
	const apart = 50 * time.Millisecond
	for _, later := range []struct {
		evt   any
		state enginepb.Connection_State
	}{
		{&events.Disconnected{}, enginepb.Connection_DISCONNECTED},
		{&events.Connected{}, enginepb.Connection_CONNECTED},
		{&events.StreamReplaced{}, enginepb.Connection_STREAM_REPLACED},
		{&events.ClientOutdated{}, enginepb.Connection_CLIENT_OUTDATED},
		{&events.ConnectFailure{}, enginepb.Connection_CONNECT_FAILED},
		{&events.TemporaryBan{}, enginepb.Connection_TEMPORARY_BAN},
	} {
		e, jawd, _, closeLink := connectedEngine(t, apart)
		client := e.cli()
		lastPing := time.Now()
		go e.handleFrom(client, failedPings(1, lastPing))
		saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
		evt := later.evt
		if _, ok := evt.(*events.Connected); ok {
			evt = connects(client, time.Now())
		}
		go e.handleFrom(client, evt)
		saidNext(t, jawd, later.state)
		// Long enough that only the later state can tell the restore is older.
		time.Sleep(3 * apart)
		saysNothing(t, e, client, &events.KeepAliveRestored{}, "a restore after a later "+later.state.String())
		// A timeout whose failed ping, a ping after the last that got through, can have come
		// before the state was said, as one made before it.
		older := failedPings(1, lastPing.Add(-2*apart))
		saysNothing(t, e, client, older, "a timeout older than a later "+later.state.String())
		closeLink()
	}
}

func TestARestoreWhileTheClientIsntConnectedSaysNothing(t *testing.T) {
	const apart = 50 * time.Millisecond
	e, jawd, up, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	go e.handleFrom(client, failedPings(1, time.Now()))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	time.Sleep(3 * apart)
	up.Store(false)
	saysNothing(t, e, client, &events.KeepAliveRestored{}, "a restore with no connection")
	up.Store(true)
	go e.handleFrom(client, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}

func TestARestoreSoonerThanTheNextPingAfterTheTimeoutCouldHaveGotThroughSaysNothing(t *testing.T) {
	const apart = time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	lastPing := time.Now()
	go e.handleFrom(client, failedPings(1, lastPing))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	saysNothing(t, e, client, &events.KeepAliveRestored{}, "an earlier run's restore")
	time.Sleep(time.Until(lastPing.Add(2*apart + apart/2)))
	go e.handleFrom(client, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}

func TestATimeoutOlderThanTheTimeoutSaidLastSaysNothingAndALaterOneOfItsRunIsSaid(t *testing.T) {
	const apart = time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	run := time.Now()
	go e.handleFrom(client, failedPings(2, run))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	saysNothing(t, e, client, failedPings(1, run), "the run's first failure, after its second")
	saysNothing(t, e, client, failedPings(5, run.Add(-time.Minute)), "an earlier run's failure")
	go e.handleFrom(client, failedPings(3, run))
	if said := saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT); said.GetReason() != "3 keepalives failed" {
		t.Fatalf("the run's third failure said %q", said.GetReason())
	}
}

// A restore and a Disconnected dispatched at once, each on a goroutine of its own as whatsmeow
// does: whichever is said first, jawd's last word is the Disconnected.
func TestARestoreRacingADisconnectNeverHasTheLastWord(t *testing.T) {
	const apart = time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	said := make(chan enginepb.Connection_State, 64)
	go func() {
		for {
			frame, err := jawd.Recv()
			if err != nil {
				return
			}
			if c := frame.GetEvent().GetConnection(); c != nil {
				said <- c.GetState()
			}
		}
	}()
	next := func() enginepb.Connection_State {
		select {
		case state := <-said:
			return state
		case <-time.After(10 * time.Second):
			t.Fatal("the engine said nothing more")
			return 0
		}
	}
	const rounds = 200
	restoredLast := 0
	for round := range rounds {
		go e.handleFrom(client, failedPings(1, time.Now()))
		if state := next(); state != enginepb.Connection_KEEPALIVE_TIMEOUT {
			t.Fatalf("round %d: the timeout said %v", round, state)
		}
		// Once the next ping could have got through.
		time.Sleep(3 * apart)
		start := make(chan struct{})
		var both sync.WaitGroup
		both.Add(2)
		go func() { defer both.Done(); <-start; e.handleFrom(client, &events.KeepAliveRestored{}) }()
		go func() { defer both.Done(); <-start; e.handleFrom(client, &events.Disconnected{}) }()
		close(start)
		both.Wait()
		// Said after both, so every word of theirs comes before it.
		go e.handleFrom(client, connects(client, time.Now()))
		var last enginepb.Connection_State
		for state := next(); state != enginepb.Connection_CONNECTED; state = next() {
			last = state
		}
		if last != enginepb.Connection_DISCONNECTED {
			restoredLast++
		}
	}
	if restoredLast > 0 {
		t.Fatalf("in %d of %d rounds jawd's last word was the restore, after the Disconnected", restoredLast, rounds)
	}
}

// The engine whatsmeow runs in counts pings whatsmeow's least interval apart, which the timeout's and
// the restore's bounds count on.
func TestTheEngineCountsPingsWhatsmeowsLeastIntervalApart(t *testing.T) {
	e, _, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	if e.told.apart != whatsmeow.KeepAliveIntervalMin {
		t.Fatalf("pings counted %v apart, where whatsmeow's least is %v", e.told.apart, whatsmeow.KeepAliveIntervalMin)
	}
}

// A timeout is said after a state only when its failure can't have come before it: the nth failure
// since the last ping that got through comes at least n pings after that ping.
func TestATimeoutIsSaidAfterAStateOnlyOnceItsFailureCantHaveComeBeforeIt(t *testing.T) {
	const apart = 2 * time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	lastPing := time.Now().Add(-apart * 3 / 2)
	saysNothing(t, e, client, failedPings(1, lastPing), "a first failure that could have come before the Connected")
	go e.handleFrom(client, failedPings(2, lastPing))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
}

// A timeout is judged against the state said last, not an earlier one.
func TestATimeoutIsJudgedAgainstTheStateSaidLast(t *testing.T) {
	const apart = 2 * time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	time.Sleep(apart)
	go e.handleFrom(client, &events.Disconnected{})
	saidNext(t, jawd, enginepb.Connection_DISCONNECTED)
	lastPing := time.Now().Add(-apart * 3 / 2)
	saysNothing(t, e, client, failedPings(1, lastPing), "a timeout that could have come before the Disconnected")
}

// whatsmeow's keepalive runs from the handshake on, and its Connected comes only once the IQs after
// it are answered, so a timeout after a late Connected is judged by when its connection began: the
// connection's own is said, and one from before it isn't.
func TestATimeoutAfterALateConnectedIsJudgedByWhenItsConnectionBegan(t *testing.T) {
	const apart = 200 * time.Millisecond
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	go e.handleFrom(client, &events.Disconnected{})
	saidNext(t, jawd, enginepb.Connection_DISCONNECTED)
	// whatsmeow connects again, notes the connection began a round trip after the handshake its
	// keepalive starts at, and says Connected three pings later.
	handshake := time.Now()
	began := handshake.Add(apart / 10)
	time.Sleep(time.Until(began.Add(3 * apart)))
	go e.handleFrom(client, connects(client, began))
	saidNext(t, jawd, enginepb.Connection_CONNECTED)
	saysNothing(t, e, client, failedPings(1, handshake.Add(-2*apart)), "a failure of the connection before")
	go e.handleFrom(client, failedPings(1, handshake))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
}

// After n failures since the last ping that got through, a restore can't come sooner than the
// (n+1)th ping after it.
func TestARestoreIsSaidOnlyOnceTheNextPingCouldHaveGotThrough(t *testing.T) {
	const apart = 2 * time.Second
	e, jawd, _, closeLink := connectedEngine(t, apart)
	defer closeLink()
	client := e.cli()
	lastPing := time.Now()
	go e.handleFrom(client, failedPings(1, lastPing))
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_TIMEOUT)
	time.Sleep(time.Until(lastPing.Add(apart * 3 / 2)))
	saysNothing(t, e, client, &events.KeepAliveRestored{}, "a restore between the first and second pings after the last that got through")
	time.Sleep(time.Until(lastPing.Add(apart * 5 / 2)))
	go e.handleFrom(client, &events.KeepAliveRestored{})
	saidNext(t, jawd, enginepb.Connection_KEEPALIVE_RESTORED)
}
