package wa

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"go.mau.fi/util/dbutil"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
	"go.mau.fi/whatsmeow/store"
	"go.mau.fi/whatsmeow/store/sqlstore"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	waLog "go.mau.fi/whatsmeow/util/log"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// Engine is one run of jaw-engine: whatsmeow's events out to jawd, jawd's commands in.
type Engine struct {
	ctx       context.Context
	link      *link.Link
	acks      *link.Acks
	log       waLog.Logger
	container *sqlstore.Container
	echoes    echoes
	// told is the connection state the engine told jawd last, so the keepalive's aren't said out of
	// order (keepalive.go).
	told told
	// client is whatsmeow's client for the device in session.db, replaced by one on a fresh device
	// once the account is logged out (renew); retired is the last one replaced so.
	client  atomic.Pointer[whatsmeow.Client]
	retired atomic.Pointer[whatsmeow.Client]
	// The link the phone has paired, while it's finishing (pairing.go).
	pairing pairing
	reach   reach
	media   media
	// What downloads and uploads files instead of the client, in the tests.
	mediaDownloader mediaDownloader
	mediaUploader   mediaUploader
}

// cli is the client running now, nil until the session is open.
func (e *Engine) cli() *whatsmeow.Client {
	return e.client.Load()
}

// Run speaks to jawd on l until jawd closes it: hello first, then the session in home/session.db,
// whose session.lock the caller holds (LockSession). A session.db from a newer whatsmeow, which
// this one can't read, is said as WhatsApp saying jaw is outdated, and the engine answers what it
// can until jawd closes the link, since starting it again wouldn't read it either.
func Run(ctx context.Context, l *link.Link, home, program, whatsmeowVersion string, log waLog.Logger) error {
	e := newEngine(ctx, l, log)
	if err := e.hello(program, whatsmeowVersion); err != nil {
		return err
	}
	if err := e.open(home); errors.Is(err, dbutil.ErrUnsupportedDatabaseVersion) {
		log.Errorf("%v", err)
		e.connection(enginepb.Connection_CLIENT_OUTDATED, "session.db is from a newer jaw-engine: "+errors.Unwrap(err).Error())
		return e.serve()
	} else if err != nil {
		return err
	}
	defer func() { e.cli().Disconnect() }()
	go e.start()
	return e.serve()
}

func newEngine(ctx context.Context, l *link.Link, log waLog.Logger) *Engine {
	e := &Engine{ctx: ctx, link: l, acks: link.NewAcks(), log: log, reach: whatsmeowReach{}}
	e.told.apart, e.told.connected = whatsmeow.KeepAliveIntervalMin, connected
	return e
}

// hello sends the engine's Hello and checks jawd's.
func (e *Engine) hello(program, whatsmeowVersion string) error {
	ours := &enginepb.Hello{Protocol: link.Protocol, Program: program, Whatsmeow: whatsmeowVersion}
	if err := e.link.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: ours}}); err != nil {
		return err
	}
	frame, err := e.link.Recv()
	if err != nil {
		return fmt.Errorf("jawd didn't say hello: %w", err)
	}
	theirs := frame.GetHello()
	if theirs == nil {
		return errors.New("jawd's first frame wasn't a hello")
	}
	if theirs.Protocol != link.Protocol {
		return fmt.Errorf("jawd speaks link protocol %d and this engine %d", theirs.Protocol, link.Protocol)
	}
	return nil
}

// open opens whatsmeow's session and makes the client, without connecting.
func (e *Engine) open(home string) error {
	if err := os.MkdirAll(home, 0o700); err != nil {
		return err
	}
	container, err := openSession(e.ctx, home, e.log.Sub("db"))
	if err != nil {
		return fmt.Errorf("can't open whatsmeow's session: %w", err)
	}
	device, err := container.GetFirstDevice(e.ctx)
	if err != nil {
		return fmt.Errorf("can't read whatsmeow's device: %w", err)
	}
	client, err := e.newClient(device)
	if err != nil {
		return err
	}
	e.container = container
	e.client.Store(client)
	return nil
}

