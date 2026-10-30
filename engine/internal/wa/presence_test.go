package wa

import (
	"testing"
	"time"

	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

func TestPresenceIsSaidOfThePersonNotTheirDeviceWithWhenTheyWereLastSeenOnlyIfTheyShowIt(t *testing.T) {
	from := types.NewJID("351910000002", types.DefaultUserServer)
	from.Device = 3
	seen := time.UnixMilli(1792000000123)
	away := presenceUpdate(&events.Presence{From: from, Unavailable: true, LastSeen: seen})
	if away.Jid != "351910000002@s.whatsapp.net" || !away.Unavailable || away.LastSeenMs != 1792000000123 {
		t.Fatalf("away: %v", away)
	}
	if hidden := presenceUpdate(&events.Presence{From: from, Unavailable: true}); hidden.LastSeenMs != 0 {
		t.Fatalf("someone who hides when they were last seen: %v", hidden)
	}
}

func TestTypingAndRecordingAreToldApartAndStoppingIsPaused(t *testing.T) {
	chat := types.NewJID("351910000002", types.DefaultUserServer)
	source := types.MessageSource{Chat: chat, Sender: chat}
	for _, c := range []struct {
		state types.ChatPresence
		media types.ChatPresenceMedia
		want  enginepb.ChatPresence_State
	}{
		{types.ChatPresenceComposing, types.ChatPresenceMediaText, enginepb.ChatPresence_COMPOSING},
		{types.ChatPresenceComposing, types.ChatPresenceMediaAudio, enginepb.ChatPresence_RECORDING},
		{types.ChatPresencePaused, types.ChatPresenceMediaText, enginepb.ChatPresence_PAUSED},
	} {
		got := typing(&events.ChatPresence{MessageSource: source, State: c.state, Media: c.media})
		if got.State != c.want || got.Chat != chat.String() {
			t.Fatalf("%v %v gave %v", c.state, c.media, got)
		}
	}
}

func TestTypingSaysWhoByEitherJIDWithoutTheirDevice(t *testing.T) {
	group := types.NewJID("120363012345678901", types.GroupServer)
	lid := types.NewJID("100000000002", types.HiddenUserServer)
	lid.Device = 7
	phone := types.NewJID("351910000002", types.DefaultUserServer)
	phone.Device = 7
	source := types.MessageSource{Chat: group, Sender: lid, SenderAlt: phone, IsGroup: true}
	got := typing(&events.ChatPresence{MessageSource: source, State: types.ChatPresenceComposing})
	if got.Sender != "100000000002@lid" || got.SenderAlt != "351910000002@s.whatsapp.net" {
		t.Fatalf("typing by her LID with her number beside it gave %v", got)
	}
	alone := typing(&events.ChatPresence{MessageSource: types.MessageSource{Chat: group, Sender: lid, IsGroup: true}})
	if alone.SenderAlt != "" {
		t.Fatalf("typing with no other JID gave %q beside it", alone.SenderAlt)
	}
}

func TestFollowingPresenceAndAskingAboutPeopleWithoutASessionSaySo(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	for n, cmd := range []*enginepb.Command{
		{Kind: &enginepb.Command_SubscribePresence{SubscribePresence: &enginepb.SubscribePresence{Jid: "351910000002@s.whatsapp.net"}}},
		{Kind: &enginepb.Command_UserInfo{UserInfo: &enginepb.UserInfo{Jids: []string{"351910000002@s.whatsapp.net"}}}},
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
