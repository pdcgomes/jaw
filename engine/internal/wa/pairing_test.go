package wa

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"fmt"
	"slices"
	"strings"
	"sync"
	"testing"
	"time"

	"go.mau.fi/libsignal/ecc"
	"go.mau.fi/whatsmeow"
	waBinary "go.mau.fi/whatsmeow/binary"
	"go.mau.fi/whatsmeow/proto/waAdv"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"go.mau.fi/whatsmeow/util/keys"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// fakeReach stands in for WhatsApp: nothing connects, a logout does what whatsmeow's does once
// WhatsApp has answered (unless it fails as told), and it counts what it was asked.
type fakeReach struct {
	sync.Mutex
	connects, disconnects int
	isConnected           bool
	logoutFails           error
}

func (f *fakeReach) connect(*whatsmeow.Client) error {
	f.Lock()
	defer f.Unlock()
	f.connects++
	return nil
}

func (f *fakeReach) disconnect(*whatsmeow.Client) {
	f.Lock()
	defer f.Unlock()
	f.disconnects++
}

func (f *fakeReach) logout(ctx context.Context, client *whatsmeow.Client) error {
	if f.logoutFails != nil {
		return f.logoutFails
	}
	client.Disconnect()
	return client.Store.Delete(ctx)
}

func (f *fakeReach) connected(*whatsmeow.Client) bool {
	f.Lock()
	defer f.Unlock()
	return f.isConnected
}

func (f *fakeReach) asked() (connects, disconnects int) {
	f.Lock()
	defer f.Unlock()
	return f.connects, f.disconnects
}

// unlinkedEngine is an engine whose session.db, in a folder of its own, has no device, reaching
// WhatsApp only through fake, and jawd's end of its link.
func unlinkedEngine(t *testing.T, fake *fakeReach) (*Engine, *link.Link, func()) {
	t.Helper()
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	if err := e.open(t.TempDir()); err != nil {
		t.Fatal(err)
	}
	e.reach = fake
	return e, jawd, closeLink
}

// pairedByThePhone has the phone pair client's device as whatsmeow does it, its ID saved before
// PairSuccess, and checks the engine tells jawd nothing of it yet. A link still finishing when the
// test ends is let go, so its time doesn't run out after.
func pairedByThePhone(t *testing.T, e *Engine, client *whatsmeow.Client) {
	t.Helper()
	t.Cleanup(func() {
		e.pairing.Lock()
		e.pairing.end()
		e.pairing.Unlock()
	})
	paired(t, client.Store, 21)
	success := &events.PairSuccess{ID: *client.Store.ID, LID: types.NewJID("123456789", types.HiddenUserServer), Platform: "android"}
	if !returnsAlone(func() { e.handleFrom(client, success) }) {
		t.Fatal("the phone pairing went to jawd before the link finished")
	}
}

// replyTo is the engine's answer to cmd, sent as command id.
func replyTo(t *testing.T, jawd *link.Link, id uint64, cmd *enginepb.Command) *enginepb.Reply {
	t.Helper()
	command(t, jawd, id, cmd)
	frame := recv(t, jawd)
	if frame.Id != id || frame.GetReply() == nil {
		t.Fatalf("command %d got %v", id, frame)
	}
	return frame.GetReply()
}

func linkStart() *enginepb.Command {
	return &enginepb.Command{Kind: &enginepb.Command_LinkStart{LinkStart: &enginepb.LinkStart{}}}
}

func logout() *enginepb.Command {
	return &enginepb.Command{Kind: &enginepb.Command_Logout{Logout: &enginepb.Logout{}}}
}

func linkCancel(onlyLinking bool) *enginepb.Command {
	return &enginepb.Command{Kind: &enginepb.Command_LinkCancel{LinkCancel: &enginepb.LinkCancel{OnlyLinking: onlyLinking}}}
}

func refused(reply *enginepb.Reply, saying string, retry bool) bool {
	f := reply.GetFailure()
	return f != nil && strings.Contains(f.Message, saying) && f.Retry == retry
}

