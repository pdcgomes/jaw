package wa

import (
	"context"
	"errors"
	"fmt"
	"net"
	"strconv"
	"strings"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/types"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// What jawd sends through the engine: messages with the IDs jawd made for them, read receipts,
// typing and presence, and which numbers are on WhatsApp. jawd keeps what's waiting and decides
// when; the engine only carries each out and says whether trying again might work.

// sending carries out a sending command, or returns nil for a command that isn't one.
func (e *Engine) sending(cmd *enginepb.Command) *enginepb.Reply {
	switch c := cmd.Kind.(type) {
	case *enginepb.Command_Send:
		return e.sendMessage(c.Send)
	case *enginepb.Command_MarkRead:
		return e.markRead(c.MarkRead)
	case *enginepb.Command_ChatPresence:
		return e.chatPresence(c.ChatPresence)
	case *enginepb.Command_Presence:
		return e.presence(c.Presence)
	case *enginepb.Command_OnWhatsapp:
		return e.onWhatsApp(c.OnWhatsapp)
	case *enginepb.Command_AppStatePatch:
		return e.appStatePatch(c.AppStatePatch)
	}
	return nil
}

// chatJID parses a chat jawd names, which is never a device's.
func chatJID(s string) (types.JID, error) {
	j, err := types.ParseJID(s)
	if err != nil {
		return j, fmt.Errorf("%q isn't a JID: %w", s, err)
	}
	if j.Device > 0 {
		return j, fmt.Errorf("%q is a device, not a chat", s)
	}
	return j, nil
}

func (e *Engine) sendMessage(c *enginepb.Send) *enginepb.Reply {
	to, err := chatJID(c.Chat)
	if err != nil {
		return failure(err.Error(), false)
	}
	if c.Id == "" {
		return failure("a message to send needs the ID jawd made for it", false)
	}
	message := &waE2E.Message{}
	if err := proto.Unmarshal(c.Raw, message); err != nil {
		return failure(fmt.Sprintf("that isn't a message to send: %v", err), false)
	}
	resp, err := e.client.SendMessage(e.ctx, to, message, whatsmeow.SendRequestExtra{ID: c.Id})
	if err != nil {
		return failure(fmt.Sprintf("can't send %s: %v", c.Id, err), retryable(err))
	}
	sent := &enginepb.Sent{Id: c.Id, Chat: jid(resp.Chat)}
	if !resp.Timestamp.IsZero() {
		sent.AtMs = resp.Timestamp.UnixMilli()
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_Sent{Sent: sent}}
}

// retryable says whether a send that failed with err might work later: a connection that's down
// or slow, or WhatsApp having a moment. A refusal won't: a number that isn't on WhatsApp, a group
// you're not in or can't write in, a chat that can't be sent to, a message the server won't take
// (too large, blocked and the like, which it answers with a 4xx code), or one that can't be encoded.
func retryable(err error) bool {
	switch {
	case errors.Is(err, whatsmeow.ErrRecipientADJID),
		errors.Is(err, whatsmeow.ErrUnknownServer),
		errors.Is(err, whatsmeow.ErrNoPermissionInGroup),
		errors.Is(err, whatsmeow.ErrInvalidInlineBotID),
		errors.Is(err, whatsmeow.ErrGroupNotFound),
		errors.Is(err, whatsmeow.ErrNotInGroup),
		errors.Is(err, whatsmeow.ErrBroadcastListUnsupported):
		return false
	case errors.Is(err, whatsmeow.ErrNotConnected),
		errors.Is(err, whatsmeow.ErrNotLoggedIn),
		errors.Is(err, whatsmeow.ErrMessageTimedOut),
		errors.Is(err, whatsmeow.ErrIQTimedOut),
		errors.Is(err, context.DeadlineExceeded):
		return true
	}
	var iq *whatsmeow.IQError
	if errors.As(err, &iq) {
		return transient(iq.Code)
	}
	if errors.Is(err, whatsmeow.ErrServerReturnedError) {
		return transient(serverCode(err))
	}
	var netErr net.Error
	if errors.As(err, &netErr) {
		return true
	}
	// whatsmeow says a number has no account only in words.
	if strings.Contains(err.Error(), "no LID found") || strings.Contains(err.Error(), "failed to marshal") {
		return false
	}
	// The rest it wraps without a type of its own (devices, sessions, keys) are mostly the network
	// or the server, so they're tried again while the connection lasts.
	return true
}

// transient says whether a server's error code is worth trying again after: none, a timeout,
// too many requests, or the server's own trouble.
func transient(code int) bool {
	return code == 0 || code == 408 || code == 429 || code >= 500
}

// serverCode is the code in whatsmeow's "server returned error N", or 0.
func serverCode(err error) int {
	fields := strings.Fields(err.Error())
	if len(fields) == 0 {
		return 0
	}
	code, _ := strconv.Atoi(fields[len(fields)-1])
	return code
}

func (e *Engine) markRead(c *enginepb.MarkRead) *enginepb.Reply {
	chat, err := chatJID(c.Chat)
	if err != nil {
		return failure(err.Error(), false)
	}
	var sender types.JID
	if c.Sender != "" {
		if sender, err = types.ParseJID(c.Sender); err != nil {
			return failure(fmt.Sprintf("%q isn't a JID: %v", c.Sender, err), false)
		}
	}
	if len(c.Ids) == 0 {
		return done()
	}
	ids := make([]types.MessageID, len(c.Ids))
	for i, id := range c.Ids {
		ids[i] = types.MessageID(id)
	}
	if err := e.client.MarkRead(e.ctx, ids, time.UnixMilli(c.AtMs), chat, sender); err != nil {
		return failure(fmt.Sprintf("can't mark %d messages read in %s: %v", len(ids), c.Chat, err), true)
	}
	return done()
}

func (e *Engine) chatPresence(c *enginepb.ChatPresence) *enginepb.Reply {
	chat, err := chatJID(c.Chat)
	if err != nil {
		return failure(err.Error(), false)
	}
	state, media := types.ChatPresencePaused, types.ChatPresenceMediaText
	switch c.State {
	case enginepb.ChatPresence_COMPOSING:
		state = types.ChatPresenceComposing
	case enginepb.ChatPresence_RECORDING:
		state, media = types.ChatPresenceComposing, types.ChatPresenceMediaAudio
	case enginepb.ChatPresence_PAUSED:
	default:
		return failure(fmt.Sprintf("there's no chat presence %v", c.State), false)
	}
	if err := e.client.SendChatPresence(e.ctx, chat, state, media); err != nil {
		return failure(fmt.Sprintf("can't say you're typing in %s: %v", c.Chat, err), true)
	}
	return done()
}

func (e *Engine) presence(c *enginepb.Presence) *enginepb.Reply {
	state := types.PresenceUnavailable
	if c.Available {
		state = types.PresenceAvailable
	}
	if err := e.client.SendPresence(e.ctx, state); err != nil {
		// Without a push name WhatsApp won't take presence; it comes with the first app-state sync.
		return failure(fmt.Sprintf("can't say you're %s: %v", state, err), !errors.Is(err, whatsmeow.ErrNoPushName))
	}
	return done()
}

func (e *Engine) onWhatsApp(c *enginepb.OnWhatsApp) *enginepb.Reply {
	if len(c.Phones) == 0 {
		return &enginepb.Reply{Kind: &enginepb.Reply_OnWhatsappList{OnWhatsappList: &enginepb.OnWhatsAppList{}}}
	}
	answers, err := e.client.IsOnWhatsApp(e.ctx, c.Phones)
	if err != nil {
		return failure(fmt.Sprintf("can't ask WhatsApp who's on it: %v", err), true)
	}
	list := &enginepb.OnWhatsAppList{}
	for _, a := range answers {
		user := &enginepb.OnWhatsAppUser{Query: a.Query, IsIn: a.IsIn, Jid: jid(a.JID), Phone: jid(a.PhoneNumber)}
		if a.VerifiedName != nil && a.VerifiedName.Details != nil {
			user.VerifiedName = a.VerifiedName.Details.GetVerifiedName()
		}
		list.Users = append(list.Users, user)
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_OnWhatsappList{OnWhatsappList: list}}
}
