package main

import (
	"bufio"
	"bytes"
	"errors"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// TestMain lets the test binary stand in for jaw-engine, and for a jawd that starts it.
func TestMain(m *testing.M) {
	switch os.Getenv("JAW_ENGINE_TEST_AS") {
	case "engine":
		main()
	case "jawd":
		standIn()
	}
	os.Exit(m.Run())
}

// standIn is jawd as far as the engine can tell: it starts the engine on JAW_ENGINE_TEST_HOME,
// says hello, and once the engine serves, prints each PID it started, the engine's last, and waits
// to be killed. With JAW_ENGINE_TEST_HELD set, a process it starts also holds the other end of the
// engine's standard input, as one jawd starts at the wrong moment can.
func standIn() {
	stdin, ours, err := os.Pipe()
	if err != nil {
		panic(err)
	}
	engine := exec.Command(os.Args[0], "--home", os.Getenv("JAW_ENGINE_TEST_HOME"))
	// The race detector's second at exit isn't the engine's.
	engine.Env = append(os.Environ(), "JAW_ENGINE_TEST_AS=engine", "GORACE=atexit_sleep_ms=0")
	engine.Stdin = stdin
	stdout, err := engine.StdoutPipe()
	if err != nil {
		panic(err)
	}
	engine.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	if err := engine.Start(); err != nil {
		panic(err)
	}
	_ = stdin.Close()
	jawd := link.New(stdout, ours)
	if _, err := jawd.Recv(); err != nil {
		panic(err)
	}
	hello := &enginepb.Hello{Protocol: link.Protocol, Program: "a stand-in for jawd"}
	if err := jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: hello}}); err != nil {
		panic(err)
	}
	if _, err := jawd.Recv(); err != nil {
		panic(err)
	}
	if os.Getenv("JAW_ENGINE_TEST_HELD") != "" {
		holder := exec.Command("sleep", "30")
		holder.ExtraFiles = []*os.File{ours}
		if err := holder.Start(); err != nil {
			panic(err)
		}
		_, _ = os.Stdout.WriteString("holder " + strconv.Itoa(holder.Process.Pid) + "\n")
	}
	_, _ = os.Stdout.WriteString("engine " + strconv.Itoa(engine.Process.Pid) + "\n")
	time.Sleep(time.Hour)
}

func TestTheEngineExitsOnItsOwnSoonAfterJawdIsKilled(t *testing.T) {
	for _, held := range []bool{false, true} {
		name := "its standard input closing then"
		if held {
			name = "its standard input held open by another"
		}
		t.Run(name, func(t *testing.T) {
			jawd := exec.Command(os.Args[0])
			jawd.Env = append(os.Environ(), "JAW_ENGINE_TEST_AS=jawd", "JAW_ENGINE_TEST_HOME="+t.TempDir())
			if held {
				jawd.Env = append(jawd.Env, "JAW_ENGINE_TEST_HELD=1")
			}
			out, err := jawd.StdoutPipe()
			if err != nil {
				t.Fatal(err)
			}
			var pids []int
			jawd.Stderr = os.Stderr
			if err := jawd.Start(); err != nil {
				t.Fatal(err)
			}
			t.Cleanup(func() {
				_ = jawd.Process.Kill()
				for _, pid := range pids {
					if running(pid) {
						_ = syscall.Kill(pid, syscall.SIGKILL)
					}
				}
			})
			said := bufio.NewScanner(out)
			engine := 0
			for engine == 0 && said.Scan() {
				which, number, _ := strings.Cut(said.Text(), " ")
				pid, err := strconv.Atoi(number)
				if err != nil {
					t.Fatalf("the stand-in said %q", said.Text())
				}
				pids = append(pids, pid)
				if which == "engine" {
					engine = pid
				}
			}
			if engine == 0 {
				t.Fatal("the stand-in never said the engine served")
			}
			if err := jawd.Process.Kill(); err != nil {
				t.Fatal(err)
			}
			_ = jawd.Wait()
			killed := time.Now()
			for running(engine) {
				if time.Since(killed) > 5*time.Second {
					t.Fatalf("the engine was still running %v after jawd was killed", time.Since(killed))
				}
				time.Sleep(10 * time.Millisecond)
			}
			t.Logf("the engine exited %v after jawd was killed", time.Since(killed).Round(time.Millisecond))
		})
	}
}

// anEngine is jaw-engine on a folder, the test binary standing in for it and the test for jawd.
type anEngine struct {
	cmd   *exec.Cmd
	jawd  *link.Link
	stdin io.Closer
	// What it logged, to be read once it has exited.
	said bytes.Buffer
}

