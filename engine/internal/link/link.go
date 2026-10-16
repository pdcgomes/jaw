// Package link carries Frames between jaw-engine and jawd on the engine's standard input and
// output: each is its length, four bytes big-endian, then the protobuf.
package link

import (
	"bufio"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"sync"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
	"google.golang.org/protobuf/proto"
)

// MaxFrame is the largest frame either side sends. Big payloads, such as history, travel as files.
const MaxFrame = 16 << 20

// Protocol is the link's version, which both sides' Hello must carry.
const Protocol = uint32(enginepb.Version_VERSION_CURRENT)

// Link reads frames from one side and writes them to the other. Send is safe to call from any
// goroutine; Recv is for one reader.
type Link struct {
	r  *bufio.Reader
	mu sync.Mutex
	w  *bufio.Writer
}

func New(r io.Reader, w io.Writer) *Link {
	return &Link{r: bufio.NewReaderSize(r, 64<<10), w: bufio.NewWriterSize(w, 64<<10)}
}

// Send writes one frame and flushes it, so it leaves at once.
func (l *Link) Send(frame *enginepb.Frame) error {
	data, err := proto.Marshal(frame)
	if err != nil {
		return err
	}
	if len(data) > MaxFrame {
		return fmt.Errorf("a frame of %d bytes is over the link's %d", len(data), MaxFrame)
	}
	var size [4]byte
	binary.BigEndian.PutUint32(size[:], uint32(len(data)))
	l.mu.Lock()
	defer l.mu.Unlock()
	if _, err := l.w.Write(size[:]); err != nil {
		return err
	}
	if _, err := l.w.Write(data); err != nil {
		return err
	}
	return l.w.Flush()
}

// Recv reads the next frame. It returns io.EOF when the other side has closed the link between
// frames, and io.ErrUnexpectedEOF when it closed in the middle of one.
func (l *Link) Recv() (*enginepb.Frame, error) {
	var size [4]byte
	if _, err := io.ReadFull(l.r, size[:]); err != nil {
		return nil, err
	}
	n := binary.BigEndian.Uint32(size[:])
	if n > MaxFrame {
		return nil, fmt.Errorf("a frame of %d bytes is over the link's %d", n, MaxFrame)
	}
	data := make([]byte, n)
	if _, err := io.ReadFull(l.r, data); err != nil {
		if errors.Is(err, io.EOF) {
			return nil, io.ErrUnexpectedEOF
		}
		return nil, err
	}
	frame := &enginepb.Frame{}
	if err := proto.Unmarshal(data, frame); err != nil {
		return nil, fmt.Errorf("a frame that isn't the link's protobuf: %w", err)
	}
	return frame, nil
}