func TestALinkThePhoneHasPairedNamesTheAccountOnlyOnceItsClientConnects(t *testing.T) {
	e, jawd, closeLink := unlinkedEngine(t, &fakeReach{})
	defer closeLink()
	client := e.cli()
	pairedByThePhone(t, e, client)
	go e.handleFrom(client, &events.Disconnected{})
	if said := recv(t, jawd).GetEvent().GetConnection(); said.GetState() != enginepb.Connection_DISCONNECTED || said.GetJid() != "" {
		t.Fatalf("while the link finishes, the connection said %v", said)
	}
	if reply := replyTo(t, jawd, 1, logout()); !refused(reply, "no account is linked", false) {
		t.Fatalf("a logout while the link finishes got %v", reply)
	}
	groups := &enginepb.Command{Kind: &enginepb.Command_Groups{Groups: &enginepb.Groups{}}}
	if reply := replyTo(t, jawd, 2, groups); !refused(reply, "no account is linked yet", false) {
		t.Fatalf("asking for groups while the link finishes got %v", reply)
	}
	go e.handleFrom(client, &events.Connected{})
	done := recv(t, jawd).GetEvent().GetLink().GetDone()
	if done.GetJid() != client.Store.ID.String() || done.GetPlatform() != "android" {
		t.Fatalf("the link finished as %v", done)
	}
	if said := recv(t, jawd).GetEvent().GetConnection(); said.GetState() != enginepb.Connection_CONNECTED || said.GetJid() != client.Store.ID.String() {
		t.Fatalf("after the link finished, the connection said %v", said)
	}
	if e.cli() != client || e.half(client) || devicesIn(t, e) != 1 {
		t.Fatalf("the linked device: kept %v, half made %v, %d in session.db", e.cli() == client, e.half(client), devicesIn(t, e))
	}
}

func TestALinkWhatsAppRefusesAfterThePhonePairedLeavesNoDeviceAndSaysWhy(t *testing.T) {
	for _, c := range []struct {
		refusal any
		why     string
		then    enginepb.Connection_State
	}{
		{&events.LoggedOut{Reason: events.ConnectFailureLoggedOut}, "WhatsApp refused the new device: logged out from another device", enginepb.Connection_STATE_UNSPECIFIED},
		{&events.ConnectFailure{Reason: events.ConnectFailureClientUnknown, Message: "unknown"}, "WhatsApp refused the new device: ", enginepb.Connection_CONNECT_FAILED},
		{&events.ClientOutdated{}, "WhatsApp wants a newer version of jaw", enginepb.Connection_CLIENT_OUTDATED},
		{&events.TemporaryBan{Code: events.TempBanSentToTooManyPeople, Expire: time.Hour}, "banned", enginepb.Connection_TEMPORARY_BAN},
	} {
		t.Run(fmt.Sprintf("%T", c.refusal), func(t *testing.T) {
			fake := &fakeReach{}
			e, jawd, closeLink := unlinkedEngine(t, fake)
			defer closeLink()
			client := e.cli()
			pairedByThePhone(t, e, client)
			if _, ok := c.refusal.(*events.LoggedOut); ok {
				// whatsmeow deletes the device as it dispatches LoggedOut.
				if err := client.Store.Delete(context.Background()); err != nil {
					t.Fatal(err)
				}
			}
			go e.handleFrom(client, c.refusal)
			failed := recv(t, jawd).GetEvent().GetLink()
			if !strings.Contains(failed.GetFailed(), c.why) {
				t.Fatalf("the link failed saying %v", failed)
			}
			fresh := renewed(t, e, client)
			if c.then != enginepb.Connection_STATE_UNSPECIFIED {
				if said := recv(t, jawd).GetEvent().GetConnection(); said.GetState() != c.then || said.GetJid() != "" {
					t.Fatalf("then the connection said %v", said)
				}
			}
			if fresh.Store.ID != nil || devicesIn(t, e) != 0 || e.half(client) {
				t.Fatalf("after the refusal: the fresh device %v, %d in session.db", fresh.Store.ID, devicesIn(t, e))
			}
			if reply := replyTo(t, jawd, 3, linkStart()); reply.GetDone() == nil {
				t.Fatalf("linking again got %v", reply)
			}
			if connects, _ := fake.asked(); connects != 1 {
				t.Fatalf("linking again connected %d times", connects)
			}
		})
	}
}

// saidUntilPong is what the engine says to jawd until it answers a ping, sent now as command id.
func saidUntilPong(t *testing.T, jawd *link.Link, id uint64) []*enginepb.Event {
	t.Helper()
	command(t, jawd, id, &enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}})
	var said []*enginepb.Event
	for {
		frame := recv(t, jawd)
		if frame.Id == id && frame.GetReply().GetPong() != nil {
			return said
		}
		said = append(said, frame.GetEvent())
	}
}

