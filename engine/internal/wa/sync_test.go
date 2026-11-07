package wa

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// handled runs evt through the engine's handler and returns the event jawd got, acking it if it
// carries a seq, and whether the handler waited for that ack.
func handled(t *testing.T, e *Engine, jawd *link.Link, evt any) (*enginepb.Event, bool) {
	t.Helper()
	returned := make(chan struct{})
	go func() {
		e.handle(e.cli(), evt)
		close(returned)
	}()
	frame, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	event := frame.GetEvent()
	if event == nil {
		t.Fatalf("the handler sent %v", frame)
	}
	waited := false
	if event.Seq != 0 {
		select {
		case <-returned:
			t.Fatalf("the handler returned before %T was acked", evt)
		case <-time.After(20 * time.Millisecond):
			waited = true
		}
		if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Ack{Ack: &enginepb.Ack{Seq: event.Seq}}}); err != nil {
			t.Fatal(err)
		}
	}
	select {
	case <-returned:
	case <-time.After(2 * time.Second):
		t.Fatalf("the handler didn't return after %T", evt)
	}
	return event, waited
}

func mustJID(t *testing.T, s string) types.JID {
	t.Helper()
	j, err := types.ParseJID(s)
	if err != nil {
		t.Fatal(err)
	}
	return j
}

func TestAnAppStateMutationIsKeptWithItsIndexAndRawValue(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	value := &waSyncAction.SyncActionValue{
		Timestamp: proto.Int64(1_792_000_000_000),
		PinAction: &waSyncAction.PinAction{Pinned: proto.Bool(true)},
	}
	index := []string{"pin_v1", "351910000001@s.whatsapp.net"}
	event, waited := handled(t, e, jawd, &events.AppState{Index: index, SyncActionValue: value})
	if !waited {
		t.Fatal("an app-state mutation wasn't waited for")
	}
	state := event.GetAppState()
	if state == nil || len(state.Index) != 2 || state.Index[0] != "pin_v1" {
		t.Fatalf("the app state came as %v", event)
	}
	got := &waSyncAction.SyncActionValue{}
	if err := proto.Unmarshal(state.Value, got); err != nil || !proto.Equal(got, value) {
		t.Fatalf("the value came back as %v (%v)", got, err)
	}
	event, waited = handled(t, e, jawd, &events.AppStateSyncComplete{Name: "regular", Version: 7})
	if waited || event.GetAppStateSynced().GetName() != "regular" || event.GetAppStateSynced().GetVersion() != 7 {
		t.Fatalf("a finished sync came as %v, waited %v", event, waited)
	}
}

func TestGroupsAndTheirChangesArriveAsWhatsmeowDescribesThem(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	joined := &events.JoinedGroup{GroupInfo: types.GroupInfo{
		JID:               mustJID(t, "120363000000000002@g.us"),
		GroupName:         types.GroupName{Name: "Climbing"},
		GroupEphemeral:    types.GroupEphemeral{IsEphemeral: true, DisappearingTimer: 86400},
		GroupLinkedParent: types.GroupLinkedParent{LinkedParentJID: mustJID(t, "120363000000000001@g.us")},
		GroupIsDefaultSub: types.GroupIsDefaultSub{IsDefaultSubGroup: true},
		Participants: []types.GroupParticipant{{
			JID:         mustJID(t, "100000000001@lid"),
			PhoneNumber: mustJID(t, "351910000002@s.whatsapp.net"),
			LID:         mustJID(t, "100000000001@lid"),
			IsAdmin:     true,
		}},
	}}
	event, waited := handled(t, e, jawd, joined)
	group := event.GetGroup()
	if !waited || group.GetName() != "Climbing" || group.GetEphemeralSeconds() != 86400 || !group.GetIsDefaultSub() ||
		group.GetLinkedParent() != "120363000000000001@g.us" {
		t.Fatalf("a joined group came as %v, waited %v", event, waited)
	}
	if p := group.GetParticipants(); len(p) != 1 || p[0].GetPhone() != "351910000002@s.whatsapp.net" || !p[0].GetAdmin() {
		t.Fatalf("its members came as %v", p)
	}

	sender := mustJID(t, "100000000001@lid")
	change := &events.GroupInfo{
		JID:       mustJID(t, "120363000000000002@g.us"),
		Sender:    &sender,
		Timestamp: time.UnixMilli(1_792_000_000_000),
		Name:      &types.GroupName{Name: "Climbing on Saturdays"},
		Ephemeral: &types.GroupEphemeral{IsEphemeral: false},
		Join:      []types.JID{mustJID(t, "351910000003@s.whatsapp.net")},
		Demote:    []types.JID{sender},
	}
	event, waited = handled(t, e, jawd, change)
	c := event.GetGroupChange()
	if !waited || c.GetSender() != "100000000001@lid" || c.GetTimestampMs() != 1_792_000_000_000 {
		t.Fatalf("a group change came as %v, waited %v", event, waited)
	}
	if c.Name == nil || *c.Name != "Climbing on Saturdays" || c.EphemeralSeconds == nil || *c.EphemeralSeconds != 0 {
		t.Fatalf("its name and timer came as %v and %v", c.Name, c.EphemeralSeconds)
	}
	if c.Topic != nil || c.Announce != nil || len(c.Join) != 1 || len(c.Demote) != 1 || len(c.Leave) != 0 {
		t.Fatalf("what didn't change came as set: %v", c)
	}
}

