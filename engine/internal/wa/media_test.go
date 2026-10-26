package wa

import (
	"context"
	"crypto/sha256"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"syscall"
	"testing"
	"time"

	"go.mau.fi/whatsmeow"
	"go.mau.fi/whatsmeow/proto/waMmsRetry"
	"go.mau.fi/whatsmeow/types/events"
	"go.mau.fi/whatsmeow/util/gcmutil"
	"go.mau.fi/whatsmeow/util/hkdfutil"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// fakeDownloader stands in for whatsmeow's client: it writes `data` into the file a piece at a
// time, as a download does, or fails with `err`. With `hold`, it waits after the first piece
// until hold is closed or the download is cancelled.
type fakeDownloader struct {
	data []byte
	err  error
	hold chan struct{}
	// What it was asked, for the tests that check it.
	mu    sync.Mutex
	asked []string
}

func (f *fakeDownloader) DownloadMediaWithPathToFile(ctx context.Context, directPath string, encFileHash, fileHash,
	mediaKey []byte, mediaType whatsmeow.MediaType, mmsType string, allowNoHash bool, file whatsmeow.File) error {
	f.mu.Lock()
	f.asked = append(f.asked, fmt.Sprintf("%s %s %s", directPath, mediaType, mmsType))
	f.mu.Unlock()
	if f.err != nil {
		return f.err
	}
	const piece = 1 << 10
	for at := 0; at < len(f.data); at += piece {
		end := min(at+piece, len(f.data))
		if _, err := file.Write(f.data[at:end]); err != nil {
			return err
		}
		if at == 0 && f.hold != nil {
			select {
			case <-ctx.Done():
				return ctx.Err()
			case <-f.hold:
			}
		}
	}
	return nil
}

func photo(n int) []byte {
	data := make([]byte, n)
	for i := range data {
		data[i] = byte(i * 7)
	}
	return data
}

func downloadOf(data []byte, path string) *enginepb.MediaDownload {
	sum := sha256.Sum256(data)
	return &enginepb.MediaDownload{
		Id:            fmt.Sprintf("%x", sum),
		Kind:          enginepb.MediaDownload_IMAGE,
		DirectPath:    "/v/t62.7118-24/photo.enc?ccb=11-4",
		MediaKey:      make([]byte, 32),
		FileSha256:    sum[:],
		FileEncSha256: make([]byte, 32),
		Size:          uint64(len(data)),
		Path:          path,
	}
}

// reading reads what the engine sends jawd, for the rest of the test.
func reading(jawd *link.Link) <-chan *enginepb.Frame {
	frames := make(chan *enginepb.Frame, 64)
	go func() {
		defer close(frames)
		for {
			frame, err := jawd.Recv()
			if err != nil {
				return
			}
			frames <- frame
		}
	}()
	return frames
}

// next is the next frame the engine sends, within a few seconds.
func next(t *testing.T, frames <-chan *enginepb.Frame) *enginepb.Frame {
	t.Helper()
	select {
	case frame, ok := <-frames:
		if !ok {
			t.Fatal("the link closed")
		}
		return frame
	case <-time.After(5 * time.Second):
		t.Fatal("the engine sent nothing")
	}
	return nil
}

// answer is a download's answer, a failure written as one.
func answer(reply *enginepb.Reply) *enginepb.MediaFile {
	if failure := reply.GetFailure(); failure != nil {
		return &enginepb.MediaFile{Message: "failure: " + failure.Message}
	}
	return reply.GetMediaFile()
}

// downloaded runs the download on the engine, collecting what it says on the way, and returns its
// answer and the progress it reported.
func downloaded(t *testing.T, e *Engine, frames <-chan *enginepb.Frame, c *enginepb.MediaDownload) (*enginepb.MediaFile, []*enginepb.MediaProgress) {
	t.Helper()
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaDownload(c) }()
	var progress []*enginepb.MediaProgress
	for {
		select {
		case reply := <-answered:
			// Each event was sent before the answer was returned; the last may still be on its way
			// from the link to the channel.
			for {
				select {
				case frame := <-frames:
					progress = append(progress, frame.GetEvent().GetMediaProgress())
					continue
				case <-time.After(100 * time.Millisecond):
				}
				return answer(reply), progress
			}
		case frame := <-frames:
			p := frame.GetEvent().GetMediaProgress()
			if p == nil {
				t.Fatalf("a download sent %v", frame)
			}
			progress = append(progress, p)
		case <-time.After(5 * time.Second):
			t.Fatal("the download didn't answer")
		}
	}
}