// newClient makes whatsmeow's client for device, with jaw's settings, its events handed on by
// handleFrom.
func (e *Engine) newClient(device *store.Device) (*whatsmeow.Client, error) {
	client := whatsmeow.NewClient(device, e.log.Sub("client"))
	// WhatsApp hears a message arrived only once jawd has stored it, and a message it sends again
	// after a crash isn't decrypted twice.
	client.SynchronousAck = true
	client.EnableDecryptedEventBuffer = true
	// jawd asks for each history blob when it's ready for it.
	client.ManualHistorySyncDownload = true
	client.EmitAppStateEventsOnFullSync = true
	client.AutomaticMessageRerequestFromPhone = true
	// A recipient's retry receipt for a message sent before a restart can still be answered.
	client.UseRetryMessageStore = true
	if proxy := os.Getenv("HTTPS_PROXY"); proxy != "" {
		if err := client.SetProxyAddress(proxy); err != nil {
			return nil, fmt.Errorf("can't use the proxy in HTTPS_PROXY: %w", err)
		}
	}
	client.AddEventHandler(func(evt any) { e.handleFrom(client, evt) })
	return client, nil
}

// start connects when an account is linked, and otherwise says there's none.
func (e *Engine) start() {
	client := e.cli()
	if client.Store.ID == nil {
		e.connection(enginepb.Connection_UNLINKED, "")
		return
	}
	e.connection(enginepb.Connection_CONNECTING, "")
	if err := client.Connect(); err != nil {
		e.connection(enginepb.Connection_CONNECT_FAILED, err.Error())
	}
}

// serve reads jawd's frames until it closes the link.
func (e *Engine) serve() error {
	for {
		frame, err := e.link.Recv()
		if err != nil {
			e.acks.Close()
			if errors.Is(err, io.EOF) {
				return nil
			}
			return err
		}
		switch body := frame.Body.(type) {
		case *enginepb.Frame_Ack:
			e.acks.Acked(body.Ack.Seq)
		case *enginepb.Frame_Command:
			go e.command(frame.Id, body.Command)
		default:
			e.log.Warnf("jawd sent a frame the engine doesn't take: %T", frame.Body)
		}
	}
}

func (e *Engine) send(event *enginepb.Event) {
	if err := e.link.Send(&enginepb.Frame{Body: &enginepb.Frame_Event{Event: event}}); err != nil {
		e.log.Errorf("can't send an event to jawd: %v", err)
	}
}

// durable sends an event jawd must store and waits until it has. It returns false if jawd went
// first, and then nothing it was sent can be counted on.
func (e *Engine) durable(event *enginepb.Event) bool {
	seq, done := e.acks.Next()
	event.Seq = seq
	e.send(event)
	select {
	case <-done:
		return true
	case <-e.acks.Gone():
		return false
	}
}

// keep sends an event jawd must store and waits until it has. If jawd has gone, it never returns:
// returning would let whatsmeow tell WhatsApp the event arrived, or move past it, and the engine is
// about to exit anyway, so WhatsApp sends it again.
func (e *Engine) keep(event *enginepb.Event) {
	if !e.durable(event) {
		select {}
	}
	// Only once jawd has it, from whichever client, may a patch waiting for this echo be answered.
	if state := event.GetAppState(); state != nil {
		e.echoes.kept(state)
	}
}

// handleFrom is whatsmeow's event handler for client. What jawd must store goes on from any
// client, since whatsmeow tells WhatsApp it arrived once this returns. The rest goes on only from
// the client running now: once the account is logged out, whatsmeow can still have its client say
// how its connection went (Connected after LoggedOut), which would tell jawd the removed device is
// linked again.
func (e *Engine) handleFrom(client *whatsmeow.Client, evt any) {
	if out, ok := evt.(*events.LoggedOut); ok {
		e.loggedOut(client, out.Reason)
		return
	}
	if client == e.cli() && client != e.retired.Load() {
		e.handle(client, evt)
	} else if event, _ := e.kept(evt); event != nil {
		e.keep(event)
	}
}

// handle hands one of the running client's events to jawd. It runs on whatsmeow's one queue, so it
// hands each event on and returns; what jawd must store waits for jawd's ack.
func (e *Engine) handle(client *whatsmeow.Client, evt any) {
	if event, kept := e.kept(evt); event != nil {
		e.keep(event)
	} else if !kept {
		e.say(client, evt)
	}
}

