package wa

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// What jawd keeps in sync beyond messages: app state, push names, groups, pictures, contacts and
// the LID map, each handed over as whatsmeow has it, for jawd to understand.

func appState(v *events.AppState) (*enginepb.AppState, error) {
	value, err := proto.Marshal(v.SyncActionValue)
	if err != nil {
		return nil, err
	}
	return &enginepb.AppState{Index: v.Index, Value: value}, nil
}

func pushName(v *events.PushName) *enginepb.PushName {
	return &enginepb.PushName{Jid: jid(v.JID), JidAlt: jid(v.JIDAlt), PushName: v.NewPushName}
}

func groupInfo(g *types.GroupInfo) *enginepb.GroupInfo {
	info := &enginepb.GroupInfo{
		Jid:            g.JID.String(),
		Name:           g.Name,
		Topic:          g.Topic,
		Owner:          jid(g.OwnerJID),
		Announce:       g.IsAnnounce,
		Locked:         g.IsLocked,
		IsParent:       g.IsParent,
		LinkedParent:   jid(g.LinkedParentJID),
		IsDefaultSub:   g.IsDefaultSubGroup,
		AddressingMode: string(g.AddressingMode),
	}
	if !g.GroupCreated.IsZero() {
		info.CreatedMs = g.GroupCreated.UnixMilli()
	}
	if g.IsEphemeral {
		info.EphemeralSeconds = g.DisappearingTimer
	}
	for _, p := range g.Participants {
		info.Participants = append(info.Participants, &enginepb.Participant{
			Jid:         jid(p.JID),
			Phone:       jid(p.PhoneNumber),
			Lid:         jid(p.LID),
			Admin:       p.IsAdmin,
			SuperAdmin:  p.IsSuperAdmin,
			DisplayName: p.DisplayName,
		})
	}
	return info
}

func jids(list []types.JID) []string {
	out := make([]string, 0, len(list))
	for _, j := range list {
		out = append(out, j.String())
	}
	return out
}

func groupChange(v *events.GroupInfo) *enginepb.GroupChange {
	c := &enginepb.GroupChange{
		Jid:     v.JID.String(),
		Join:    jids(v.Join),
		Leave:   jids(v.Leave),
		Promote: jids(v.Promote),
		Demote:  jids(v.Demote),
		Deleted: v.Delete != nil,
	}
	if v.Sender != nil {
		c.Sender = jid(*v.Sender)
	}
	if v.SenderPN != nil {
		c.SenderPn = jid(*v.SenderPN)
	}
	if !v.Timestamp.IsZero() {
		c.TimestampMs = v.Timestamp.UnixMilli()
	}
	if v.Name != nil {
		c.Name = &v.Name.Name
	}
	if v.Topic != nil {
		c.Topic = &v.Topic.Topic
	}
	if v.Announce != nil {
		c.Announce = &v.Announce.IsAnnounce
	}
	if v.Locked != nil {
		c.Locked = &v.Locked.IsLocked
	}
	if v.Ephemeral != nil {
		var seconds uint32
		if v.Ephemeral.IsEphemeral {
			seconds = v.Ephemeral.DisappearingTimer
		}
		c.EphemeralSeconds = &seconds
	}
	if v.Link != nil {
		c.Linked = jid(v.Link.Group.JID)
	}
	if v.Unlink != nil {
		c.Unlinked = jid(v.Unlink.Group.JID)
	}
	return c
}

func picture(v *events.Picture) *enginepb.Picture {
	p := &enginepb.Picture{Jid: v.JID.String(), Author: jid(v.Author), Removed: v.Remove, PictureId: v.PictureID}
	if !v.Timestamp.IsZero() {
		p.TimestampMs = v.Timestamp.UnixMilli()
	}
	return p
}

// groups answers Groups with every group the account is in.
func (e *Engine) groups() *enginepb.Reply {
	groups, err := e.cli().GetJoinedGroups(e.ctx)
	if err != nil {
		return failure(fmt.Sprintf("can't list the groups: %v", err), true)
	}
	list := &enginepb.GroupList{}
	for _, g := range groups {
		list.Groups = append(list.Groups, groupInfo(g))
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_GroupList{GroupList: list}}
}

// contacts answers Contacts from whatsmeow's own store, without asking WhatsApp.
func (e *Engine) contacts() *enginepb.Reply {
	all, err := e.cli().Store.Contacts.GetAllContacts(e.ctx)
	if err != nil {
		return failure(fmt.Sprintf("can't read the contacts whatsmeow keeps: %v", err), true)
	}
	var phones []types.JID
	for j := range all {
		if j.Server == types.DefaultUserServer {
			phones = append(phones, j)
		}
	}
	lids, err := e.cli().Store.LIDs.GetManyLIDsForPNs(e.ctx, phones)
	if err != nil {
		e.log.Warnf("can't read the LIDs of %d contacts: %v", len(phones), err)
	}
	list := &enginepb.ContactList{}
	for j, c := range all {
		contact := &enginepb.Contact{
			Jid:          j.String(),
			FirstName:    c.FirstName,
			FullName:     c.FullName,
			PushName:     c.PushName,
			BusinessName: c.BusinessName,
		}
		if lid, ok := lids[j]; ok {
			contact.Lid = jid(lid)
		}
		list.Contacts = append(list.Contacts, contact)
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_ContactList{ContactList: list}}
}

