package wa

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waMmsRetry"
	"go.mau.fi/whatsmeow/types"
	"go.mau.fi/whatsmeow/types/events"
	"golang.org/x/sys/unix"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// Media: files downloaded for jawd, decrypted on the way to the path it names, and the phone asked
// to upload one again once WhatsApp's server no longer has it. A file is written to a partial file
// of its own, sent to the drive ordered before anything written after it, linked in under its
// name, and its folder likewise, so a crash, a kill or a power cut leaves the whole file under its
// name or nothing, and no row naming it reaches the disk before it. It's durable at the next full
// flush, which jawd's synced commit naming it is, so files cost the drive no flush of their own
// (on macOS; elsewhere fsync is the barrier, and durable). Its name is jawd's, and comes from its
// content, so a file already there is the same file and is never written over.

// mediaDownloader downloads and decrypts a file into the file it's given: whatsmeow's client, or
// a fake in the tests.
type mediaDownloader interface {
	DownloadMediaWithPathToFile(ctx context.Context, directPath string, encFileHash, fileHash, mediaKey []byte,
		mediaType whatsmeow.MediaType, mmsType string, allowNoHash bool, file whatsmeow.File) error
}

// progressEvery is the most often a download says how far it has got.
const progressEvery = 250 * time.Millisecond

// roomKept is what a download leaves free on the disk, besides the file.
const roomKept = 64 << 20

// retryKeysKept is the most keys the engine keeps to read the phone's answers with, and how long.
const (
	retryKeysKept = 4096
	retryKeyFor   = time.Hour
)

// media is what the engine keeps of the downloads in flight and of the files it asked the phone for.
type media struct {
	mu sync.Mutex
	// The downloads running, by jawd's id, to cancel.
	running map[string]context.CancelFunc
	// The media keys of the files the phone was asked to upload again, by message ID.
	keys map[string]retryKey
	// Numbers the partial files this engine writes, so two never share a name.
	next atomic.Uint64
	// Held while a file is sent to the drive and put in place, one at a time.
	finishing sync.Mutex
	// How a file is sent to the drive before it's linked in, and its folder after; the tests' own,
	// to see the order, or barrierSync and barrierSyncDir, or fullSync and syncDir with full.
	syncFile   func(f *os.File, full bool) error
	syncFolder func(dir string, full bool) error
}

func (m *media) flushFile(f *os.File, full bool) error {
	switch {
	case m.syncFile != nil:
		return m.syncFile(f, full)
	case full:
		return fullSync(f)
	}
	return barrierSync(f)
}

func (m *media) flushFolder(dir string, full bool) error {
	switch {
	case m.syncFolder != nil:
		return m.syncFolder(dir, full)
	case full:
		return syncDir(dir)
	}
	return barrierSyncDir(dir)
}

type retryKey struct {
	key   []byte
	asked time.Time
}

// start notes a download running under id; false if one already is.
func (m *media) start(id string, cancel context.CancelFunc) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.running == nil {
		m.running = make(map[string]context.CancelFunc)
	}
	if _, running := m.running[id]; running {
		return false
	}
	m.running[id] = cancel
	return true
}

func (m *media) finish(id string) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.running, id)
}

func (m *media) cancel(id string) {
	m.mu.Lock()
	cancel := m.running[id]
	m.mu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// remember keeps the key a file was asked for with, to read the phone's answer with; the oldest
// go past retryKeysKept, and any kept longer than retryKeyFor.
func (m *media) remember(id string, key []byte) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.keys == nil {
		m.keys = make(map[string]retryKey)
	}
	now := time.Now()
	for kept, k := range m.keys {
		if now.Sub(k.asked) > retryKeyFor {
			delete(m.keys, kept)
		}
	}
	for len(m.keys) >= retryKeysKept {
		oldest, at := "", now
		for kept, k := range m.keys {
			if !k.asked.After(at) {
				oldest, at = kept, k.asked
			}
		}
		delete(m.keys, oldest)
	}
	m.keys[id] = retryKey{key: append([]byte(nil), key...), asked: now}
}