// loggedOut tells jawd WhatsApp logged the account out, why in whatsmeow's words without the code
// ("logged out from another device", not "401: logged out from another device"), and gives the
// engine a fresh device to link again with. It reads nothing of the device, which whatsmeow is
// deleting meanwhile, and says it once, for the client running now only. A device whose link was
// still finishing was never the account's, so that link has failed instead.
func (e *Engine) loggedOut(client *whatsmeow.Client, reason events.ConnectFailureReason) {
	why := strings.TrimPrefix(reason.String(), reason.NumberString()+": ")
	if e.linkFailed(client, "WhatsApp refused the new device: "+why) {
		return
	}
	if !e.retire(client) {
		return
	}
	e.tell(&enginepb.Connection{State: enginepb.Connection_LOGGED_OUT, Reason: why})
	e.renew(client)
}

// retire marks client logged out, once, if it's the client running now: renewing deletes every
// device in session.db, so a late word from a client replaced before mustn't reach it, or it
// would delete the device linked since.
func (e *Engine) retire(client *whatsmeow.Client) bool {
	return client == e.cli() && e.retired.Swap(client) != client
}

// renew replaces a client whose account is logged out with one on a fresh device, so linking again
// works without starting the engine again: whatsmeow deletes the device from session.db and won't
// connect its client again. A device left in session.db would be read as the account at the next
// start, so none is kept; if one can't be deleted, linking again waits for that start.
func (e *Engine) renew(old *whatsmeow.Client) {
	defer old.Disconnect()
	devices, err := e.container.GetAllDevices(e.ctx)
	for _, device := range devices {
		if err == nil {
			err = e.container.DeleteDevice(e.ctx, device)
		}
	}
	var fresh *whatsmeow.Client
	if err == nil {
		fresh, err = e.newClient(e.container.NewDevice())
	}
	if err != nil {
		e.log.Errorf("can't open a fresh device after the logout, so linking again waits for the engine to start again: %v", err)
		return
	}
	e.client.Store(fresh)
}

// kept is what jawd must store of evt, which whatsmeow won't send again once it's handled, and
// whether evt is of a kind jawd must store; one of those that can't be encoded is said in the log
// and comes back nil. It's the one list of those kinds, so none can go unstored from a client
// replaced after a logout.
func (e *Engine) kept(evt any) (*enginepb.Event, bool) {
	switch v := evt.(type) {
	case *events.Message:
		return &enginepb.Event{Kind: &enginepb.Event_Message{Message: e.message(v)}}, true
	case *events.AppState:
		state, err := appState(v)
		if err != nil {
			e.log.Errorf("can't encode the app state %v: %v", v.Index, err)
			return nil, true
		}
		return &enginepb.Event{Kind: &enginepb.Event_AppState{AppState: state}}, true
	case *events.PushName:
		return &enginepb.Event{Kind: &enginepb.Event_PushName{PushName: pushName(v)}}, true
	case *events.JoinedGroup:
		return &enginepb.Event{Kind: &enginepb.Event_Group{Group: groupInfo(&v.GroupInfo)}}, true
	case *events.GroupInfo:
		return &enginepb.Event{Kind: &enginepb.Event_GroupChange{GroupChange: groupChange(v)}}, true
	case *events.Picture:
		return &enginepb.Event{Kind: &enginepb.Event_Picture{Picture: picture(v)}}, true
	case *events.Receipt:
		return &enginepb.Event{Kind: &enginepb.Event_Receipt{Receipt: receipt(v)}}, true
	case *events.MediaRetry:
		return &enginepb.Event{Kind: &enginepb.Event_MediaRetried{MediaRetried: e.mediaRetried(v)}}, true
	}
	return nil, false
}