// namesTheAccount says whether event says the link done or names an account, as nothing may for a
// link that failed.
func namesTheAccount(event *enginepb.Event) bool {
	c := event.GetConnection()
	return event.GetLink().GetDone() != nil || c.GetJid() != "" || c.GetState() == enginepb.Connection_CONNECTED
}

func TestAConnectedTheClientWasDispatchingAsItsHalfMadeDeviceWasClearedSaysNothing(t *testing.T) {
	e, jawd, closeLink := unlinkedEngine(t, &fakeReach{})
	defer closeLink()
	client := e.cli()
	pairedByThePhone(t, e, client)
	go e.handleFrom(client, &events.ConnectFailure{Reason: events.ConnectFailureClientUnknown, Message: "unknown"})
	if failed := recv(t, jawd).GetEvent().GetLink().GetFailed(); failed == "" {
		t.Fatal("no link failed")
	}
	if said := recv(t, jawd).GetEvent(); said.GetConnection().GetState() != enginepb.Connection_CONNECT_FAILED || namesTheAccount(said) {
		t.Fatalf("the refusal said %v", said)
	}
	renewed(t, e, client)
	// handleFrom took it as the running client's before the clearing retired it.
	if !returnsAlone(func() { e.say(client, &events.Connected{}) }) {
		t.Fatal("the Connected went to jawd")
	}
	if said := saidUntilPong(t, jawd, 9); len(said) != 0 {
		t.Fatalf("after the clearing the engine said %v", said)
	}
}

func TestAConnectedAsTheClearingRetiresTheClientSaysNothing(t *testing.T) {
	e, jawd, closeLink := unlinkedEngine(t, &fakeReach{})
	defer closeLink()
	client := e.cli()
	pairedByThePhone(t, e, client)
	// The Connected comes once the link has ended and as its client is retired.
	var once sync.Once
	e.retiring = func() {
		once.Do(func() {
			connected := make(chan struct{})
			go func() {
				e.say(client, &events.Connected{})
				close(connected)
			}()
			select {
			case <-connected:
			case <-time.After(300 * time.Millisecond):
			}
		})
	}
	go e.handleFrom(client, &events.ConnectFailure{Reason: events.ConnectFailureClientUnknown, Message: "unknown"})
	var said []*enginepb.Event
	for len(said) == 0 || said[len(said)-1].GetConnection().GetState() != enginepb.Connection_CONNECT_FAILED {
		said = append(said, recv(t, jawd).GetEvent())
	}
	said = append(said, saidUntilPong(t, jawd, 9)...)
	if len(said) != 2 || said[0].GetLink().GetFailed() == "" || namesTheAccount(said[1]) {
		t.Fatalf("a Connected as the client was retired: the engine said %v", said)
	}
	renewed(t, e, client)
}

func TestALinkWhoseClientIsntConnectedAMinuteAfterThePhonePairedIsCleared(t *testing.T) {
	fake := &fakeReach{}
	e, jawd, closeLink := unlinkedEngine(t, fake)
	defer closeLink()
	e.pairing.finishing = 20 * time.Millisecond
	client := e.cli()
	pairedByThePhone(t, e, client)
	failed := recv(t, jawd).GetEvent().GetLink()
	if failed.GetFailed() != "WhatsApp didn't finish linking jaw; link again" {
		t.Fatalf("an unfinished link said %v", failed)
	}
	renewed(t, e, client)
	if devicesIn(t, e) != 0 {
		t.Fatalf("%d devices left in session.db", devicesIn(t, e))
	}

	// One whose client is connected, its IQs still to be answered, is given its time again.
	fake.Lock()
	fake.isConnected = true
	fake.Unlock()
	second := e.cli()
	pairedByThePhone(t, e, second)
	time.Sleep(200 * time.Millisecond)
	if e.cli() != second || !e.half(second) || devicesIn(t, e) != 1 {
		t.Fatalf("a link still connected was cleared: client kept %v, half made %v", e.cli() == second, e.half(second))
	}
	go e.handleFrom(second, &events.Connected{})
	if done := recv(t, jawd).GetEvent().GetLink().GetDone(); done == nil {
		t.Fatal("the link didn't finish")
	}
	if said := recv(t, jawd).GetEvent().GetConnection(); said.GetState() != enginepb.Connection_CONNECTED {
		t.Fatalf("then the connection said %v", said)
	}
}