// left is what's in dir, by name.
func left(t *testing.T, dir string) []string {
	t.Helper()
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, entry := range entries {
		names = append(names, entry.Name())
	}
	return names
}

func TestAFileIsDownloadedWholeUnderTheNameJawdGaveAndNothingElseIsLeft(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(10_000)
	fake := &fakeDownloader{data: data}
	e.mediaDownloader = fake
	dir := t.TempDir()
	path := filepath.Join(dir, "ab12.jpg")
	file, progress := downloaded(t, e, frames, downloadOf(data, path))
	if file.GetState() != enginepb.MediaFile_PLACED || file.GetBytes() != 10_000 {
		t.Fatalf("the download answered %v", file)
	}
	if got, _ := os.ReadFile(path); string(got) != string(data) {
		t.Fatalf("the file holds %d bytes that aren't the download's", len(got))
	}
	if names := left(t, dir); len(names) != 1 {
		t.Fatalf("the download left %v", names)
	}
	if info, _ := os.Stat(path); info.Mode().Perm() != 0o600 {
		t.Fatalf("the file's mode is %v", info.Mode().Perm())
	}
	if len(progress) < 2 || progress[0].GetBytes() == 0 || progress[len(progress)-1].GetBytes() != 10_000 {
		t.Fatalf("the progress said %v", progress)
	}
	if fake.asked[0] != "/v/t62.7118-24/photo.enc?ccb=11-4 WhatsApp Image Keys image" {
		t.Fatalf("whatsmeow was asked %v", fake.asked)
	}
}

func TestAFileAlreadyThereIsNeverWrittenOverAndCountsAsDownloaded(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(4_000)
	e.mediaDownloader = &fakeDownloader{data: data}
	dir := t.TempDir()
	path := filepath.Join(dir, "ab12.jpg")
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	// A second name for the file, which sees anything written into it.
	other := filepath.Join(dir, "other name")
	if err := os.Link(path, other); err != nil {
		t.Fatal(err)
	}
	file, _ := downloaded(t, e, frames, downloadOf(data, path))
	if file.GetState() != enginepb.MediaFile_EXISTS || file.GetBytes() != 4_000 {
		t.Fatalf("the download answered %v", file)
	}
	if names := left(t, dir); len(names) != 2 {
		t.Fatalf("the download left %v", names)
	}

	// Two downloads to one name: one puts it in place, and the other finds it there.
	both := filepath.Join(dir, "cd34.jpg")
	hold := make(chan struct{})
	slowly := &fakeDownloader{data: data, hold: hold}
	e.mediaDownloader = slowly
	first := make(chan *enginepb.Reply, 1)
	slow := downloadOf(data, both)
	slow.Id = "slow"
	go func() { first <- e.mediaDownload(slow) }()
	// Its first piece's progress says it has started.
	if frame := next(t, frames); frame.GetEvent().GetMediaProgress() == nil {
		t.Fatalf("the first download said %v", frame)
	}
	e.mediaDownloader = &fakeDownloader{data: data}
	second, _ := downloaded(t, e, frames, downloadOf(data, both))
	close(hold)
	firstFile := answer(<-first)
	states := []enginepb.MediaFile_State{second.GetState(), firstFile.GetState()}
	if states[0] != enginepb.MediaFile_PLACED || states[1] != enginepb.MediaFile_EXISTS {
		t.Fatalf("two downloads to one name answered %v", states)
	}
	if got, _ := os.ReadFile(both); string(got) != string(data) {
		t.Fatal("the name holds something else")
	}
	for _, name := range left(t, dir) {
		if strings.HasSuffix(name, ".part") {
			t.Fatalf("a partial file was left: %v", left(t, dir))
		}
	}
}

