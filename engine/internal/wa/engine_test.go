package wa

import (
	"context"
	"fmt"
	"io"
	"strings"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waAdv"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// engineWithoutSession is an engine on one end of a pair of pipes, with no whatsmeow session, and
// jawd's end of the link, after both have said hello. jawd's hello carries `protocol`.
func engineWithoutSession(t *testing.T, protocol uint32) (*Engine, *link.Link, chan error, func()) {
	t.Helper()
	toEngine, fromJawd := io.Pipe()
	toJawd, fromEngine := io.Pipe()
	e := newEngine(context.Background(), link.New(toEngine, fromEngine), NewLogger("test", "error"))
	jawd := link.New(toJawd, fromJawd)
	greeted := make(chan error, 1)
	go func() { greeted <- e.hello("jaw-engine test", "v-test") }()
	first, err := jawd.Recv()
	if err != nil || first.GetHello() == nil {
		t.Fatalf("the engine's first frame: %v, %v", first, err)
	}
	if got := first.GetHello().Protocol; got != link.Protocol {
		t.Fatalf("the engine says protocol %d", got)
	}
	if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: &enginepb.Hello{Protocol: protocol, Program: "jawd test"}}}); err != nil {
		t.Fatal(err)
	}
	served := make(chan error, 1)
	if err := <-greeted; err != nil {
		served <- err
		return e, jawd, served, func() {}
	}
	go func() { served <- e.serve() }()
	return e, jawd, served, func() { _ = fromJawd.Close() }
}

func command(t *testing.T, jawd *link.Link, id uint64, cmd *enginepb.Command) {
	t.Helper()
	if err := jawd.Send(&enginepb.Frame{Id: id, Body: &enginepb.Frame_Command{Command: cmd}}); err != nil {
		t.Fatal(err)
	}
}

func TestHelloThenPing(t *testing.T) {
	_, jawd, served, closeLink := engineWithoutSession(t, link.Protocol)
	command(t, jawd, 1, &enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}})
	reply, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	if reply.Id != 1 || reply.GetReply().GetPong() == nil {
		t.Fatalf("a ping got %v", reply)
	}
	closeLink()
	if err := <-served; err != nil {
		t.Errorf("serve ended with %v after jawd closed the link", err)
	}
}

func TestAMismatchedProtocolIsRefused(t *testing.T) {
	_, _, served, _ := engineWithoutSession(t, link.Protocol+1)
	err := <-served
	if err == nil || !strings.Contains(err.Error(), "link protocol") {
		t.Fatalf("a mismatched hello gave %v", err)
	}
}

func TestEachBenchEventWaitsForItsAck(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	command(t, jawd, 9, &enginepb.Command{Kind: &enginepb.Command_BenchRun{BenchRun: &enginepb.BenchRun{Count: 50, Size: 100}}})
	var acked uint64
	for {
		frame, err := jawd.Recv()
		if err != nil {
			t.Fatal(err)
		}
		if result := frame.GetReply().GetBenchResult(); result != nil {
			if frame.Id != 9 || result.Count != 50 || acked != 50 {
				t.Fatalf("the bench's result %v, id %d, after %d acks", result, frame.Id, acked)
			}
			return
		}
		event := frame.GetEvent()
		if event.GetBench() == nil || len(event.GetBench().Payload) != 100 {
			t.Fatalf("a bench sent %v", frame)
		}
		if waiting := e.acks.Waiting(); waiting != 1 {
			t.Fatalf("%d events waiting for an ack; a bench sends the next only after the last's", waiting)
		}
		if event.Seq != acked+1 {
			t.Fatalf("seq %d after %d acks", event.Seq, acked)
		}
		acked = event.Seq
		if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Ack{Ack: &enginepb.Ack{Seq: event.Seq}}}); err != nil {
			t.Fatal(err)
		}
	}
}

func TestCommandsThatNeedASessionSaySo(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	command(t, jawd, 3, &enginepb.Command{Kind: &enginepb.Command_LinkStart{LinkStart: &enginepb.LinkStart{}}})
	reply, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	if failure := reply.GetReply().GetFailure(); reply.Id != 3 || failure == nil || !strings.Contains(failure.Message, "no session") {
		t.Fatalf("linking without a session gave %v", reply)
	}
}

