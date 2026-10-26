package wa

import (
	"context"
	"errors"
	"fmt"
	"net"
	"testing"

	"go.mau.fi/whatsmeow"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

func TestSendingCommandsWithoutASessionSaySoAndAreNotToBeTriedAsTheyAre(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	for n, cmd := range []*enginepb.Command{
		{Kind: &enginepb.Command_Send{Send: &enginepb.Send{Chat: "351910000002@s.whatsapp.net", Id: "3EB0AA"}}},
		{Kind: &enginepb.Command_MarkRead{MarkRead: &enginepb.MarkRead{Chat: "351910000002@s.whatsapp.net", Ids: []string{"m1"}}}},
		{Kind: &enginepb.Command_ChatPresence{ChatPresence: &enginepb.ChatPresence{Chat: "351910000002@s.whatsapp.net", State: enginepb.ChatPresence_COMPOSING}}},
		{Kind: &enginepb.Command_Presence{Presence: &enginepb.Presence{Available: true}}},
		{Kind: &enginepb.Command_OnWhatsapp{OnWhatsapp: &enginepb.OnWhatsApp{Phones: []string{"351910000002"}}}},
		{Kind: &enginepb.Command_AppStatePatch{AppStatePatch: &enginepb.AppStatePatch{
			Name: "regular_high", Index: []string{"deleteMessageForMe", "351910000002@s.whatsapp.net", "3EB0AA", "1", "0"}, Version: 3,
		}}},
		{Kind: &enginepb.Command_PollVote{PollVote: &enginepb.PollVote{
			Chat: "120363000000000002@g.us", Id: "3EB0BB", PollId: "3EB0AA", PollSender: "351910000002@s.whatsapp.net",
		}}},
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

func TestOnlyTheSendingCommandsAreSendings(t *testing.T) {
	e := newEngine(context.Background(), nil, NewLogger("test", "error"))
	if reply := e.sending(&enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}}); reply != nil {
		t.Fatalf("a ping was taken for a sending: %v", reply)
	}
}

func TestAChatToSendToIsNeverOneDevice(t *testing.T) {
	if _, err := chatJID("351910000002:3@s.whatsapp.net"); err == nil {
		t.Fatal("a device's JID was taken for a chat")
	}
	if _, err := chatJID("120363000000000002@g.us"); err != nil {
		t.Fatalf("a group's JID: %v", err)
	}
}

type timeout struct{}

func (timeout) Error() string   { return "i/o timeout" }
func (timeout) Timeout() bool   { return true }
func (timeout) Temporary() bool { return true }

func TestASendThatMightWorkLaterIsTriedAgainAndOneThatCantIsNot(t *testing.T) {
	var netErr net.Error = timeout{}
	for _, err := range []error{
		whatsmeow.ErrNotConnected,
		fmt.Errorf("%w 500", whatsmeow.ErrServerReturnedError),
		fmt.Errorf("%w 429", whatsmeow.ErrServerReturnedError),
		whatsmeow.ErrMessageTimedOut,
		fmt.Errorf("failed to get devices: %w", netErr),
		fmt.Errorf("failed to get devices: %w", &whatsmeow.IQError{Code: 503}),
		context.DeadlineExceeded,
		errors.New("failed to get user info for 351910000002@s.whatsapp.net to fill LID cache"),
	} {
		if !retryable(err) {
			t.Errorf("%v isn't tried again", err)
		}
	}
	for _, err := range []error{
		whatsmeow.ErrRecipientADJID,
		fmt.Errorf("%w x.example", whatsmeow.ErrUnknownServer),
		whatsmeow.ErrNoPermissionInGroup,
		fmt.Errorf("failed to get group members: %w", whatsmeow.ErrNotInGroup),
		fmt.Errorf("%w 403", whatsmeow.ErrServerReturnedError),
		fmt.Errorf("%w 413", whatsmeow.ErrServerReturnedError),
		fmt.Errorf("failed to get devices: %w", &whatsmeow.IQError{Code: 404}),
		errors.New("no LID found for 351910000002@s.whatsapp.net from server"),
		errors.New("failed to marshal message: proto: invalid"),
	} {
		if retryable(err) {
			t.Errorf("%v is tried again", err)
		}
	}
}

func TestAVoteThatCantBeRightIsRefusedForGoodBeforeItGoes(t *testing.T) {
	e := newEngine(context.Background(), nil, NewLogger("test", "error"))
	for name, vote := range map[string]*enginepb.PollVote{
		"a device for a chat": {Chat: "351910000002:3@s.whatsapp.net", Id: "3EB0BB", PollId: "3EB0AA", PollSender: "351910000002@s.whatsapp.net"},
		"no ID of its own":    {Chat: "120363000000000002@g.us", PollId: "3EB0AA", PollSender: "351910000002@s.whatsapp.net"},
		"no poll":             {Chat: "120363000000000002@g.us", Id: "3EB0BB", PollSender: "351910000002@s.whatsapp.net"},
		"nobody sent it":      {Chat: "120363000000000002@g.us", Id: "3EB0BB", PollId: "3EB0AA", PollSender: "@s.whatsapp.net"},
	} {
		failure := e.pollVote(vote).GetFailure()
		if failure == nil || failure.Retry {
			t.Errorf("%s: %v", name, failure)
		}
	}
}

func TestAnAppStatePatchThatCantBeRightIsRefusedForGoodBeforeItGoes(t *testing.T) {
	e := newEngine(context.Background(), nil, NewLogger("test", "error"))
	index := []string{"deleteMessageForMe", "351910000002@s.whatsapp.net", "3EB0AA", "1", "0"}
	for name, patch := range map[string]*enginepb.AppStatePatch{
		"no such collection": {Name: "regular_medium", Index: index, Version: 3},
		"no index":           {Name: "regular_high", Version: 3},
		"a value that isn't": {Name: "regular_high", Index: index, Version: 3, Value: []byte{0xff, 0xff}},
	} {
		failure := e.appStatePatch(patch).GetFailure()
		if failure == nil || failure.Retry {
			t.Errorf("%s: %v", name, failure)
		}
	}
}