func TestAFileWhoseHashDoesntMatchIsNeverPutInPlace(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(3_000)
	asked := downloadOf(data, "")
	dir := t.TempDir()
	asked.Path = filepath.Join(dir, "ab12.jpg")
	// What comes isn't what the message names.
	e.mediaDownloader = &fakeDownloader{data: photo(2_999)}
	file, _ := downloaded(t, e, frames, asked)
	if file.GetState() != enginepb.MediaFile_BAD_HASH {
		t.Fatalf("a file with another hash answered %v", file)
	}
	if names := left(t, dir); len(names) != 0 {
		t.Fatalf("a file with another hash left %v", names)
	}
	for _, err := range []error{
		whatsmeow.ErrInvalidMediaSHA256,
		fmt.Errorf("downloading: %w", whatsmeow.ErrInvalidMediaHMAC),
		whatsmeow.ErrInvalidMediaEncSHA256,
	} {
		e.mediaDownloader = &fakeDownloader{err: err}
		if file, _ := downloaded(t, e, frames, asked); file.GetState() != enginepb.MediaFile_BAD_HASH {
			t.Errorf("%v answered %v", err, file)
		}
	}
}

func TestADownloadSaysWhatHappened(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	dir := t.TempDir()
	asked := downloadOf(photo(100), filepath.Join(dir, "ab12.jpg"))
	for _, c := range []struct {
		err   error
		state enginepb.MediaFile_State
		retry bool
	}{
		{whatsmeow.ErrMediaDownloadFailedWith404, enginepb.MediaFile_GONE, false},
		{fmt.Errorf("failed to download media from last host: %w", whatsmeow.ErrMediaDownloadFailedWith410), enginepb.MediaFile_GONE, false},
		{whatsmeow.ErrMediaDownloadFailedWith403, enginepb.MediaFile_GONE, false},
		{fmt.Errorf("failed to write file: %w", &os.PathError{Op: "write", Path: "x", Err: syscall.ENOSPC}), enginepb.MediaFile_NO_SPACE, false},
		{whatsmeow.DownloadHTTPError{Response: &http.Response{StatusCode: http.StatusServiceUnavailable}}, enginepb.MediaFile_STATE_UNSPECIFIED, true},
	} {
		e.mediaDownloader = &fakeDownloader{err: c.err}
		file, _ := downloaded(t, e, frames, asked)
		if file.GetState() != c.state {
			t.Errorf("%v answered %v", c.err, file)
		}
		if c.retry && !strings.HasPrefix(file.GetMessage(), "failure: can't download the file") {
			t.Errorf("%v, which may pass, answered %v", c.err, file)
		}
		if names := left(t, dir); len(names) != 0 {
			t.Errorf("%v left %v", c.err, names)
		}
	}
	huge := downloadOf(photo(10), filepath.Join(dir, "huge.mp4"))
	huge.Size = 1 << 60
	fake := &fakeDownloader{data: photo(10)}
	e.mediaDownloader = fake
	if file, _ := downloaded(t, e, frames, huge); file.GetState() != enginepb.MediaFile_NO_SPACE || len(fake.asked) != 0 {
		t.Errorf("a file with no room for it answered %v, after asking for it %d times", file, len(fake.asked))
	}
}

func TestACancelledDownloadLeavesNothing(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(5_000)
	e.mediaDownloader = &fakeDownloader{data: data, hold: make(chan struct{})}
	dir := t.TempDir()
	asked := downloadOf(data, filepath.Join(dir, "ab12.jpg"))
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaDownload(asked) }()
	if frame := next(t, frames); frame.GetEvent().GetMediaProgress() == nil {
		t.Fatalf("the download said %v", frame)
	}
	command(t, jawd, 7, &enginepb.Command{Kind: &enginepb.Command_MediaCancel{MediaCancel: &enginepb.MediaCancel{Id: asked.Id}}})
	if frame := next(t, frames); frame.Id != 7 || frame.GetReply().GetDone() == nil {
		t.Fatalf("the cancel answered %v", frame)
	}
	select {
	case reply := <-answered:
		if reply.GetMediaFile().GetState() != enginepb.MediaFile_CANCELLED {
			t.Fatalf("the cancelled download answered %v", reply)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("the cancelled download didn't answer")
	}
	if names := left(t, dir); len(names) != 0 {
		t.Fatalf("a cancelled download left %v", names)
	}
}