func TestPushNamesAndPicturesAreKept(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	event, waited := handled(t, e, jawd, &events.PushName{
		JID:         mustJID(t, "100000000001@lid"),
		JIDAlt:      mustJID(t, "351910000002@s.whatsapp.net"),
		NewPushName: "Ana",
	})
	if name := event.GetPushName(); !waited || name.GetJidAlt() != "351910000002@s.whatsapp.net" || name.GetPushName() != "Ana" {
		t.Fatalf("a push name came as %v, waited %v", event, waited)
	}
	event, waited = handled(t, e, jawd, &events.Picture{JID: mustJID(t, "120363000000000002@g.us"), PictureID: "123"})
	if p := event.GetPicture(); !waited || p.GetPictureId() != "123" || p.GetRemoved() {
		t.Fatalf("a picture came as %v, waited %v", event, waited)
	}
}

func TestCommandsThatNeedALinkedAccountSaySo(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	for n, cmd := range []*enginepb.Command{
		{Kind: &enginepb.Command_Groups{Groups: &enginepb.Groups{}}},
		{Kind: &enginepb.Command_Avatar{Avatar: &enginepb.Avatar{Jid: "120363000000000002@g.us"}}},
		{Kind: &enginepb.Command_HistoryRequest{HistoryRequest: &enginepb.HistoryRequest{Chat: "351910000002@s.whatsapp.net"}}},
	} {
		command(t, jawd, uint64(n+1), cmd)
		reply, err := jawd.Recv()
		if err != nil {
			t.Fatal(err)
		}
		if reply.GetReply().GetFailure() == nil {
			t.Fatalf("%T without a session gave %v", cmd.Kind, reply)
		}
	}
}

func TestAPictureIsWrittenWholeOrNotAtAll(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/gone" {
			http.NotFound(w, r)
			return
		}
		_, _ = w.Write([]byte("a jpeg"))
	}))
	defer server.Close()
	folder := t.TempDir()
	path := filepath.Join(folder, "avatars", "a.jpg")
	n, err := download(context.Background(), server.URL+"/a.jpg", path)
	if err != nil || n != 6 {
		t.Fatalf("download gave %d, %v", n, err)
	}
	if data, _ := os.ReadFile(path); string(data) != "a jpeg" {
		t.Fatalf("the file holds %q", data)
	}
	gone := filepath.Join(folder, "avatars", "b.jpg")
	if _, err := download(context.Background(), server.URL+"/gone", gone); err == nil {
		t.Fatal("a missing picture downloaded")
	}
	if entries, _ := os.ReadDir(filepath.Dir(gone)); len(entries) != 1 {
		t.Fatalf("a failed download left %d files", len(entries))
	}
}

func TestAReceiptIsKeptBeforeWhatsmeowAcksIt(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	receipt := &events.Receipt{
		MessageSource: types.MessageSource{Chat: mustJID(t, "351910000002@s.whatsapp.net"), Sender: mustJID(t, "351910000002@s.whatsapp.net")},
		MessageIDs:    []types.MessageID{"3EB0AA"},
		Type:          types.ReceiptTypeRead,
	}
	event, waited := handled(t, e, jawd, receipt)
	if !waited || event.GetReceipt().GetType() != "read" || len(event.GetReceipt().GetIds()) != 1 {
		t.Fatalf("a receipt came as %v, waited %v", event, waited)
	}
}

func TestAHistoryBlobIsWrittenWholeOrNotAtAll(t *testing.T) {
	folder := t.TempDir()
	path := filepath.Join(folder, "spool", "3EB0AA.pb")
	if err := writeWhole(path, []byte("a blob")); err != nil {
		t.Fatal(err)
	}
	if data, _ := os.ReadFile(path); string(data) != "a blob" {
		t.Fatalf("the blob holds %q", data)
	}
	if _, err := os.Stat(path + ".part"); !os.IsNotExist(err) {
		t.Fatalf("the .part file is still there: %v", err)
	}
}

func TestAHistoryBlobWhatsAppNoLongerHasIsNotAskedForAgain(t *testing.T) {
	for _, err := range []error{
		whatsmeow.ErrMediaDownloadFailedWith403,
		fmt.Errorf("downloading: %w", whatsmeow.ErrMediaDownloadFailedWith404),
		fmt.Errorf("downloading: %w", whatsmeow.ErrMediaDownloadFailedWith410),
	} {
		if !gone(err) {
			t.Errorf("%v should mean the blob is gone", err)
		}
	}
	for _, err := range []error{
		errors.New("connection reset"),
		whatsmeow.DownloadHTTPError{Response: &http.Response{StatusCode: http.StatusServiceUnavailable}},
	} {
		if gone(err) {
			t.Errorf("%v may pass, so the blob should be asked for again", err)
		}
	}
}