// say tells jawd what client, the one running now, says of its connection, its linking, catching
// up and app state's versions, none of which needs an ack. WhatsApp refusing a device whose link
// is finishing fails that link before the refusal is said, so it names no account.
func (e *Engine) say(client *whatsmeow.Client, evt any) {
	switch v := evt.(type) {
	case *events.AppStateSyncComplete:
		e.send(&enginepb.Event{Kind: &enginepb.Event_AppStateSynced{AppStateSynced: &enginepb.AppStateSynced{
			Name:    string(v.Name),
			Version: v.Version,
		}}})
	case *events.Presence:
		e.send(&enginepb.Event{Kind: &enginepb.Event_Presence{Presence: presenceUpdate(v)}})
	case *events.ChatPresence:
		e.send(&enginepb.Event{Kind: &enginepb.Event_Typing{Typing: typing(v)}})
	case *events.Connected:
		if e.finished(client) {
			e.tellConnected(client.LastSuccessfulConnect)
		}
	case *events.Disconnected:
		e.connection(enginepb.Connection_DISCONNECTED, "")
	case *events.KeepAliveTimeout:
		e.timedOut(v)
	case *events.KeepAliveRestored:
		e.restored()
	case *events.StreamReplaced:
		e.connection(enginepb.Connection_STREAM_REPLACED, "")
	case *events.ClientOutdated:
		e.linkFailed(client, "WhatsApp wants a newer version of jaw")
		e.connection(enginepb.Connection_CLIENT_OUTDATED, "")
	case *events.ConnectFailure:
		reason := fmt.Sprintf("%s %s", v.Reason, v.Message)
		e.linkFailed(client, "WhatsApp refused the new device: "+reason)
		e.connection(enginepb.Connection_CONNECT_FAILED, reason)
	case *events.TemporaryBan:
		e.linkFailed(client, "WhatsApp has banned the account for a while")
		state := e.state(enginepb.Connection_TEMPORARY_BAN, v.Code.String())
		state.UntilMs = time.Now().Add(v.Expire).UnixMilli()
		e.tell(state)
	case *events.PairSuccess:
		e.paired(client, v)
	case *events.PairError:
		// The QR codes' channel says why, after this; whatever device pairing left, with its ID
		// or deleted by whatsmeow, goes first, so linking can start again.
		if client.Store.ID != nil || client.Store.Deleted {
			e.clear(client)
		}
	case *events.OfflineSyncPreview:
		e.send(&enginepb.Event{Kind: &enginepb.Event_OfflinePreview{OfflinePreview: &enginepb.OfflinePreview{
			Total:          uint32(v.Total),
			Messages:       uint32(v.Messages),
			Receipts:       uint32(v.Receipts),
			Notifications:  uint32(v.Notifications),
			AppDataChanges: uint32(v.AppDataChanges),
		}}})
	case *events.OfflineSyncCompleted:
		e.send(&enginepb.Event{Kind: &enginepb.Event_OfflineDone{OfflineDone: &enginepb.OfflineDone{Count: uint32(v.Count)}}})
	}
}

// state is the connection in state, naming the account once one is linked: not while a link is
// finishing, whose device may yet go.
func (e *Engine) state(state enginepb.Connection_State, reason string) *enginepb.Connection {
	c := &enginepb.Connection{State: state, Reason: reason}
	if client := e.cli(); client != nil && !e.half(client) {
		if id := client.Store.ID; id != nil {
			c.Jid = id.String()
			c.Lid = client.Store.LID.String()
			c.PushName = client.Store.PushName
		}
	}
	return c
}

func (e *Engine) connection(state enginepb.Connection_State, reason string) {
	e.tell(e.state(state, reason))
}

// message is a whatsmeow message as jawd takes it: the raw protobuf, still wrapped, and what only
// the engine can decrypt.
func (e *Engine) message(v *events.Message) *enginepb.Message {
	raw := v.RawMessage
	if raw == nil {
		raw = v.Message
	}
	m := &enginepb.Message{Info: info(&v.Info), RetryCount: uint32(v.RetryCount), UnavailableRequest: string(v.UnavailableRequestID)}
	if data, err := proto.Marshal(raw); err == nil {
		m.Raw = data
	} else {
		e.log.Errorf("can't encode message %s: %v", v.Info.ID, err)
	}
	var decrypted proto.Message
	switch {
	case v.Message.GetPollUpdateMessage() != nil:
		if vote, err := e.cli().DecryptPollVote(e.ctx, v); err == nil {
			decrypted = vote
		} else {
			e.log.Warnf("can't decrypt the poll vote in %s: %v", v.Info.ID, err)
		}
	case v.Message.GetEncReactionMessage() != nil:
		if reaction, err := e.cli().DecryptReaction(e.ctx, v); err == nil {
			decrypted = reaction
		} else {
			e.log.Warnf("can't decrypt the reaction in %s: %v", v.Info.ID, err)
		}
	}
	if decrypted != nil {
		if data, err := proto.Marshal(decrypted); err == nil {
			m.Decrypted = data
		}
	}
	return m
}

