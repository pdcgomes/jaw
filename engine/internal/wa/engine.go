package wa

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"slices"
	"sync"
	"time"

	_ "github.com/mattn/go-sqlite3"
	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waE2E"
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
	ctx    context.Context
	link   *link.Link
	acks   *link.Acks
	log    waLog.Logger
	client *whatsmeow.Client
}

// Run speaks to jawd on l until jawd closes it: hello first, then the session in home/session.db.
func Run(ctx context.Context, l *link.Link, home, program, whatsmeowVersion string, log waLog.Logger) error {
	e := newEngine(ctx, l, log)
	if err := e.hello(program, whatsmeowVersion); err != nil {
		return err
	}
	if err := e.open(home); err != nil {
		return err
	}
	defer e.client.Disconnect()
	go e.start()
	return e.serve()
}

func newEngine(ctx context.Context, l *link.Link, log waLog.Logger) *Engine {
	return &Engine{ctx: ctx, link: l, acks: link.NewAcks(), log: log}
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
	dsn := (&url.URL{
		Scheme:   "file",
		Path:     filepath.Join(home, "session.db"),
		RawQuery: "_foreign_keys=on&_journal_mode=WAL&_synchronous=NORMAL&_busy_timeout=5000",
	}).String()
	container, err := sqlstore.New(e.ctx, "sqlite3", dsn, e.log.Sub("db"))
	if err != nil {
		return fmt.Errorf("can't open whatsmeow's session: %w", err)
	}
	device, err := container.GetFirstDevice(e.ctx)
	if err != nil {
		return fmt.Errorf("can't read whatsmeow's device: %w", err)
	}
	client := whatsmeow.NewClient(device, e.log.Sub("client"))
	// WhatsApp hears a message arrived only once jawd has stored it, and a message it sends again
	// after a crash isn't decrypted twice.
	client.SynchronousAck = true
	client.EnableDecryptedEventBuffer = true
	// jawd asks for each history blob when it's ready for it.
	client.ManualHistorySyncDownload = true
	client.EmitAppStateEventsOnFullSync = true
	client.AutomaticMessageRerequestFromPhone = true
	if proxy := os.Getenv("HTTPS_PROXY"); proxy != "" {
		if err := client.SetProxyAddress(proxy); err != nil {
			return fmt.Errorf("can't use the proxy in HTTPS_PROXY: %w", err)
		}
	}
	client.AddEventHandler(e.handle)
	e.client = client
	return nil
}

// start connects when an account is linked, and otherwise says there's none.
func (e *Engine) start() {
	if e.client.Store.ID == nil {
		e.connection(enginepb.Connection_UNLINKED, "")
		return
	}
	e.connection(enginepb.Connection_CONNECTING, "")
	if err := e.client.Connect(); err != nil {
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
}

// handle is whatsmeow's event handler. It runs on whatsmeow's one queue, so it hands each event on
// and returns; what whatsmeow won't send again waits for jawd's ack.
func (e *Engine) handle(evt any) {
	switch v := evt.(type) {
	case *events.Message:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_Message{Message: e.message(v)}})
	case *events.AppState:
		if state, err := appState(v); err == nil {
			e.keep(&enginepb.Event{Kind: &enginepb.Event_AppState{AppState: state}})
		} else {
			e.log.Errorf("can't encode the app state %v: %v", v.Index, err)
		}
	case *events.AppStateSyncComplete:
		e.send(&enginepb.Event{Kind: &enginepb.Event_AppStateSynced{AppStateSynced: &enginepb.AppStateSynced{
			Name:    string(v.Name),
			Version: v.Version,
		}}})
	case *events.PushName:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_PushName{PushName: pushName(v)}})
	case *events.JoinedGroup:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_Group{Group: groupInfo(&v.GroupInfo)}})
	case *events.GroupInfo:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_GroupChange{GroupChange: groupChange(v)}})
	case *events.Picture:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_Picture{Picture: picture(v)}})
	case *events.Receipt:
		e.keep(&enginepb.Event{Kind: &enginepb.Event_Receipt{Receipt: receipt(v)}})
	case *events.Connected:
		e.connection(enginepb.Connection_CONNECTED, "")
	case *events.Disconnected:
		e.connection(enginepb.Connection_DISCONNECTED, "")
	case *events.KeepAliveTimeout:
		e.connection(enginepb.Connection_KEEPALIVE_TIMEOUT, fmt.Sprintf("%d keepalives failed", v.ErrorCount))
	case *events.LoggedOut:
		e.connection(enginepb.Connection_LOGGED_OUT, v.Reason.String())
	case *events.StreamReplaced:
		e.connection(enginepb.Connection_STREAM_REPLACED, "")
	case *events.ClientOutdated:
		e.connection(enginepb.Connection_CLIENT_OUTDATED, "")
	case *events.ConnectFailure:
		e.connection(enginepb.Connection_CONNECT_FAILED, fmt.Sprintf("%s %s", v.Reason, v.Message))
	case *events.TemporaryBan:
		state := e.state(enginepb.Connection_TEMPORARY_BAN, v.Code.String())
		state.UntilMs = time.Now().Add(v.Expire).UnixMilli()
		e.send(&enginepb.Event{Kind: &enginepb.Event_Connection{Connection: state}})
	case *events.PairSuccess:
		e.send(&enginepb.Event{Kind: &enginepb.Event_Link{Link: &enginepb.Link{Step: &enginepb.Link_Done{Done: &enginepb.Linked{
			Jid:          v.ID.String(),
			Lid:          v.LID.String(),
			Platform:     v.Platform,
			BusinessName: v.BusinessName,
		}}}}})
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