func TestALinkConnectedButNeverFinishedIsClearedOnceItsLongestHasPassed(t *testing.T) {
	fake := &fakeReach{isConnected: true}
	e, jawd, closeLink := unlinkedEngine(t, fake)
	defer closeLink()
	e.pairing.finishing, e.pairing.longest = 20*time.Millisecond, 300*time.Millisecond
	client := e.cli()
	paired := time.Now()
	pairedByThePhone(t, e, client)
	failed := recv(t, jawd).GetEvent().GetLink()
	if failed.GetFailed() != "WhatsApp didn't finish linking jaw; link again" {
		t.Fatalf("a link connected for its longest said %v", failed)
	}
	if took := time.Since(paired); took < 300*time.Millisecond {
		t.Fatalf("a link still connected was cleared after %v, before its longest", took)
	}
	if fresh := renewed(t, e, client); fresh.Store.ID != nil || devicesIn(t, e) != 0 || e.half(client) {
		t.Fatalf("after its longest: the fresh device %v, %d in session.db", fresh.Store.ID, devicesIn(t, e))
	}
}

func TestLinkStartRefusesOnlyALinkedAccountOrALinkFinishingAndClearsOneThatCant(t *testing.T) {
	fake := &fakeReach{}
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	e.reach = fake
	if reply := replyTo(t, jawd, 1, linkStart()); !refused(reply, "an account is already linked; log out first", false) {
		t.Fatalf("linking with an account linked got %v", reply)
	}

	fake = &fakeReach{isConnected: true}
	e, jawd, closeLink = unlinkedEngine(t, fake)
	defer closeLink()
	client := e.cli()
	pairedByThePhone(t, e, client)
	if reply := replyTo(t, jawd, 2, linkStart()); !refused(reply, "linking is finishing", true) {
		t.Fatalf("linking again while the link finishes, connected, got %v", reply)
	}
	fake.Lock()
	fake.isConnected = false
	fake.Unlock()
	if reply := replyTo(t, jawd, 3, linkStart()); reply.GetDone() == nil {
		t.Fatalf("linking again once the link can't finish got %v", reply)
	}
	if fresh := e.cli(); fresh == client || fresh.Store.ID != nil || devicesIn(t, e) != 0 {
		t.Fatalf("linking again kept the half-made device: client kept %v, %d in session.db", fresh == client, devicesIn(t, e))
	}
	if connects, _ := fake.asked(); connects != 1 {
		t.Fatalf("linking again connected %d times", connects)
	}
}

func TestAPairingThatFailsOnceWhatsmeowHasGivenTheDeviceItsIDLeavesNoDevice(t *testing.T) {
	jid := types.JID{User: "351910000001", Device: 22, Server: types.DefaultUserServer}
	for _, c := range []struct {
		name string
		left func(t *testing.T, device *store.Device)
	}{
		{"its ID set, not saved", func(_ *testing.T, device *store.Device) { device.ID = &jid }},
		{"saved, then deleted by whatsmeow", func(t *testing.T, device *store.Device) {
			paired(t, device, 22)
			if err := device.Delete(context.Background()); err != nil {
				t.Fatal(err)
			}
		}},
	} {
		t.Run(c.name, func(t *testing.T) {
			fake := &fakeReach{}
			e, jawd, closeLink := unlinkedEngine(t, fake)
			defer closeLink()
			client := e.cli()
			c.left(t, client.Store)
			// The QR codes' channel says why; the engine says nothing itself.
			if !returnsAlone(func() { e.handleFrom(client, &events.PairError{ID: jid, Error: errPairing}) }) {
				t.Fatal("the pair error went to jawd from the engine's handler")
			}
			if fresh := e.cli(); fresh == client || fresh.Store.ID != nil || fresh.Store.Deleted || devicesIn(t, e) != 0 {
				t.Fatalf("after the pair error: client kept %v, %d devices in session.db", fresh == client, devicesIn(t, e))
			}
			if reply := replyTo(t, jawd, 4, linkStart()); reply.GetDone() == nil {
				t.Fatalf("linking again got %v", reply)
			}
		})
	}

	// A pairing that failed before the device had an ID leaves the device as it was.
	e, _, closeLink := unlinkedEngine(t, &fakeReach{})
	defer closeLink()
	client := e.cli()
	if !returnsAlone(func() { e.handleFrom(client, &events.PairError{Error: whatsmeow.ErrPairInvalidDeviceIdentityHMAC}) }) {
		t.Fatal("the pair error went to jawd from the engine's handler")
	}
	if e.cli() != client {
		t.Fatal("a pairing that never gave the device an ID replaced it")
	}
}

