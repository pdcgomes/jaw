package wa

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"errors"
	"fmt"
	"hash"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"time"

	"go.mau.fi/whatsmeow"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// Uploads: a file jawd names, encrypted with a media key of its own on its way to WhatsApp's media
// server. whatsmeow encrypts it into a file and uploads that, so the engine gives it a partial file
// of its own beside the name jawd chose, in jaw's folder, and removes it once the upload is over: a
// crash's is removed at jawd's start, and the file is never held in memory.

// mediaUploader encrypts and uploads what it reads: whatsmeow's client, or a fake in the tests.
type mediaUploader interface {
	UploadReader(ctx context.Context, plaintext io.Reader, tempFile io.ReadWriteSeeker, appInfo whatsmeow.MediaType) (whatsmeow.UploadResponse, error)
}

func (e *Engine) uploader() mediaUploader {
	if e.mediaUploader != nil {
		return e.mediaUploader
	}
	return e.cli()
}

// errChanged is what the encrypted copy says when the file it was made from isn't the one jawd named.
var errChanged = errors.New("the file isn't the one jawd named: its SHA-256 doesn't match")

func uploaded(state enginepb.MediaUploaded_State, message string) *enginepb.Reply {
	return &enginepb.Reply{Kind: &enginepb.Reply_MediaUploaded{MediaUploaded: &enginepb.MediaUploaded{State: state, Message: message}}}
}

// encryptedSize is the size of a file of `size` bytes once encrypted: AES-CBC's padding, which is
// always at least one byte, and the MAC's ten.
func encryptedSize(size uint64) uint64 {
	return (size/16+1)*16 + 10
}

// mediaUpload uploads one file as MediaUpload says.
func (e *Engine) mediaUpload(c *enginepb.MediaUpload) *enginepb.Reply {
	mediaType, _, ok := mediaTypes(c.Kind)
	switch {
	case !ok:
		return failure(fmt.Sprintf("there's no kind of file %v to upload", c.Kind), false)
	case c.Id == "":
		return failure("an upload needs an id", false)
	case !filepath.IsAbs(c.Path) || !filepath.IsAbs(c.Temp):
		return failure(fmt.Sprintf("%q and %q must be whole paths", c.Path, c.Temp), false)
	case len(c.FileSha256) != sha256.Size:
		return failure("a file to upload needs its SHA-256, which the upload checks it against", false)
	}
	source, err := os.Open(c.Path)
	if err != nil {
		return failure(fmt.Sprintf("can't read the file to upload: %v", err), !errors.Is(err, os.ErrNotExist))
	}
	defer source.Close()
	info, err := source.Stat()
	if err != nil {
		return failure(fmt.Sprintf("can't read the file to upload: %v", err), true)
	}
	size := encryptedSize(uint64(info.Size()))
	if err := room(filepath.Dir(c.Temp), size); err != nil {
		return uploaded(enginepb.MediaUploaded_NO_SPACE, err.Error())
	}
	ctx, cancel := context.WithCancel(e.ctx)
	defer cancel()
	if !e.media.start(c.Id, cancel) {
		return failure(fmt.Sprintf("upload %s is already running", c.Id), true)
	}
	defer e.media.finish(c.Id)
	part := fmt.Sprintf("%s.%d-%d.part", c.Temp, os.Getpid(), e.media.next.Add(1))
	f, err := os.OpenFile(part, os.O_RDWR|os.O_CREATE|os.O_EXCL, 0o600)
	if err != nil {
		return uploadFailed(err)
	}
	defer func() {
		_ = f.Close()
		_ = os.Remove(part)
	}()
	plain := sha256.New()
	temp := &encrypted{f: f, plain: plain, want: c.FileSha256, report: func(n uint64) { e.progress(c.Id, n, size) }}
	resp, err := e.uploader().UploadReader(ctx, io.TeeReader(source, plain), temp, mediaType)
	if err != nil {
		return uploadFailed(err)
	}
	if subtle.ConstantTimeCompare(resp.FileSHA256, c.FileSha256) != 1 {
		return uploaded(enginepb.MediaUploaded_CHANGED, errChanged.Error())
	}
	e.progress(c.Id, size, size)
	return &enginepb.Reply{Kind: &enginepb.Reply_MediaUploaded{MediaUploaded: &enginepb.MediaUploaded{
		State:         enginepb.MediaUploaded_UPLOADED,
		Url:           resp.URL,
		DirectPath:    resp.DirectPath,
		MediaKey:      resp.MediaKey,
		FileSha256:    resp.FileSHA256,
		FileEncSha256: resp.FileEncSHA256,
		FileLength:    resp.FileLength,
	}}}
}

// uploadFailed is what jawd is told of an upload that didn't go.
func uploadFailed(err error) *enginepb.Reply {
	switch {
	case errors.Is(err, context.Canceled):
		return uploaded(enginepb.MediaUploaded_CANCELLED, "the upload was cancelled")
	case errors.Is(err, errChanged):
		return uploaded(enginepb.MediaUploaded_CHANGED, errChanged.Error())
	case errors.Is(err, syscall.ENOSPC):
		return uploaded(enginepb.MediaUploaded_NO_SPACE, fmt.Sprintf("the disk is full: %v", err))
	}
	if code := uploadStatus(err); code != 0 {
		return failure(fmt.Sprintf("WhatsApp's media server refused the file with %d", code), transient(code))
	}
	return failure(fmt.Sprintf("can't upload the file: %v", err), true)
}

// uploadStatus is the HTTP status in whatsmeow's "upload failed with status code N", or 0.
func uploadStatus(err error) int {
	const said = "upload failed with status code "
	text := err.Error()
	at := strings.LastIndex(text, said)
	if at < 0 {
		return 0
	}
	code, _ := strconv.Atoi(strings.Fields(text[at+len(said):] + " ")[0])
	return code
}

// encrypted is the partial file whatsmeow encrypts the file into and then uploads from. Once the
// encryption is done it checks the plaintext was the file jawd named, before any of it goes, and
// then says how much of it has gone, at most every progressEvery.
type encrypted struct {
	f      *os.File
	plain  hash.Hash
	want   []byte
	sent   uint64
	last   time.Time
	report func(uint64)
}

func (e *encrypted) Write(p []byte) (int, error) { return e.f.Write(p) }

// Seek back to the start is whatsmeow's turn from encrypting to uploading.
func (e *encrypted) Seek(offset int64, whence int) (int64, error) {
	if offset == 0 && whence == io.SeekStart && subtle.ConstantTimeCompare(e.plain.Sum(nil), e.want) != 1 {
		return 0, errChanged
	}
	return e.f.Seek(offset, whence)
}

func (e *encrypted) Read(p []byte) (int, error) {
	n, err := e.f.Read(p)
	e.sent += uint64(n)
	if now := time.Now(); now.Sub(e.last) >= progressEvery {
		e.last = now
		e.report(e.sent)
	}
	return n, err
}