func info(i *types.MessageInfo) *enginepb.MessageInfo {
	m := &enginepb.MessageInfo{
		Chat:           i.Chat.String(),
		Sender:         i.Sender.String(),
		FromMe:         i.IsFromMe,
		Group:          i.IsGroup,
		AddressingMode: string(i.AddressingMode),
		SenderAlt:      jid(i.SenderAlt),
		RecipientAlt:   jid(i.RecipientAlt),
		BroadcastOwner: jid(i.BroadcastListOwner),
		Id:             i.ID,
		ServerId:       uint32(i.ServerID),
		Type:           i.Type,
		PushName:       i.PushName,
		TimestampMs:    i.Timestamp.UnixMilli(),
		Category:       i.Category,
		Multicast:      i.Multicast,
		MediaType:      i.MediaType,
		Edit:           string(i.Edit),
	}
	if i.DeviceSentMeta != nil {
		m.DeviceSentDestination = i.DeviceSentMeta.DestinationJID
	}
	if i.VerifiedName != nil && i.VerifiedName.Details != nil {
		m.VerifiedName = i.VerifiedName.Details.GetVerifiedName()
	}
	return m
}

func receipt(v *events.Receipt) *enginepb.Receipt {
	return &enginepb.Receipt{
		Chat:          v.Chat.String(),
		Sender:        v.Sender.String(),
		FromMe:        v.IsFromMe,
		Group:         v.IsGroup,
		SenderAlt:     jid(v.SenderAlt),
		Ids:           slices.Clone(v.MessageIDs),
		TimestampMs:   v.Timestamp.UnixMilli(),
		Type:          string(v.Type),
		MessageSender: jid(v.MessageSender),
	}
}

func jid(j types.JID) string {
	if j.IsEmpty() {
		return ""
	}
	return j.String()
}

// ---------------------------------------------------------------- commands

func (e *Engine) command(id uint64, cmd *enginepb.Command) {
	reply := e.do(cmd)
	if err := e.link.Send(&enginepb.Frame{Id: id, Body: &enginepb.Frame_Reply{Reply: reply}}); err != nil {
		e.log.Errorf("can't reply to jawd: %v", err)
	}
}

func (e *Engine) do(cmd *enginepb.Command) *enginepb.Reply {
	switch c := cmd.Kind.(type) {
	case *enginepb.Command_Ping:
		return &enginepb.Reply{Kind: &enginepb.Reply_Pong{Pong: &enginepb.Pong{AtMs: time.Now().UnixMilli()}}}
	case *enginepb.Command_BenchRun:
		return e.bench(c.BenchRun.Count, c.BenchRun.Size)
	case *enginepb.Command_MediaCancel:
		e.media.cancel(c.MediaCancel.Id)
		return done()
	}
	client := e.cli()
	if client == nil {
		return failure("the engine has no session open", false)
	}
	switch c := cmd.Kind.(type) {
	case *enginepb.Command_LinkStart:
		return e.linkStart(c.LinkStart.Phone)
	case *enginepb.Command_LinkCancel:
		if c.LinkCancel.GetOnlyLinking() && client.Store.ID != nil {
			return done()
		}
		e.reach.disconnect(client)
		return done()
	case *enginepb.Command_Logout:
		if client.Store.ID == nil || e.half(client) {
			return failure("no account is linked", false)
		}
		if err := e.reach.logout(e.ctx, client); err != nil {
			return failure(fmt.Sprintf("can't log out: %v", err), true)
		}
		// whatsmeow says nothing after a logout jawd asked for, and deletes the device as it does
		// after the phone's.
		if e.retire(client) {
			e.renew(client)
		}
		return done()
	case *enginepb.Command_HistoryDownload:
		return e.historyDownload(c.HistoryDownload)
	case *enginepb.Command_Contacts:
		return e.contacts()
	case *enginepb.Command_LidLookup:
		return e.lidLookup(c.LidLookup)
	}
	if client.Store.ID == nil || e.half(client) {
		return failure("no account is linked yet", false)
	}
	switch c := cmd.Kind.(type) {
	case *enginepb.Command_Groups:
		return e.groups()
	case *enginepb.Command_AppStateResync:
		return e.appStateResync(c.AppStateResync)
	case *enginepb.Command_Avatar:
		return e.avatar(c.Avatar)
	case *enginepb.Command_HistoryRequest:
		return e.historyRequest(c.HistoryRequest)
	case *enginepb.Command_SubscribePresence:
		return e.subscribePresence(c.SubscribePresence)
	case *enginepb.Command_UserInfo:
		return e.userInfo(c.UserInfo)
	case *enginepb.Command_MediaDownload:
		return e.mediaDownload(c.MediaDownload)
	case *enginepb.Command_MediaUpload:
		return e.mediaUpload(c.MediaUpload)
	case *enginepb.Command_MediaRetry:
		return e.mediaRetry(c.MediaRetry)
	}
	if reply := e.sending(cmd); reply != nil {
		return reply
	}
	return failure(fmt.Sprintf("the engine doesn't know %T", cmd.Kind), false)
}