// errPairing is how whatsmeow's pairing fails here, once it has given the device its ID.
var errPairing = fmt.Errorf("failed to save device store: %w", context.DeadlineExceeded)

func TestALogoutJawdAsksForLeavesNoDeviceAndOneWhatsAppDoesntAnswerLeavesTheAccount(t *testing.T) {
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	e.reach = &fakeReach{logoutFails: fmt.Errorf("error sending logout request: %w", whatsmeow.ErrNotConnected)}
	old := e.cli()
	if reply := replyTo(t, jawd, 1, logout()); !refused(reply, "can't log out: error sending logout request", true) {
		t.Fatalf("a logout WhatsApp didn't hear got %v", reply)
	}
	if e.cli() != old || devicesIn(t, e) != 1 {
		t.Fatalf("a logout WhatsApp didn't hear: client kept %v, %d devices in session.db", e.cli() == old, devicesIn(t, e))
	}
	e.reach = &fakeReach{}
	if reply := replyTo(t, jawd, 2, logout()); reply.GetDone() == nil {
		t.Fatalf("a logout got %v", reply)
	}
	if fresh := renewed(t, e, old); fresh.Store.ID != nil || devicesIn(t, e) != 0 {
		t.Fatalf("after the logout: the fresh device %v, %d in session.db", fresh.Store.ID, devicesIn(t, e))
	}
	if reply := replyTo(t, jawd, 3, logout()); !refused(reply, "no account is linked", false) {
		t.Fatalf("a logout with no account linked got %v", reply)
	}
}

// pairSuccess is what WhatsApp sends client once the phone of account jid has paired its device,
// signed for the device's keys as whatsmeow checks it before it asks PrePairCallback.
func pairSuccess(t *testing.T, client *whatsmeow.Client, jid types.JID) *waBinary.Node {
	t.Helper()
	details, err := proto.Marshal(&waAdv.ADVDeviceIdentity{RawID: proto.Uint32(7), Timestamp: proto.Uint64(1_792_000_000), KeyIndex: proto.Uint32(1)})
	if err != nil {
		t.Fatal(err)
	}
	phone := keys.NewKeyPair()
	signed := ecc.CalculateSignature(ecc.NewDjbECPrivateKey(*phone.Priv), slices.Concat(whatsmeow.AdvAccountSignaturePrefix, details, client.Store.IdentityKey.Pub[:]))
	identity, err := proto.Marshal(&waAdv.ADVSignedDeviceIdentity{Details: details, AccountSignatureKey: phone.Pub[:], AccountSignature: signed[:]})
	if err != nil {
		t.Fatal(err)
	}
	mac := hmac.New(sha256.New, client.Store.AdvSecretKey)
	mac.Write(identity)
	container, err := proto.Marshal(&waAdv.ADVSignedDeviceIdentityHMAC{Details: identity, HMAC: mac.Sum(nil)})
	if err != nil {
		t.Fatal(err)
	}
	lid := types.JID{User: "123456789", Device: jid.Device, Server: types.HiddenUserServer}
	return &waBinary.Node{Tag: "iq", Attrs: waBinary.Attrs{"id": "pair-1", "t": "1792000000"}, Content: []waBinary.Node{{
		Tag: "pair-success",
		Content: []waBinary.Node{
			{Tag: "device-identity", Content: container},
			{Tag: "device", Attrs: waBinary.Attrs{"jid": jid, "lid": lid}},
			{Tag: "platform", Attrs: waBinary.Attrs{"name": "android"}},
		},
	}}}
}

func linkStartKeeping(account string) *enginepb.Command {
	return &enginepb.Command{Kind: &enginepb.Command_LinkStart{LinkStart: &enginepb.LinkStart{Account: account}}}
}