// engineOn starts jaw-engine on home, killed should it still run when the test ends, or 30 s from
// now, so that no read of its link waits for good.
func engineOn(t *testing.T, home string) *anEngine {
	t.Helper()
	e := &anEngine{cmd: exec.Command(os.Args[0], "--home", home)}
	// The race detector's second at exit isn't the engine's.
	e.cmd.Env = append(os.Environ(), "JAW_ENGINE_TEST_AS=engine", "GORACE=atexit_sleep_ms=0")
	stdin, err := e.cmd.StdinPipe()
	if err != nil {
		t.Fatal(err)
	}
	stdout, err := e.cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	e.cmd.Stderr = &e.said
	if err := e.cmd.Start(); err != nil {
		t.Fatal(err)
	}
	e.jawd, e.stdin = link.New(stdout, stdin), stdin
	cut := time.AfterFunc(30*time.Second, func() { _ = e.cmd.Process.Kill() })
	t.Cleanup(func() {
		cut.Stop()
		_ = e.cmd.Process.Kill()
		_ = e.cmd.Wait()
	})
	return e
}

// serving says hello to the engine as jawd does, and waits until it serves with its session open,
// which it says by saying it's unlinked, as it is on a folder of its own.
func (e *anEngine) serving(t *testing.T) {
	t.Helper()
	if frame, err := e.jawd.Recv(); err != nil || frame.GetHello() == nil {
		t.Fatalf("the engine's hello: %v, %v", frame, err)
	}
	hello := &enginepb.Hello{Protocol: link.Protocol, Program: "jaw-engine's tests"}
	if err := e.jawd.Send(&enginepb.Frame{Body: &enginepb.Frame_Hello{Hello: hello}}); err != nil {
		t.Fatal(err)
	}
	for {
		frame, err := e.jawd.Recv()
		if err != nil {
			t.Fatalf("the engine closed the link before it served: %v", err)
		}
		if frame.GetEvent().GetConnection().GetState() == enginepb.Connection_UNLINKED {
			return
		}
	}
}

// answers has the engine answer a ping.
func (e *anEngine) answers(t *testing.T) {
	t.Helper()
	ping := &enginepb.Command{Kind: &enginepb.Command_Ping{Ping: &enginepb.Ping{}}}
	if err := e.jawd.Send(&enginepb.Frame{Id: 1, Body: &enginepb.Frame_Command{Command: ping}}); err != nil {
		t.Fatal(err)
	}
	for {
		frame, err := e.jawd.Recv()
		if err != nil {
			t.Fatalf("the engine closed the link before it answered: %v", err)
		}
		if frame.GetId() == 1 && frame.GetReply().GetPong() != nil {
			return
		}
	}
}

func TestASecondEngineOnTheSameFolderIsRefusedAndTheFirstRunsOnUntouched(t *testing.T) {
	home := t.TempDir()
	first := engineOn(t, home)
	first.serving(t)
	second := engineOn(t, home)
	if frame, err := second.jawd.Recv(); err == nil {
		t.Fatalf("the second engine said %v", frame)
	}
	var exit *exec.ExitError
	if err := second.cmd.Wait(); !errors.As(err, &exit) || exit.ExitCode() != 1 {
		t.Fatalf("the second engine ended with %v, saying %q", err, second.said.String())
	}
	if said := second.said.String(); !strings.Contains(said, "another jaw-engine is running on "+home) {
		t.Fatalf("the second engine said %q", said)
	}
	first.answers(t)
	_ = first.stdin.Close()
	if err := first.cmd.Wait(); err != nil {
		t.Fatalf("the first engine ended with %v once jawd closed the link, saying %q", err, first.said.String())
	}
}

func TestALockLeftByAnEngineThatDiedIsFreeAgain(t *testing.T) {
	home := t.TempDir()
	first := engineOn(t, home)
	first.serving(t)
	if err := first.cmd.Process.Kill(); err != nil {
		t.Fatal(err)
	}
	_ = first.cmd.Wait()
	// It's the lock that the system let go, not the file.
	if _, err := os.Stat(filepath.Join(home, "session.lock")); err != nil {
		t.Fatal(err)
	}
	second := engineOn(t, home)
	second.serving(t)
	second.answers(t)
}

// running says whether pid is a process that hasn't exited; a zombie, which only waits for its
// parent to see it has, has.
func running(pid int) bool {
	if syscall.Kill(pid, 0) != nil {
		return false
	}
	stat, err := exec.Command("ps", "-o", "stat=", "-p", strconv.Itoa(pid)).Output()
	return err == nil && !strings.HasPrefix(strings.TrimSpace(string(stat)), "Z")
}

func TestTheDeviceIsListedWithTheReleaseItsVersionIsOrFollows(t *testing.T) {
	for v, want := range map[string][3]uint32{
		"0.2.0":             {0, 2, 0},
		"1.12.3-3-g4d2c1e9": {1, 12, 3},
		"32b8eed":           {0, 1, 0},
		"unknown":           {0, 1, 0},
		"1.2":               {0, 1, 0},
	} {
		if got := versionParts(v); got != want {
			t.Errorf("versionParts(%q) = %v, want %v", v, got, want)
		}
	}
}