func done() *enginepb.Reply {
	return &enginepb.Reply{Kind: &enginepb.Reply_Done{Done: &enginepb.Done{}}}
}

func failure(message string, retry bool) *enginepb.Reply {
	return &enginepb.Reply{Kind: &enginepb.Reply_Failure{Failure: &enginepb.Failure{Message: message, Retry: retry}}}
}

func (e *Engine) step(step *enginepb.Link) {
	e.send(&enginepb.Event{Kind: &enginepb.Event_Link{Link: step}})
}

// notCleared is why linking can't start while the device a logout or a failed link left hasn't
// given way to a fresh one: whatsmeow is still deleting it, or the engine couldn't (renew).
const notCleared = "the device the last account or link left isn't cleared yet; try again in a moment, or once jawd has started the engine again"

// linkStart links an account: by QR codes, or by a code to type on the phone when phone is set,
// which needs the connection up first, as whatsmeow's PairPhone says. A link the phone has paired
// that can't finish now, its client not connected, is cleared first; one that's connected may.
func (e *Engine) linkStart(phone string) *enginepb.Reply {
	client := e.cli()
	if client == e.retired.Load() || client.Store.Deleted {
		return failure(notCleared, true)
	}
	if e.half(client) {
		if e.reach.connected(client) {
			return failure("the phone has paired, and linking is finishing; try again if it doesn't", true)
		}
		e.abandon(client)
		if client = e.cli(); client == e.retired.Load() {
			return failure(notCleared, true)
		}
	}
	if client.Store.ID != nil {
		return failure("an account is already linked; log out first", false)
	}
	e.reach.disconnect(client)
	codes, err := client.GetQRChannel(e.ctx)
	if err != nil {
		return failure(fmt.Sprintf("can't start linking: %v", err), true)
	}
	if err := e.reach.connect(client); err != nil {
		return failure(fmt.Sprintf("can't reach WhatsApp: %v", err), true)
	}
	ready := make(chan struct{})
	go func() {
		var once sync.Once
		defer once.Do(func() { close(ready) })
		for item := range codes {
			switch item.Event {
			case whatsmeow.QRChannelEventCode:
				once.Do(func() { close(ready) })
				if phone == "" {
					expires := time.Now().Add(item.Timeout).UnixMilli()
					e.step(&enginepb.Link{Step: &enginepb.Link_Qr{Qr: &enginepb.QrCode{Code: item.Code, ExpiresMs: expires}}})
				}
			case whatsmeow.QRChannelSuccess.Event:
				// The phone has paired: the link finishes once the client connects with the new
				// device, or fails (pairing.go).
			case whatsmeow.QRChannelTimeout.Event:
				e.step(&enginepb.Link{Step: &enginepb.Link_Failed{Failed: "the phone didn't link in time; start again"}})
			case whatsmeow.QRChannelClientOutdated.Event:
				e.step(&enginepb.Link{Step: &enginepb.Link_Failed{Failed: "WhatsApp wants a newer version of jaw"}})
			case whatsmeow.QRChannelEventError:
				e.step(&enginepb.Link{Step: &enginepb.Link_Failed{Failed: fmt.Sprintf("linking failed: %v", item.Error)}})
			default:
				e.step(&enginepb.Link{Step: &enginepb.Link_Failed{Failed: fmt.Sprintf("linking stopped: %s", item.Event)}})
			}
		}
	}()
	if phone == "" {
		return done()
	}
	select {
	case <-ready:
	case <-time.After(30 * time.Second):
		return failure("WhatsApp didn't get ready to link within 30 seconds", true)
	}
	// The server checks the name is "Browser (OS)" with a browser and system it knows.
	code, err := client.PairPhone(e.ctx, phone, true, whatsmeow.PairClientChrome, "Chrome (Linux)")
	if err != nil {
		return failure(fmt.Sprintf("can't get a code for %s: %v", phone, err), true)
	}
	e.step(&enginepb.Link{Step: &enginepb.Link_Code{Code: code}})
	return done()
}