func (m *media) key(id string) []byte {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.keys[id].key
}

func (e *Engine) downloader() mediaDownloader {
	if e.mediaDownloader != nil {
		return e.mediaDownloader
	}
	return e.cli()
}

// mediaTypes are the keys each kind of file is encrypted with, and the kind WhatsApp's media
// servers file it under.
func mediaTypes(kind enginepb.MediaDownload_Kind) (whatsmeow.MediaType, string, bool) {
	switch kind {
	case enginepb.MediaDownload_IMAGE:
		return whatsmeow.MediaImage, "image", true
	case enginepb.MediaDownload_VIDEO:
		return whatsmeow.MediaVideo, "video", true
	case enginepb.MediaDownload_AUDIO:
		return whatsmeow.MediaAudio, "audio", true
	case enginepb.MediaDownload_DOCUMENT:
		return whatsmeow.MediaDocument, "document", true
	}
	return "", "", false
}

func mediaFile(state enginepb.MediaFile_State, bytes uint64, message string) *enginepb.Reply {
	return &enginepb.Reply{Kind: &enginepb.Reply_MediaFile{MediaFile: &enginepb.MediaFile{State: state, Bytes: bytes, Message: message}}}
}

// mediaDownload downloads one file to the path jawd named, as MediaDownload says.
func (e *Engine) mediaDownload(c *enginepb.MediaDownload) *enginepb.Reply {
	mediaType, mmsType, ok := mediaTypes(c.Kind)
	switch {
	case !ok:
		return failure(fmt.Sprintf("there's no kind of file %v to download", c.Kind), false)
	case c.Id == "":
		return failure("a download needs an id", false)
	case !filepath.IsAbs(c.Path):
		return failure(fmt.Sprintf("%q isn't a whole path", c.Path), false)
	case len(c.FileSha256) != sha256.Size:
		return failure("a file to download needs its SHA-256, which names it and checks it", false)
	}
	if info, err := os.Lstat(c.Path); err == nil {
		return mediaFile(enginepb.MediaFile_EXISTS, uint64(info.Size()), "")
	}
	if err := room(filepath.Dir(c.Path), c.Size); err != nil {
		return mediaFile(enginepb.MediaFile_NO_SPACE, 0, err.Error())
	}
	ctx, cancel := context.WithCancel(e.ctx)
	defer cancel()
	if !e.media.start(c.Id, cancel) {
		return failure(fmt.Sprintf("download %s is already running", c.Id), true)
	}
	defer e.media.finish(c.Id)
	part := fmt.Sprintf("%s.%d-%d.part", c.Path, os.Getpid(), e.media.next.Add(1))
	f, err := os.OpenFile(part, os.O_RDWR|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return writeFailed(err)
	}
	placed := false
	defer func() {
		if !placed {
			_ = f.Close()
			_ = os.Remove(part)
		}
	}()
	file := &counted{f: f, report: func(n uint64) { e.progress(c.Id, n, c.Size) }}
	err = e.downloader().DownloadMediaWithPathToFile(ctx, c.DirectPath, c.FileEncSha256, c.FileSha256, c.MediaKey,
		mediaType, mmsType, false, file)
	if err != nil {
		return downloadFailed(err)
	}
	size, err := hashed(f, c.FileSha256)
	if err != nil {
		return writeFailed(err)
	}
	if size < 0 {
		return mediaFile(enginepb.MediaFile_BAD_HASH, 0, "the file downloaded isn't the one its message names: its SHA-256 doesn't match")
	}
	placed = true
	if reply := e.place(f, part, c.Path, uint64(size), c.FullFlush); reply != nil {
		return reply
	}
	e.progress(c.Id, uint64(size), c.Size)
	return mediaFile(enginepb.MediaFile_PLACED, uint64(size), "")
}