func TestDownloadsAreSyncedAndPutInPlaceOneAtATime(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(2_000)
	e.mediaDownloader = &fakeDownloader{data: data}
	dir := t.TempDir()
	asked := downloadOf(data, filepath.Join(dir, "ab12.jpg"))
	// Another download is syncing its file.
	e.media.finishing.Lock()
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaDownload(asked) }()
	next(t, frames)
	select {
	case reply := <-answered:
		t.Fatalf("a download was put in place while another was being synced: %v", reply)
	case <-time.After(200 * time.Millisecond):
	}
	if names := left(t, dir); len(names) != 1 || !strings.HasSuffix(names[0], ".part") {
		t.Fatalf("while it waits, the folder holds %v", names)
	}
	e.media.finishing.Unlock()
	if reply := <-answered; reply.GetMediaFile().GetState() != enginepb.MediaFile_PLACED {
		t.Fatalf("once the other was done, the download answered %v", reply)
	}
}

func TestAFileIsSentToTheDriveBeforeItsLinkedInAndItsFolderAfter(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(3_000)
	e.mediaDownloader = &fakeDownloader{data: data}
	dir := t.TempDir()
	path := filepath.Join(dir, "ab12.jpg")
	there := func(p string) bool {
		_, err := os.Lstat(p)
		return err == nil
	}
	partials := func() int {
		n := 0
		for _, name := range left(t, dir) {
			if strings.HasSuffix(name, ".part") {
				n++
			}
		}
		return n
	}
	var order []string
	e.media.syncFile = func(f *os.File) error {
		order = append(order, fmt.Sprintf("file: named %v, partial %v", there(path), there(f.Name())))
		return barrierSync(f)
	}
	e.media.syncFolder = func(folder string) error {
		order = append(order, fmt.Sprintf("folder %v: named %v, partials %d", folder == dir, there(path), partials()))
		return barrierSyncDir(folder)
	}
	file, _ := downloaded(t, e, frames, downloadOf(data, path))
	if file.GetState() != enginepb.MediaFile_PLACED {
		t.Fatalf("the download answered %v", file)
	}
	want := []string{"file: named false, partial true", "folder true: named true, partials 0"}
	if fmt.Sprint(order) != fmt.Sprint(want) {
		t.Fatalf("sent to the drive as %q, not %q", order, want)
	}
}

func TestAPartialFileIsNamedAsJawdsStartRemovesThem(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	frames := reading(jawd)
	data := photo(5_000)
	hold := make(chan struct{})
	e.mediaDownloader = &fakeDownloader{data: data, hold: hold}
	dir := t.TempDir()
	asked := downloadOf(data, filepath.Join(dir, "ab12.jpg"))
	answered := make(chan *enginepb.Reply, 1)
	go func() { answered <- e.mediaDownload(asked) }()
	next(t, frames)
	names := left(t, dir)
	// jaw-media's remove_partial takes `<name>.<pid>-<n>.part`.
	if len(names) != 1 || !regexp.MustCompile(`^ab12\.jpg\.[0-9]+-[0-9]+\.part$`).MatchString(names[0]) {
		t.Fatalf("while downloading, the folder holds %v", names)
	}
	close(hold)
	if reply := <-answered; reply.GetMediaFile().GetState() != enginepb.MediaFile_PLACED {
		t.Fatalf("the download answered %v", reply)
	}
}