// historyDownload downloads one history blob and writes the HistorySync to the path jawd chose,
// whole or not at all. whatsmeow stores what it needs from it too, such as poll secrets.
func (e *Engine) historyDownload(c *enginepb.HistoryDownload) *enginepb.Reply {
	notification := &waE2E.HistorySyncNotification{}
	if err := proto.Unmarshal(c.Notification, notification); err != nil {
		return failure(fmt.Sprintf("that isn't a history notification: %v", err), false)
	}
	history, err := e.cli().DownloadHistorySync(e.ctx, notification, true)
	if err != nil {
		return failure(fmt.Sprintf("can't download the history: %v", err), !gone(err))
	}
	data, err := proto.Marshal(history)
	if err != nil {
		return failure(fmt.Sprintf("can't encode the history: %v", err), false)
	}
	if err := writeWhole(c.Path, data); err != nil {
		return failure(fmt.Sprintf("can't write the history: %v", err), true)
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_HistoryFile{HistoryFile: &enginepb.HistoryFile{
		Path:          c.Path,
		Type:          int32(history.GetSyncType()),
		ChunkOrder:    history.GetChunkOrder(),
		Progress:      history.GetProgress(),
		Bytes:         uint64(len(data)),
		Conversations: uint32(len(history.GetConversations())),
	}}}
}

// gone says whether a download failed because WhatsApp no longer has the file, so asking again
// can't help.
func gone(err error) bool {
	return errors.Is(err, whatsmeow.ErrMediaDownloadFailedWith403) ||
		errors.Is(err, whatsmeow.ErrMediaDownloadFailedWith404) ||
		errors.Is(err, whatsmeow.ErrMediaDownloadFailedWith410)
}

// writeWhole writes data to path through a .part file, synced before it's renamed into place, and
// syncs the folder after, so after a power cut the file is either whole or not there.
func writeWhole(path string, data []byte) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	part := path + ".part"
	f, err := os.OpenFile(part, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o600)
	if err != nil {
		return err
	}
	_, err = f.Write(data)
	if err == nil {
		err = fullSync(f)
	}
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		_ = os.Remove(part)
		return err
	}
	if err := os.Rename(part, path); err != nil {
		return err
	}
	return syncDir(dir)
}

// syncDir makes a rename in dir durable.
func syncDir(dir string) error {
	d, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer d.Close()
	return fullSync(d)
}

// barrierSyncDir orders a name made in dir before everything written after it, as barrierSync
// does a file.
func barrierSyncDir(dir string) error {
	d, err := os.Open(dir)
	if err != nil {
		return err
	}
	defer d.Close()
	return barrierSync(d)
}

// bench sends Bench events the way a message is sent, each waiting for its ack, and measures the
// round trips.
func (e *Engine) bench(count, size uint32) *enginepb.Reply {
	payload := make([]byte, size)
	trips := make([]time.Duration, 0, count)
	start := time.Now()
	for range count {
		sent := time.Now()
		if !e.durable(&enginepb.Event{Kind: &enginepb.Event_Bench{Bench: &enginepb.Bench{Payload: payload}}}) {
			return failure("jawd went during the bench", false)
		}
		trips = append(trips, time.Since(sent))
	}
	result := &enginepb.BenchResult{Count: count, ElapsedUs: uint64(time.Since(start).Microseconds())}
	if len(trips) > 0 {
		slices.Sort(trips)
		result.P50Us = uint64(trips[len(trips)/2].Microseconds())
		result.P99Us = uint64(trips[len(trips)*99/100].Microseconds())
		result.MaxUs = uint64(trips[len(trips)-1].Microseconds())
	}
	return &enginepb.Reply{Kind: &enginepb.Reply_BenchResult{BenchResult: result}}
}