// lidLookup answers LidLookup from whatsmeow's LID map, leaving out LIDs it has no number for.
func (e *Engine) lidLookup(c *enginepb.LidLookup) *enginepb.Reply {
	pairs := &enginepb.LidPairs{}
	for _, s := range c.Lids {
		lid, err := types.ParseJID(s)
		if err != nil || lid.Server != types.HiddenUserServer {
			continue
		}
		pn, err := e.cli().Store.LIDs.GetPNForLID(e.ctx, lid)
		if err != nil {
			return failure(fmt.Sprintf("can't read whatsmeow's LID map: %v", err), true)
		}
		if !pn.IsEmpty() {
			pairs.Pairs = append(pairs.Pairs, &enginepb.LidPair{Lid: lid.String(), Phone: pn.String()})
		}
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_LidPairs{LidPairs: pairs}}
}

// appStateResync fetches the named collections again from version 0. whatsmeow sends their
// mutations through the event handler, so each is stored by jawd before this answers.
func (e *Engine) appStateResync(c *enginepb.AppStateResync) *enginepb.Reply {
	names := c.Names
	if len(names) == 0 {
		for _, name := range appstate.AllPatchNames {
			names = append(names, string(name))
		}
	}
	for _, name := range names {
		if err := e.cli().FetchAppState(e.ctx, appstate.WAPatchName(name), true, false); err != nil {
			return failure(fmt.Sprintf("can't fetch the app state %s: %v", name, err), true)
		}
	}
	return done()
}

// avatar downloads a picture to the path jawd chose, whole or not at all.
func (e *Engine) avatar(c *enginepb.Avatar) *enginepb.Reply {
	target, err := types.ParseJID(c.Jid)
	if err != nil {
		return failure(fmt.Sprintf("%q isn't a JID: %v", c.Jid, err), false)
	}
	info, err := e.cli().GetProfilePictureInfo(e.ctx, target, &whatsmeow.GetProfilePictureParams{
		ExistingID:  c.ExistingId,
		IsCommunity: c.Community,
	})
	if errors.Is(err, whatsmeow.ErrProfilePictureNotSet) || errors.Is(err, whatsmeow.ErrProfilePictureUnauthorized) {
		return avatarFile(enginepb.AvatarFile_NONE, "", 0)
	}
	if err != nil {
		return failure(fmt.Sprintf("can't ask for %s's picture: %v", c.Jid, err), true)
	}
	if info == nil {
		return avatarFile(enginepb.AvatarFile_UNCHANGED, c.ExistingId, 0)
	}
	size, err := download(e.ctx, info.URL, c.Path)
	if err != nil {
		return failure(fmt.Sprintf("can't download %s's picture: %v", c.Jid, err), true)
	}
	return avatarFile(enginepb.AvatarFile_DOWNLOADED, info.ID, size)
}

func avatarFile(state enginepb.AvatarFile_State, id string, size uint64) *enginepb.Reply {
	return &enginepb.Reply{Kind: &enginepb.Reply_AvatarFile{AvatarFile: &enginepb.AvatarFile{State: state, Id: id, Bytes: size}}}
}

// pictures fetches profile pictures through HTTPS_PROXY when it's set, as whatsmeow does.
var pictures = &http.Client{Timeout: time.Minute, Transport: &http.Transport{Proxy: http.ProxyFromEnvironment}}

// download writes what url answers to path, through a .part file synced and renamed once it's
// whole, so a row never points at a file a power cut left half written.
func download(ctx context.Context, url, path string) (uint64, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return 0, err
	}
	resp, err := pictures.Do(req)
	if err != nil {
		return 0, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return 0, fmt.Errorf("the server answered %s", resp.Status)
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		return 0, err
	}
	part := path + ".part"
	f, err := os.OpenFile(part, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return 0, err
	}
	n, err := io.Copy(f, resp.Body)
	if err == nil {
		err = fullSync(f)
	}
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		_ = os.Remove(part)
		return 0, err
	}
	if err := os.Rename(part, path); err != nil {
		return 0, err
	}
	return uint64(n), syncDir(filepath.Dir(path))
}

// historyRequest asks the phone for messages from before the oldest jawd has in a chat.
func (e *Engine) historyRequest(c *enginepb.HistoryRequest) *enginepb.Reply {
	chat, err := types.ParseJID(c.Chat)
	if err != nil {
		return failure(fmt.Sprintf("%q isn't a JID: %v", c.Chat, err), false)
	}
	count := int(c.Count)
	if count <= 0 {
		count = 50
	}
	oldest := &types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, IsFromMe: c.OldestFromMe},
		ID:            c.OldestId,
		Timestamp:     time.UnixMilli(c.OldestMs),
	}
	if _, err := e.cli().SendPeerMessage(e.ctx, e.cli().BuildHistorySyncRequest(oldest, count)); err != nil {
		return failure(fmt.Sprintf("can't ask the phone for older messages: %v", err), true)
	}
	return done()
}
