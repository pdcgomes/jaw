package wa

import (
	"errors"
	"fmt"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// Others' presence: WhatsApp sends it for the people jawd subscribes to, while the account is
// online, and typing for the chats it's in. jawd decides whom to follow and keeps to WhatsApp's
// limits; the engine only asks and hands on what comes. Neither is kept by jawd before an ack, so
// both go fire and forget.

// The most people one UserInfo asks about.
const userInfoMost = 50

func presenceUpdate(v *events.Presence) *enginepb.PresenceUpdate {
	u := &enginepb.PresenceUpdate{Jid: v.From.ToNonAD().String(), Unavailable: v.Unavailable}
	if !v.LastSeen.IsZero() {
		u.LastSeenMs = v.LastSeen.UnixMilli()
	}
	return u
}

func typing(v *events.ChatPresence) *enginepb.Typing {
	state := enginepb.ChatPresence_PAUSED
	if v.State == types.ChatPresenceComposing {
		state = enginepb.ChatPresence_COMPOSING
		if v.Media == types.ChatPresenceMediaAudio {
			state = enginepb.ChatPresence_RECORDING
		}
	}
	return &enginepb.Typing{Chat: v.Chat.String(), Sender: v.Sender.ToNonAD().String(), State: state}
}

func (e *Engine) subscribePresence(c *enginepb.SubscribePresence) *enginepb.Reply {
	who, err := chatJID(c.Jid)
	if err != nil {
		return failure(err.Error(), false)
	}
	if err := e.cli().SubscribePresence(e.ctx, who); err != nil {
		// Without a push name WhatsApp won't take it; it comes with the first app-state sync.
		return failure(fmt.Sprintf("can't follow %s's presence: %v", c.Jid, err), !errors.Is(err, whatsmeow.ErrNoPushName))
	}
	return done()
}

func (e *Engine) userInfo(c *enginepb.UserInfo) *enginepb.Reply {
	if len(c.Jids) > userInfoMost {
		return failure(fmt.Sprintf("WhatsApp is asked about at most %d people at once, not %d", userInfoMost, len(c.Jids)), false)
	}
	list := &enginepb.UserInfoList{}
	if len(c.Jids) == 0 {
		return &enginepb.Reply{Kind: &enginepb.Reply_UserInfoList{UserInfoList: list}}
	}
	jids := make([]types.JID, 0, len(c.Jids))
	for _, s := range c.Jids {
		j, err := chatJID(s)
		if err != nil {
			return failure(err.Error(), false)
		}
		jids = append(jids, j)
	}
	info, err := e.cli().GetUserInfo(e.ctx, jids)
	if err != nil {
		return failure(fmt.Sprintf("can't ask WhatsApp about %d people: %v", len(jids), err), true)
	}
	for j, i := range info {
		user := &enginepb.User{Jid: j.String(), About: i.Status, PictureId: i.PictureID, Lid: jid(i.LID)}
		if i.VerifiedName != nil && i.VerifiedName.Details != nil {
			user.VerifiedName = i.VerifiedName.Details.GetVerifiedName()
		}
		list.Users = append(list.Users, user)
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_UserInfoList{UserInfoList: list}}
}
