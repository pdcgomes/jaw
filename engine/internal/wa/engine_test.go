package wa

import (
	"context"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// engineWithoutSession is an engine on one end of a pair of pipes, with no whatsmeow session, and
// jawd's end of the link, after both have said hello. jawd's hello carries `protocol`.
func engineWithoutSession(t *testing.T, protocol uint32) (*Engine, *link.Link, chan error, func()) {
	t.Helper()
	toEngine, fromJawd := io.Pipe()
	toJawd, fromEngine := io.Pipe()
	e := newEngine(context.Background(), link.New(toEngine, fromEngine), NewLogger("test", "error"))
	jawd := link.New(toJawd, fromJawd)
	greeted := make(chan error, 1)
	go func() { greeted <- e.hello("jaw-engine test", "v-test") }()
	first, err := jawd.Recv()
	if err != nil || first.GetHello() == nil {
		t.Fatalf("the engine's first frame: %v, %v", first, err)
	}
	if got := first.GetHello().Protocol; got != link.Protocol {
		t.Fatalf("the engine says protocol %d", got)
	}
	if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: &enginepb.Hello{Protocol: protocol, Program: "jawd test"}}}); err != nil {
		t.Fatal(err)
	}
	served := make(chan error, 1)
	if err := <-greeted; err != nil {
		served <- err
		return e, jawd, served, func() {}
	}
	go func() { served <- e.serve() }()
	return e, jawd, served, func() { _ = fromJawd.Close() }
}

func command(t *testing.T, jawd *link.Link, id uint64, cmd *enginepb.Command) {
	t.Helper()
	if err := jawd.Send(&enginepb.Frame{Id: id, Body: &enginepb.Frame_Command{Command: cmd}}); err != nil {
		t.Fatal(err)
	}
}

func TestHelloThenPing(t *testing.T) {
	_, jawd, served, closeLink := engineWithoutSession(t, link.Protocol)
	command(t, jawd, 1, &enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}})
	reply, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	if reply.Id != 1 || reply.GetReply().GetPong() == nil {
		t.Fatalf("a ping got %v", reply)
	}
	closeLink()
	if err := <-served; err != nil {
		t.Errorf("serve ended with %v after jawd closed the link", err)
	}
}

func TestAMismatchedProtocolIsRefused(t *testing.T) {
	_, _, served, _ := engineWithoutSession(t, link.Protocol+1)
	err := <-served
	if err == nil || !strings.Contains(err.Error(), "link protocol") {
		t.Fatalf("a mismatched hello gave %v", err)
	}
}

func TestEachBenchEventWaitsForItsAck(t *testing.T) {
	e, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	command(t, jawd, 9, &enginepb.Command{Kind: &enginepb.Command_BenchRun{BenchRun: &enginepb.BenchRun{Count: 50, Size: 100}}})
	var acked uint64
	for {
		frame, err := jawd.Recv()
		if err != nil {
			t.Fatal(err)
		}
		if result := frame.GetReply().GetBenchResult(); result != nil {
			if frame.Id != 9 || result.Count != 50 || acked != 50 {
				t.Fatalf("the bench's result %v, id %d, after %d acks", result, frame.Id, acked)
			}
			return
		}
		event := frame.GetEvent()
		if event.GetBench() == nil || len(event.GetBench().Payload) != 100 {
			t.Fatalf("a bench sent %v", frame)
		}
		if waiting := e.acks.Waiting(); waiting != 1 {
			t.Fatalf("%d events waiting for an ack; a bench sends the next only after the last's", waiting)
		}
		if event.Seq != acked+1 {
			t.Fatalf("seq %d after %d acks", event.Seq, acked)
		}
		acked = event.Seq
		if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Ack{Ack: &enginepb.Ack{Seq: event.Seq}}}); err != nil {
			t.Fatal(err)
		}
	}
}

func TestCommandsThatNeedASessionSaySo(t *testing.T) {
	_, jawd, _, closeLink := engineWithoutSession(t, link.Protocol)
	defer closeLink()
	command(t, jawd, 3, &enginepb.Command{Kind: &enginepb.Command_LinkStart{LinkStart: &enginepb.LinkStart{}}})
	reply, err := jawd.Recv()
	if err != nil {
		t.Fatal(err)
	}
	if failure := reply.GetReply().GetFailure(); reply.Id != 3 || failure == nil || !strings.Contains(failure.Message, "no session") {
		t.Fatalf("linking without a session gave %v", reply)
	}
}

func TestJawdGoingReleasesWhatWaits(t *testing.T) {
	e, jawd, served, closeLink := engineWithoutSession(t, link.Protocol)
	command(t, jawd, 4, &enginepb.Command{Kind: &enginepb.Command_BenchRun{BenchRun: &enginepb.BenchRun{Count: 10, Size: 1}}})
	if frame, err := jawd.Recv(); err != nil || frame.GetEvent().GetBench() == nil {
		t.Fatalf("the first bench event: %v, %v", frame, err)
	}
	closeLink()
	select {
	case <-e.acks.Gone():
	case <-time.After(2 * time.Second):
		t.Fatal("the engine didn't notice jawd had gone")
	}
	if err := <-served; err != nil {
		t.Errorf("serve ended with %v", err)
	}
}