// retryAnswer is the phone's answer to a media retry as WhatsApp sends it, encrypted with the key.
func retryAnswer(t *testing.T, id string, key []byte, notification *waMmsRetry.MediaRetryNotification) *events.MediaRetry {
	t.Helper()
	plaintext, err := proto.Marshal(notification)
	if err != nil {
		t.Fatal(err)
	}
	iv := make([]byte, 12)
	cipherKey := hkdfutil.SHA256(key, nil, []byte("WhatsApp Media Retry Notification"), 32)
	ciphertext, err := gcmutil.Encrypt(cipherKey, iv, plaintext, []byte(id))
	if err != nil {
		t.Fatal(err)
	}
	return &events.MediaRetry{
		Ciphertext: ciphertext,
		IV:         iv,
		MessageID:  id,
		ChatID:     mustJID(t, "120363000000000002@g.us"),
		SenderID:   mustJID(t, "351910000002@s.whatsapp.net"),
		Timestamp:  time.UnixMilli(1_792_000_000_000),
	}
}

func TestThePhonesAnswerIsReadWithTheKeyItWasAskedWithAndKeptBeforeItsAcked(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	key := make([]byte, 32)
	key[0] = 9
	e.media.remember("3EB0AA", key)
	uploaded := &waMmsRetry.MediaRetryNotification{
		StanzaID:   proto.String("3EB0AA"),
		DirectPath: proto.String("/v/t62.7118-24/again.enc"),
		Result:     waMmsRetry.MediaRetryNotification_SUCCESS.Enum(),
	}
	event, waited := handled(t, e, jawd, retryAnswer(t, "3EB0AA", key, uploaded))
	answer := event.GetMediaRetried()
	if !waited || answer.GetResult() != enginepb.MediaRetried_SUCCESS || answer.GetDirectPath() != "/v/t62.7118-24/again.enc" ||
		answer.GetChat() != "120363000000000002@g.us" || answer.GetSender() != "351910000002@s.whatsapp.net" ||
		answer.GetTimestampMs() != 1_792_000_000_000 {
		t.Fatalf("the phone's answer came as %v, waited %v", answer, waited)
	}

	missing := &waMmsRetry.MediaRetryNotification{Result: waMmsRetry.MediaRetryNotification_NOT_FOUND.Enum()}
	event, _ = handled(t, e, jawd, retryAnswer(t, "3EB0AA", key, missing))
	if got := event.GetMediaRetried().GetResult(); got != enginepb.MediaRetried_NOT_FOUND {
		t.Fatalf("a file the phone hasn't came as %v", got)
	}
	notOnPhone := &events.MediaRetry{MessageID: "3EB0AA", ChatID: mustJID(t, "351910000002@s.whatsapp.net"), Error: &events.MediaRetryError{Code: 2}}
	event, _ = handled(t, e, jawd, notOnPhone)
	if got := event.GetMediaRetried().GetResult(); got != enginepb.MediaRetried_NOT_FOUND {
		t.Fatalf("error code 2 came as %v", got)
	}
	// An engine started again since it asked has no key to read the answer with.
	event, _ = handled(t, e, jawd, retryAnswer(t, "3EB0BB", key, uploaded))
	if got := event.GetMediaRetried(); got.GetResult() != enginepb.MediaRetried_UNREADABLE || got.GetDirectPath() != "" {
		t.Fatalf("an answer without its key came as %v", got)
	}
}

func TestTheKeysKeptForThePhonesAnswersAreBounded(t *testing.T) {
	var m media
	for n := range retryKeysKept + 10 {
		m.remember(fmt.Sprintf("m%d", n), []byte{byte(n)})
	}
	if len(m.keys) != retryKeysKept {
		t.Fatalf("%d keys are kept", len(m.keys))
	}
	if m.key(fmt.Sprintf("m%d", retryKeysKept+9)) == nil {
		t.Fatal("the newest key went")
	}
}

func TestMediaCommandsThatNeedALinkedAccountSaySo(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	for n, cmd := range []*enginepb.Command{
		{Kind: &enginepb.Command_MediaDownload{MediaDownload: downloadOf(photo(10), "/tmp/x.jpg")}},
		{Kind: &enginepb.Command_MediaRetry{MediaRetry: &enginepb.MediaRetry{Chat: "351910000002@s.whatsapp.net", Id: "3EB0AA"}}},
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
