package wa

import (
	"context"
	"crypto/sha256"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// fakeUploader stands in for whatsmeow's client as UploadReader works: it encrypts what it reads, a
// piece at a time, into the file it's given, seeks back to its start, and uploads that, a piece at
// a time, or fails with `err`. With `hold`, it waits once the upload has started, until hold is
// closed or the upload is cancelled.
type fakeUploader struct {
	err  error
	hold chan struct{}
	mu   sync.Mutex
	// What it was asked, and what it sent.
	asked []string
	sent  int
	// The temporary file it was given, by name.
	temp string
}

func (f *fakeUploader) UploadReader(ctx context.Context, plaintext io.Reader, tempFile io.ReadWriteSeeker, appInfo whatsmeow.MediaType) (whatsmeow.UploadResponse, error) {
	f.mu.Lock()
	f.asked = append(f.asked, string(appInfo))
	if e, ok := tempFile.(*encrypted); ok {
		f.temp = e.f.Name()
	}
	f.mu.Unlock()
	if f.err != nil {
		return whatsmeow.UploadResponse{}, f.err
	}
	plain := sha256.New()
	piece := make([]byte, 32<<10)
	var length uint64
	for {
		n, err := plaintext.Read(piece)
		plain.Write(piece[:n])
		length += uint64(n)
		for i := range piece[:n] {
			piece[i] ^= 0x5a
		}
		if _, werr := tempFile.Write(piece[:n]); werr != nil {
			return whatsmeow.UploadResponse{}, werr
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return whatsmeow.UploadResponse{}, err
		}
	}
	if _, err := tempFile.Seek(0, io.SeekStart); err != nil {
		return whatsmeow.UploadResponse{}, fmt.Errorf("failed to seek to start of temporary file: %w", err)
	}
	enc := sha256.New()
	for first := true; ; first = false {
		n, err := tempFile.Read(piece)
		enc.Write(piece[:n])
		f.mu.Lock()
		f.sent += n
		f.mu.Unlock()
		if first && f.hold != nil {
			select {
			case <-ctx.Done():
				return whatsmeow.UploadResponse{}, fmt.Errorf("failed to execute request: %w", ctx.Err())
			case <-f.hold:
			}
		}
		if err == io.EOF {
			break
		}
		if err != nil {
			return whatsmeow.UploadResponse{}, err
		}
	}
	return whatsmeow.UploadResponse{
		URL:           "https://mmg.whatsapp.net/v/t62.7118-24/up.enc",
		DirectPath:    "/v/t62.7118-24/up.enc",
		MediaKey:      make([]byte, 32),
		FileSHA256:    plain.Sum(nil),
		FileEncSHA256: enc.Sum(nil),
		FileLength:    length,
	}, nil
}

func uploadOf(t *testing.T, dir string, data []byte) *enginepb.MediaUpload {
	t.Helper()
	path := filepath.Join(dir, "photo.jpg")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(data)
	return &enginepb.MediaUpload{
		Id:         "up:1:sd",
		Kind:       enginepb.MediaDownload_IMAGE,
		Path:       path,
		Temp:       filepath.Join(dir, "uploads", "up-1-sd"),
		FileSha256: sum[:],
	}
}

// uploadedBy runs the upload on the engine, collecting the progress it says on the way.
func uploadedBy(t *testing.T, e *Engine, frames <-chan *enginepb.Frame, c *enginepb.MediaUpload) (*enginepb.Reply, []*enginepb.MediaProgress) {
	t.Helper()
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaUpload(c) }()
	var progress []*enginepb.MediaProgress
	for {
		select {
		case reply := <-answered:
			for {
				select {
				case frame := <-frames:
					progress = append(progress, frame.GetEvent().GetMediaProgress())
					continue
				case <-time.After(100 * time.Millisecond):
				}
				return reply, progress
			}
		case frame := <-frames:
			progress = append(progress, frame.GetEvent().GetMediaProgress())
		case <-time.After(10 * time.Second):
			t.Fatal("the upload didn't answer")
		}
	}
}

func mkdir(t *testing.T, dir string) {
	t.Helper()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		t.Fatal(err)
	}
}