func (e *Engine) state(state enginepb.Connection_State, reason string) *enginepb.Connection {
	c := &enginepb.Connection{State: state, Reason: reason}
	if e.client != nil && e.client.Store.ID != nil {
		c.Jid = e.client.Store.ID.String()
		c.Lid = e.client.Store.LID.String()
		c.PushName = e.client.Store.PushName
	}
	return c
}

func (e *Engine) connection(state enginepb.Connection_State, reason string) {
	e.send(&enginepb.Event{Kind: &enginepb.Event_Connection{Connection: e.state(state, reason)}})
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
		if vote, err := e.client.DecryptPollVote(e.ctx, v); err == nil {
			decrypted = vote
		} else {
			e.log.Warnf("can't decrypt the poll vote in %s: %v", v.Info.ID, err)
		}
	case v.Message.GetEncReactionMessage() != nil:
		if reaction, err := e.client.DecryptReaction(e.ctx, v); err == nil {
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
	}
	if e.client == nil {
		return failure("the engine has no session open", false)
	}
	switch c := cmd.Kind.(type) {
	case *enginepb.Command_LinkStart:
		return e.linkStart(c.LinkStart.Phone)
	case *enginepb.Command_LinkCancel:
		e.client.Disconnect()
		return done()
	case *enginepb.Command_Logout:
		if err := e.client.Logout(e.ctx); err != nil {
			return failure(fmt.Sprintf("can't log out: %v", err), true)
		}
		return done()
	case *enginepb.Command_HistoryDownload:
		return e.historyDownload(c.HistoryDownload)
	case *enginepb.Command_Contacts:
		return e.contacts()
	case *enginepb.Command_LidLookup:
		return e.lidLookup(c.LidLookup)
	}
	if e.client.Store.ID == nil {
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

// linkStart links an account: by QR codes, or by a code to type on the phone when phone is set,
// which needs the connection up first, as whatsmeow's PairPhone says.
func (e *Engine) linkStart(phone string) *enginepb.Reply {
	if e.client.Store.ID != nil {
		return failure("an account is already linked; log out first", false)
	}
	e.client.Disconnect()
	codes, err := e.client.GetQRChannel(e.ctx)
	if err != nil {
		return failure(fmt.Sprintf("can't start linking: %v", err), true)
	}
	if err := e.client.Connect(); err != nil {
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
				// PairSuccess has already said so, with the account.
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
	code, err := e.client.PairPhone(e.ctx, phone, true, whatsmeow.PairClientChrome, "Chrome (Linux)")
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
	history, err := e.client.DownloadHistorySync(e.ctx, notification, true)
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