// place sends the partial file f to the drive, links it in under path, never over a file there,
// removes the partial file and sends the folder, each flushed in full with full; nil once it's in
// place. One file at a time is placed.
func (e *Engine) place(f *os.File, part, path string, size uint64, full bool) *enginepb.Reply {
	e.media.finishing.Lock()
	defer e.media.finishing.Unlock()
	err := e.media.flushFile(f, full)
	if closeErr := f.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		_ = os.Remove(part)
		return writeFailed(err)
	}
	if err := os.Link(part, path); err != nil {
		_ = os.Remove(part)
		if errors.Is(err, os.ErrExist) {
			return mediaFile(enginepb.MediaFile_EXISTS, size, "")
		}
		return writeFailed(err)
	}
	if err := os.Remove(part); err != nil {
		_ = os.Remove(path)
		return writeFailed(err)
	}
	if err := e.media.flushFolder(filepath.Dir(path), full); err != nil {
		_ = os.Remove(path)
		return writeFailed(err)
	}
	return nil
}

// room says why there isn't room for a file of `size` bytes in dir, leaving roomKept free.
func room(dir string, size uint64) error {
	var stats unix.Statfs_t
	if err := unix.Statfs(dir, &stats); err != nil {
		return fmt.Errorf("can't read the free space in %s: %w", dir, err)
	}
	free := uint64(stats.Bavail) * uint64(stats.Bsize)
	if free < size+roomKept {
		return fmt.Errorf("the disk has %d MB free, and a file of %d MB needs %d MB more beside it",
			free>>20, size>>20, roomKept>>20)
	}
	return nil
}

// hashed checks the whole file against sha, and says its size, or -1 if it doesn't match.
func hashed(f *os.File, sha []byte) (int64, error) {
	if _, err := f.Seek(0, io.SeekStart); err != nil {
		return 0, err
	}
	h := sha256.New()
	size, err := io.Copy(h, f)
	if err != nil {
		return 0, err
	}
	if subtle.ConstantTimeCompare(h.Sum(nil), sha) != 1 {
		return -1, nil
	}
	return size, nil
}

// downloadFailed is what jawd is told of a download whatsmeow gave up on.
func downloadFailed(err error) *enginepb.Reply {
	switch {
	case errors.Is(err, context.Canceled):
		return mediaFile(enginepb.MediaFile_CANCELLED, 0, "the download was cancelled")
	case gone(err):
		return mediaFile(enginepb.MediaFile_GONE, 0, fmt.Sprintf("WhatsApp's server no longer has the file: %v", err))
	case errors.Is(err, whatsmeow.ErrInvalidMediaSHA256),
		errors.Is(err, whatsmeow.ErrInvalidMediaEncSHA256),
		errors.Is(err, whatsmeow.ErrInvalidMediaHMAC),
		errors.Is(err, whatsmeow.ErrInvalidUnencryptedMediaSHA256),
		errors.Is(err, whatsmeow.ErrTooShortFile):
		return mediaFile(enginepb.MediaFile_BAD_HASH, 0, fmt.Sprintf("the file downloaded isn't the one its message names: %v", err))
	case errors.Is(err, syscall.ENOSPC):
		return mediaFile(enginepb.MediaFile_NO_SPACE, 0, fmt.Sprintf("the disk is full: %v", err))
	case errors.Is(err, whatsmeow.ErrNoURLPresent):
		return failure("the message doesn't say where its file is", false)
	}
	return failure(fmt.Sprintf("can't download the file: %v", err), true)
}

// writeFailed is what jawd is told when the file couldn't be written.
func writeFailed(err error) *enginepb.Reply {
	if errors.Is(err, syscall.ENOSPC) {
		return mediaFile(enginepb.MediaFile_NO_SPACE, 0, fmt.Sprintf("the disk is full: %v", err))
	}
	return failure(fmt.Sprintf("can't write the file: %v", err), true)
}