func TestJawdGoingReleasesWhatWaits(t *testing.T) {
	e, jawd, served, closeLink := engineWithoutSession(t, link.Protocol)
	command(t, jawd, 4, &enginepb.Command{Kind: &enginepb.Command_BenchRun{BenchRun: &enginepb.BenchRun{Count: 10, Size: 1}}})
	if frame, err := jawd.Recv(); err != nil || frame.GetEvent().GetBench() == nil {
		t.Fatalf("the first bench event: %v, %v", frame, err)
	}
	closeLink()
	select {
	case <-e.acks.Gone():
	case <-time.After(2 * time.Second):
		t.Fatal("the engine didn't notice jawd had gone")
	}
	if err := <-served; err != nil {
		t.Errorf("serve ended with %v", err)
	}
}

// recv is jawd's next frame from the engine, failing the test if none comes in time.
func recv(t *testing.T, jawd *link.Link) *enginepb.Frame {
	t.Helper()
	type received struct {
		frame *enginepb.Frame
		err   error
	}
	got := make(chan received, 1)
	go func() {
		frame, err := jawd.Recv()
		got <- received{frame, err}
	}()
	select {
	case r := <-got:
		if r.err != nil {
			t.Fatal(r.err)
		}
		return r.frame
	case <-time.After(10 * time.Second):
		t.Fatal("the engine sent jawd nothing")
		return nil
	}
}

// linkedEngine is an engine whose session.db, in a folder of its own, has a device for an account,
// as one that has linked does, and jawd's end of its link.
func linkedEngine(t *testing.T) (*Engine, *link.Link, func()) {
	t.Helper()
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	if err := e.open(t.TempDir()); err != nil {
		t.Fatal(err)
	}
	paired(t, e.cli().Store, 12)
	return e, jawd, closeLink
}

// paired stores device as linked to the account as its device number n, as pairing would, with
// nothing sent to WhatsApp.
func paired(t *testing.T, device *store.Device, n uint16) {
	t.Helper()
	device.ID = &types.JID{User: "351910000001", Device: n, Server: types.DefaultUserServer}
	// The account's signatures, made up but of the lengths session.db checks.
	device.Account = &waAdv.ADVSignedDeviceIdentity{
		Details:             []byte{1},
		AccountSignature:    make([]byte, 64),
		AccountSignatureKey: make([]byte, 32),
		DeviceSignature:     make([]byte, 64),
	}
	if err := device.Save(context.Background()); err != nil {
		t.Fatal(err)
	}
}

// renewed waits for the engine to have replaced old with a client on a fresh device, and returns it.
func renewed(t *testing.T, e *Engine, old *whatsmeow.Client) *whatsmeow.Client {
	t.Helper()
	deadline := time.Now().Add(2 * time.Second)
	for e.cli() == old && time.Now().Before(deadline) {
		time.Sleep(10 * time.Millisecond)
	}
	if e.cli() == old {
		t.Fatal("no fresh client")
	}
	return e.cli()
}

// loggedOutByThePhone has the phone log client's device out, as whatsmeow does it, checks jawd
// heard so, and returns the client the engine gives itself for linking again.
func loggedOutByThePhone(t *testing.T, e *Engine, jawd *link.Link, client *whatsmeow.Client) *whatsmeow.Client {
	t.Helper()
	// whatsmeow deletes the device as it dispatches LoggedOut.
	if err := client.Store.Delete(context.Background()); err != nil {
		t.Fatal(err)
	}
	go e.handleFrom(client, &events.LoggedOut{Reason: events.ConnectFailureLoggedOut})
	if said := recv(t, jawd).GetEvent().GetConnection(); said.GetState() != enginepb.Connection_LOGGED_OUT {
		t.Fatalf("a logout said %v", said)
	}
	return renewed(t, e, client)
}

func acked(t *testing.T, jawd *link.Link, seq uint64) {
	t.Helper()
	if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Ack{Ack: &enginepb.Ack{Seq: seq}}}); err != nil {
		t.Fatal(err)
	}
}

