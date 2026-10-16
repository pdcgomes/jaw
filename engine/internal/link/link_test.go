package link

import (
	"bytes"
	"encoding/binary"
	"errors"
	"io"
	"testing"
	"time"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
	"google.golang.org/protobuf/proto"
)

func TestFramesRoundTrip(t *testing.T) {
	var wire bytes.Buffer
	l := New(&wire, &wire)
	sent := []*enginepb.Frame{
		{Body: &enginepb.Frame_Hello{Hello: &enginepb.Hello{Protocol: Protocol, Program: "jaw-engine test"}}},
		{Id: 7, Body: &enginepb.Frame_Reply{Reply: &enginepb.Reply{Kind: &enginepb.Reply_Pong{Pong: &enginepb.Pong{AtMs: 1}}}}},
		{Body: &enginepb.Frame_Event{Event: &enginepb.Event{Seq: 3, Kind: &enginepb.Event_Bench{Bench: &enginepb.Bench{Payload: bytes.Repeat([]byte{1}, 70_000)}}}}},
	}
	for _, f := range sent {
		if err := l.Send(f); err != nil {
			t.Fatal(err)
		}
	}
	for i, want := range sent {
		got, err := l.Recv()
		if err != nil {
			t.Fatalf("frame %d: %v", i, err)
		}
		if !proto.Equal(got, want) {
			t.Errorf("frame %d came back as %v", i, got)
		}
	}
	if _, err := l.Recv(); !errors.Is(err, io.EOF) {
		t.Errorf("after the last frame: %v, not EOF", err)
	}
}

func TestATruncatedFrameIsAnError(t *testing.T) {
	var wire bytes.Buffer
	_ = binary.Write(&wire, binary.BigEndian, uint32(10))
	wire.Write([]byte{1, 2, 3})
	if _, err := New(&wire, io.Discard).Recv(); !errors.Is(err, io.ErrUnexpectedEOF) {
		t.Errorf("a frame cut short: %v, not ErrUnexpectedEOF", err)
	}
}

func TestAnOversizedFrameIsRefused(t *testing.T) {
	var wire bytes.Buffer
	_ = binary.Write(&wire, binary.BigEndian, uint32(MaxFrame+1))
	if _, err := New(&wire, io.Discard).Recv(); err == nil {
		t.Error("a frame over MaxFrame was read")
	}
}

func TestAcksReleaseTheirWaiter(t *testing.T) {
	acks := NewAcks()
	first, firstDone := acks.Next()
	second, secondDone := acks.Next()
	if second != first+1 {
		t.Fatalf("seqs %d then %d", first, second)
	}
	acks.Acked(second)
	select {
	case <-secondDone:
	case <-time.After(time.Second):
		t.Fatal("the second wasn't released")
	}
	select {
	case <-firstDone:
		t.Fatal("the first was released without its ack")
	default:
	}
	if acks.Waiting() != 1 {
		t.Errorf("%d waiting, not 1", acks.Waiting())
	}
	acks.Acked(first)
	acks.Acked(first)
	<-firstDone
	acks.Close()
	acks.Close()
	<-acks.Gone()
}