func (e *Engine) progress(id string, bytes, size uint64) {
	e.send(&enginepb.Event{Kind: &enginepb.Event_MediaProgress{MediaProgress: &enginepb.MediaProgress{
		Id: id, Bytes: bytes, Size: size,
	}}})
}

// counted is the file a download is written to, saying how far it has got as whatsmeow writes it.
// It has no ReadFrom, so io.Copy goes through Write and every byte is counted.
type counted struct {
	f       *os.File
	written uint64
	last    time.Time
	report  func(uint64)
}

func (c *counted) Write(p []byte) (int, error) {
	n, err := c.f.Write(p)
	c.written += uint64(n)
	if now := time.Now(); now.Sub(c.last) >= progressEvery {
		c.last = now
		c.report(c.written)
	}
	return n, err
}

func (c *counted) Read(p []byte) (int, error)                   { return c.f.Read(p) }
func (c *counted) ReadAt(p []byte, off int64) (int, error)      { return c.f.ReadAt(p, off) }
func (c *counted) WriteAt(p []byte, off int64) (int, error)     { return c.f.WriteAt(p, off) }
func (c *counted) Seek(offset int64, whence int) (int64, error) { return c.f.Seek(offset, whence) }
func (c *counted) Truncate(size int64) error                    { return c.f.Truncate(size) }
func (c *counted) Stat() (os.FileInfo, error)                   { return c.f.Stat() }

// mediaRetry asks the phone to upload a message's file again, keeping the key to read its answer.
func (e *Engine) mediaRetry(c *enginepb.MediaRetry) *enginepb.Reply {
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
	if c.Id == "" || len(c.MediaKey) != 32 {
		return failure("asking the phone for a file needs its message's ID and its media key", false)
	}
	info := &types.MessageInfo{
		MessageSource: types.MessageSource{Chat: chat, Sender: sender, IsFromMe: c.FromMe, IsGroup: chat.Server == types.GroupServer},
		ID:            c.Id,
	}
	e.media.remember(c.Id, c.MediaKey)
	if err := e.cli().SendMediaRetryReceipt(e.ctx, info, c.MediaKey); err != nil {
		return failure(fmt.Sprintf("can't ask the phone for %s's file: %v", c.Id, err), retryable(err))
	}
	return done()
}

// mediaRetried is the phone's answer to a MediaRetry as jawd keeps it, read with the key it was
// asked with.
func (e *Engine) mediaRetried(v *events.MediaRetry) *enginepb.MediaRetried {
	answer := &enginepb.MediaRetried{
		Id:          string(v.MessageID),
		Chat:        jid(v.ChatID),
		Sender:      jid(v.SenderID),
		FromMe:      v.FromMe,
		TimestampMs: v.Timestamp.UnixMilli(),
	}
	key := e.media.key(string(v.MessageID))
	if key == nil {
		answer.Result = enginepb.MediaRetried_UNREADABLE
		return answer
	}
	notification, err := whatsmeow.DecryptMediaRetryNotification(v, key)
	switch {
	case errors.Is(err, whatsmeow.ErrMediaNotAvailableOnPhone):
		answer.Result = enginepb.MediaRetried_NOT_FOUND
	case err != nil:
		e.log.Warnf("can't read the phone's answer for %s's file: %v", v.MessageID, err)
		answer.Result = enginepb.MediaRetried_GENERAL_ERROR
	case notification.GetResult() == waMmsRetry.MediaRetryNotification_SUCCESS && notification.GetDirectPath() != "":
		answer.Result = enginepb.MediaRetried_SUCCESS
		answer.DirectPath = notification.GetDirectPath()
	case notification.GetResult() == waMmsRetry.MediaRetryNotification_NOT_FOUND:
		answer.Result = enginepb.MediaRetried_NOT_FOUND
	case notification.GetResult() == waMmsRetry.MediaRetryNotification_DECRYPTION_ERROR:
		answer.Result = enginepb.MediaRetried_DECRYPTION_ERROR
	default:
		answer.Result = enginepb.MediaRetried_GENERAL_ERROR
	}
	return answer
}