// returnsAlone says whether fn returns without sending jawd anything, which it couldn't do without
// jawd reading it from the pipe.
func returnsAlone(fn func()) bool {
	returned := make(chan struct{})
	go func() {
		fn()
		close(returned)
	}()
	select {
	case <-returned:
		return true
	case <-time.After(2 * time.Second):
		return false
	}
}

func devicesIn(t *testing.T, e *Engine) int {
	t.Helper()
	devices, err := e.container.GetAllDevices(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	return len(devices)
}

func TestALogoutGivesAFreshDeviceToLinkAgainWithAndDropsWhatTheOldClientSaysOfItsConnection(t *testing.T) {
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	old := e.cli()
	// whatsmeow deletes the device as it dispatches LoggedOut.
	if err := old.Store.Delete(context.Background()); err != nil {
		t.Fatal(err)
	}
	go e.handleFrom(old, &events.LoggedOut{Reason: events.ConnectFailureLoggedOut})
	frame := recv(t, jawd)
	said := frame.GetEvent().GetConnection()
	if said.GetState() != enginepb.Connection_LOGGED_OUT || said.GetReason() != "logged out from another device" {
		t.Fatalf("a logout said %v", frame)
	}
	fresh := renewed(t, e, old)
	if fresh == old || fresh.Store.ID != nil || fresh.Store.Deleted {
		t.Fatalf("after the logout the client is %p (was %p), its device %v, deleted %v", fresh, old, fresh.Store.ID, fresh.Store.Deleted)
	}
	if n := devicesIn(t, e); n != 0 {
		t.Fatalf("%d devices left in session.db", n)
	}
	if !returnsAlone(func() { e.handleFrom(old, &events.Connected{}) }) {
		t.Fatal("the old client's Connected went to jawd")
	}
	if !returnsAlone(func() { e.handleFrom(old, &events.LoggedOut{Reason: events.ConnectFailureLoggedOut}) }) {
		t.Fatal("the old client's second LoggedOut went to jawd")
	}
	kept := &events.Message{
		Info:    types.MessageInfo{MessageSource: types.MessageSource{Chat: types.NewJID("351912345678", types.DefaultUserServer)}, ID: "3EB0LATE"},
		Message: &waE2E.Message{Conversation: proto.String("still kept")},
	}
	go e.handleFrom(old, kept)
	frame = recv(t, jawd)
	if frame.GetEvent().GetMessage().GetInfo().GetId() != "3EB0LATE" || frame.GetEvent().Seq == 0 {
		t.Fatalf("a message the old client handed over after the logout: %v", frame)
	}
	acked(t, jawd, frame.GetEvent().Seq)
	go e.handleFrom(fresh, &events.Connected{})
	if frame = recv(t, jawd); frame.GetEvent().GetConnection().GetState() != enginepb.Connection_CONNECTED {
		t.Fatalf("the fresh client's Connected: %v", frame)
	}
}

func TestALogoutLeavesNoDeviceInTheSessionWhetherOrNotWhatsmeowHasDeletedItYet(t *testing.T) {
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	old := e.cli()
	go e.handleFrom(old, &events.LoggedOut{Reason: events.ConnectFailureMainDeviceGone})
	frame := recv(t, jawd)
	if reason := frame.GetEvent().GetConnection().GetReason(); reason != "primary device was logged out" {
		t.Fatalf("a logout's reason: %q", reason)
	}
	renewed(t, e, old)
	if n := devicesIn(t, e); n != 0 {
		t.Fatalf("%d devices left in session.db, which the next start would read as the account", n)
	}
}

// stored is one event of each kind jawd must store, as whatsmeow hands them over, with the kind of
// event jawd gets for it.
func stored() []struct {
	evt  any
	kind string
} {
	ana := types.NewJID("351912345678", types.DefaultUserServer)
	group := types.NewJID("120363000000000001", types.GroupServer)
	return []struct {
		evt  any
		kind string
	}{
		{&events.Message{
			Info:    types.MessageInfo{MessageSource: types.MessageSource{Chat: ana, Sender: ana}, ID: "3EB0LATE"},
			Message: &waE2E.Message{Conversation: proto.String("still kept")},
		}, "message"},
		{&events.Receipt{
			MessageSource: types.MessageSource{Chat: ana, Sender: ana},
			MessageIDs:    []types.MessageID{"3EB0MINE"},
			Timestamp:     time.UnixMilli(1_792_000_000_000),
			Type:          types.ReceiptTypeRead,
		}, "receipt"},
		{&events.AppState{
			Index:           []string{"pin_v1", ana.String()},
			SyncActionValue: &waSyncAction.SyncActionValue{PinAction: &waSyncAction.PinAction{Pinned: proto.Bool(true)}},
		}, "app state"},
		{&events.PushName{JID: ana, NewPushName: "Ana"}, "push name"},
		{&events.JoinedGroup{GroupInfo: types.GroupInfo{JID: group, GroupName: types.GroupName{Name: "Lisbon Climbing"}}}, "group"},
		{&events.GroupInfo{JID: group, Name: &types.GroupName{Name: "Lisbon Climbing"}}, "group change"},
		{&events.Picture{JID: ana, PictureID: "123"}, "picture"},
		{&events.MediaRetry{ChatID: ana, MessageID: "3EB0GONE", Timestamp: time.UnixMilli(1_792_000_000_000)}, "media retried"},
	}
}

func kindOf(event *enginepb.Event) string {
	switch event.GetKind().(type) {
	case *enginepb.Event_Message:
		return "message"
	case *enginepb.Event_Receipt:
		return "receipt"
	case *enginepb.Event_AppState:
		return "app state"
	case *enginepb.Event_PushName:
		return "push name"
	case *enginepb.Event_Group:
		return "group"
	case *enginepb.Event_GroupChange:
		return "group change"
	case *enginepb.Event_Picture:
		return "picture"
	case *enginepb.Event_MediaRetried:
		return "media retried"
	}
	return fmt.Sprintf("%T", event.GetKind())
}

func TestEveryKindJawdMustStoreGoesOnFromAClientReplacedAfterALogoutAndWhatItSaysOfItsConnectionDoesnt(t *testing.T) {
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	old := e.cli()
	loggedOutByThePhone(t, e, jawd, old)
	for _, sample := range stored() {
		go e.handleFrom(old, sample.evt)
		frame := recv(t, jawd)
		if event := frame.GetEvent(); kindOf(event) != sample.kind || event.Seq == 0 {
			t.Fatalf("a %s from the replaced client reached jawd as %v", sample.kind, frame)
		}
		acked(t, jawd, frame.GetEvent().Seq)
	}
	for _, evt := range []any{
		&events.Connected{}, &events.Disconnected{}, &events.KeepAliveTimeout{}, &events.KeepAliveRestored{}, &events.StreamReplaced{},
		&events.ClientOutdated{}, &events.ConnectFailure{}, &events.TemporaryBan{}, &events.PairSuccess{},
		&events.OfflineSyncPreview{}, &events.OfflineSyncCompleted{}, &events.AppStateSyncComplete{},
	} {
		if !returnsAlone(func() { e.handleFrom(old, evt) }) {
			t.Fatalf("the replaced client's %T went to jawd", evt)
		}
	}
}

func TestALateLogoutFromAClientReplacedTwiceLeavesTheDeviceLinkedSince(t *testing.T) {
	e, jawd, closeLink := linkedEngine(t)
	defer closeLink()
	first := e.cli()
	second := loggedOutByThePhone(t, e, jawd, first)
	paired(t, second.Store, 13)
	third := loggedOutByThePhone(t, e, jawd, second)
	paired(t, third.Store, 14)
	if !returnsAlone(func() { e.handleFrom(first, &events.LoggedOut{Reason: events.ConnectFailureLoggedOut}) }) {
		t.Fatal("a late logout from the first client went to jawd")
	}
	if n := devicesIn(t, e); n != 1 || e.cli() != third {
		t.Fatalf("after the late logout: %d devices in session.db, the client running now replaced: %v", n, e.cli() != third)
	}
}