func TestAFileIsUploadedThroughAPartialFileOfItsOwnThatsRemovedAfter(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	fake := &fakeUploader{}
	e.mediaUploader = fake
	dir := t.TempDir()
	data := photo(100_000)
	asked := uploadOf(t, dir, data)
	mkdir(t, filepath.Dir(asked.Temp))
	reply, progress := uploadedBy(t, e, frames, asked)
	up := reply.GetMediaUploaded()
	sum := sha256.Sum256(data)
	if up.GetState() != enginepb.MediaUploaded_UPLOADED || up.GetDirectPath() != "/v/t62.7118-24/up.enc" ||
		up.GetUrl() == "" || len(up.GetMediaKey()) != 32 || string(up.GetFileSha256()) != string(sum[:]) ||
		len(up.GetFileEncSha256()) != 32 || up.GetFileLength() != 100_000 {
		t.Fatalf("the upload answered %v", reply)
	}
	if fake.asked[0] != string(whatsmeow.MediaImage) {
		t.Fatalf("whatsmeow was asked to upload with %v", fake.asked)
	}
	// whatsmeow encrypted into a partial file beside the name jawd gave, which jawd's start removes.
	if !regexp.MustCompile(`/uploads/up-1-sd\.[0-9]+-[0-9]+\.part$`).MatchString(fake.temp) {
		t.Fatalf("whatsmeow encrypted into %q", fake.temp)
	}
	if names := left(t, filepath.Dir(asked.Temp)); len(names) != 0 {
		t.Fatalf("the upload left %v", names)
	}
	size := encryptedSize(100_000)
	if len(progress) < 2 || progress[len(progress)-1].GetBytes() != size || progress[0].GetSize() != size ||
		progress[0].GetId() != "up:1:sd" {
		t.Fatalf("the progress said %v", progress)
	}
}

func TestAFileIsNeverHeldInMemoryOnItsWayUp(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	e.mediaUploader = &fakeUploader{}
	dir := t.TempDir()
	const big = 48 << 20
	asked := uploadOf(t, dir, photo(big))
	mkdir(t, filepath.Dir(asked.Temp))
	runtime.GC()
	var before, after runtime.MemStats
	runtime.ReadMemStats(&before)
	reply, _ := uploadedBy(t, e, frames, asked)
	runtime.ReadMemStats(&after)
	if reply.GetMediaUploaded().GetState() != enginepb.MediaUploaded_UPLOADED {
		t.Fatalf("the upload answered %v", reply)
	}
	// Reading the file whole, or encrypting it in memory, would take more than the file.
	if took := after.TotalAlloc - before.TotalAlloc; took > big/8 {
		t.Fatalf("uploading a %d MB file took %d KB of memory", big>>20, took>>10)
	}
}

func TestAFileThatChangedIsNeverUploaded(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	fake := &fakeUploader{}
	e.mediaUploader = fake
	dir := t.TempDir()
	asked := uploadOf(t, dir, photo(5_000))
	mkdir(t, filepath.Dir(asked.Temp))
	// Something wrote into the file after jawd named it.
	if err := os.WriteFile(asked.Path, photo(4_999), 0o600); err != nil {
		t.Fatal(err)
	}
	reply, _ := uploadedBy(t, e, frames, asked)
	if reply.GetMediaUploaded().GetState() != enginepb.MediaUploaded_CHANGED {
		t.Fatalf("a file that changed answered %v", reply)
	}
	if fake.sent != 0 {
		t.Fatalf("%d bytes of it went", fake.sent)
	}
	if names := left(t, filepath.Dir(asked.Temp)); len(names) != 0 {
		t.Fatalf("the upload left %v", names)
	}
}