func TestAFolderThatKeepsAnAccountsMessagesLinksOnlyThatAccountAndAnotherIsRefusedLeavingNothing(t *testing.T) {
	pedro := types.JID{User: "351910000001", Device: 23, Server: types.DefaultUserServer}
	bea := types.JID{User: "351919999999", Device: 24, Server: types.DefaultUserServer}
	fake := &fakeReach{}
	e, jawd, closeLink := unlinkedEngine(t, fake)
	defer closeLink()
	client := e.cli()
	if reply := replyTo(t, jawd, 1, linkStartKeeping("351910000001")); reply.GetDone() == nil {
		t.Fatalf("linking got %v", reply)
	}
	// Another account pairs: whatsmeow asks PrePairCallback before it gives the device an ID.
	client.DangerousInternals().HandlePairSuccess(context.Background(), pairSuccess(t, client, bea))
	refused := recv(t, jawd).GetEvent().GetLink().GetRefused()
	if refused.GetAccount() != "351910000001" || refused.GetOther() != "351919999999" {
		t.Fatalf("another account linking said %v", refused)
	}
	if said := saidUntilPong(t, jawd, 2); len(said) != 0 {
		t.Fatalf("then the engine said %v", said)
	}
	if e.cli() != client || client.Store.ID != nil || client.Store.Deleted || devicesIn(t, e) != 0 {
		t.Fatalf("after the refusal: client kept %v, its ID %v, %d devices in session.db", e.cli() == client, client.Store.ID, devicesIn(t, e))
	}

	// Linking starts again at once, and the account jaw.db keeps pairs on to WhatsApp's
	// confirmation, which can't go here, so whatsmeow deletes the device and the engine renews.
	if reply := replyTo(t, jawd, 3, linkStartKeeping("351910000001")); reply.GetDone() == nil {
		t.Fatalf("linking again got %v", reply)
	}
	if connects, _ := fake.asked(); connects != 2 {
		t.Fatalf("linking twice connected %d times", connects)
	}
	client.DangerousInternals().HandlePairSuccess(context.Background(), pairSuccess(t, client, pedro))
	if failed := recv(t, jawd).GetEvent().GetLink().GetFailed(); !strings.Contains(failed, "pairing confirmation") {
		t.Fatalf("the account jaw.db keeps didn't pair on: %q", failed)
	}
	fresh := renewed(t, e, client)

	// A folder that keeps no account's messages links any.
	if reply := replyTo(t, jawd, 4, linkStart()); reply.GetDone() == nil {
		t.Fatalf("linking a folder that keeps none got %v", reply)
	}
	fresh.DangerousInternals().HandlePairSuccess(context.Background(), pairSuccess(t, fresh, bea))
	if failed := recv(t, jawd).GetEvent().GetLink().GetFailed(); !strings.Contains(failed, "pairing confirmation") {
		t.Fatalf("a folder that keeps none didn't take another account: %q", failed)
	}
	renewed(t, e, fresh)
}

func TestAnInterfacesLinkCancelStopsOnlyALinkThePhoneHasntPaired(t *testing.T) {
	fake := &fakeReach{}
	e, jawd, closeLink := unlinkedEngine(t, fake)
	defer closeLink()
	if reply := replyTo(t, jawd, 1, linkCancel(true)); reply.GetDone() == nil {
		t.Fatalf("cancelling linking got %v", reply)
	}
	if _, disconnects := fake.asked(); disconnects != 1 {
		t.Fatalf("cancelling linking disconnected %d times", disconnects)
	}
	pairedByThePhone(t, e, e.cli())
	if reply := replyTo(t, jawd, 2, linkCancel(true)); reply.GetDone() == nil {
		t.Fatalf("cancelling a link the phone has paired got %v", reply)
	}
	if _, disconnects := fake.asked(); disconnects != 1 {
		t.Fatal("an interface's cancel disconnected a link the phone has paired")
	}

	fake = &fakeReach{}
	e, jawd, closeLink = linkedEngine(t)
	defer closeLink()
	e.reach = fake
	if reply := replyTo(t, jawd, 3, linkCancel(true)); reply.GetDone() == nil {
		t.Fatalf("an interface's cancel with an account linked got %v", reply)
	}
	if _, disconnects := fake.asked(); disconnects != 0 {
		t.Fatal("an interface's cancel disconnected the linked account")
	}
	if reply := replyTo(t, jawd, 4, linkCancel(false)); reply.GetDone() == nil {
		t.Fatalf("jawd's disconnect at a stop got %v", reply)
	}
	if _, disconnects := fake.asked(); disconnects != 1 {
		t.Fatal("jawd's disconnect at a stop didn't disconnect")
	}
}