func TestACancelledUploadLeavesNothing(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	e.mediaUploader = &fakeUploader{hold: make(chan struct{})}
	dir := t.TempDir()
	asked := uploadOf(t, dir, photo(200_000))
	mkdir(t, filepath.Dir(asked.Temp))
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaUpload(asked) }()
	if frame := next(t, frames); frame.GetEvent().GetMediaProgress() == nil {
		t.Fatalf("the upload said %v", frame)
	}
	command(t, jawd, 9, &enginepb.Command{Kind: &enginepb.Command_MediaCancel{MediaCancel: &enginepb.MediaCancel{Id: asked.Id}}})
	if frame := next(t, frames); frame.Id != 9 || frame.GetReply().GetDone() == nil {
		t.Fatalf("the cancel answered %v", frame)
	}
	select {
	case reply := <-answered:
		if reply.GetMediaUploaded().GetState() != enginepb.MediaUploaded_CANCELLED {
			t.Fatalf("the cancelled upload answered %v", reply)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the cancelled upload didn't answer")
	}
	if names := left(t, filepath.Dir(asked.Temp)); len(names) != 0 {
		t.Fatalf("a cancelled upload left %v", names)
	}
}

func TestAnUploadSaysWhetherTryingAgainMightWork(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	dir := t.TempDir()
	asked := uploadOf(t, dir, photo(1_000))
	mkdir(t, filepath.Dir(asked.Temp))
	for _, c := range []struct {
		err   error
		state enginepb.MediaUploaded_State
		retry bool
		said  string
	}{
		{fmt.Errorf("upload failed with status code 413"), enginepb.MediaUploaded_STATE_UNSPECIFIED, false, "refused the file with 413"},
		{fmt.Errorf("upload failed with status code 503"), enginepb.MediaUploaded_STATE_UNSPECIFIED, true, "refused the file with 503"},
		{fmt.Errorf("failed to refresh media connections: %w", whatsmeow.ErrNotConnected), enginepb.MediaUploaded_STATE_UNSPECIFIED, true, "can't upload"},
		{fmt.Errorf("failed to encrypt file: %w", &os.PathError{Op: "write", Path: "x", Err: syscall.ENOSPC}), enginepb.MediaUploaded_NO_SPACE, false, "full"},
	} {
		e.mediaUploader = &fakeUploader{err: c.err}
		reply, _ := uploadedBy(t, e, frames, asked)
		if got := reply.GetMediaUploaded().GetState(); got != c.state {
			t.Errorf("%v answered %v", c.err, reply)
		}
		failure := reply.GetFailure()
		if c.state == enginepb.MediaUploaded_STATE_UNSPECIFIED && (failure == nil || failure.Retry != c.retry || !strings.Contains(failure.Message, c.said)) {
			t.Errorf("%v answered %v, which should say %q and retry %v", c.err, reply, c.said, c.retry)
		}
		if names := left(t, filepath.Dir(asked.Temp)); len(names) != 0 {
			t.Errorf("%v left %v", c.err, names)
		}
	}
	huge := uploadOf(t, dir, photo(10))
	huge.Temp = filepath.Join(dir, "uploads", "huge")
	room := uint64(0)
	if err := roomFor(filepath.Dir(huge.Temp), &room); err == nil {
		// A file bigger than the disk's free space can't be encrypted beside it.
		if err := os.Truncate(huge.Path, int64(room)+1); err == nil {
			sum := sha256.Sum256(nil)
			huge.FileSha256 = sum[:]
			fake := &fakeUploader{}
			e.mediaUploader = fake
			if reply, _ := uploadedBy(t, e, frames, huge); reply.GetMediaUploaded().GetState() != enginepb.MediaUploaded_NO_SPACE || len(fake.asked) != 0 {
				t.Errorf("a file with no room for it answered %v, after asking whatsmeow %d times", reply, len(fake.asked))
			}
		}
	}
}

// roomFor reads the free space in dir.
func roomFor(dir string, free *uint64) error {
	var stats syscall.Statfs_t
	if err := syscall.Statfs(dir, &stats); err != nil {
		return err
	}
	*free = uint64(stats.Bavail) * uint64(stats.Bsize)
	return nil
}

func TestAnUploadNeedsALinkedAccount(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	// A file that's there and is the one named, so only the missing session stops it.
	up := uploadOf(t, t.TempDir(), photo(1_000))
	mkdir(t, filepath.Dir(up.Temp))
	command(t, jawd, 1, &enginepb.Command{Kind: &enginepb.Command_MediaUpload{MediaUpload: up}})
	reply, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	if failure := reply.GetReply().GetFailure(); failure == nil || !strings.Contains(failure.Message, "no session") {
		t.Fatalf("an upload without a session gave %v", reply)
	}
}
