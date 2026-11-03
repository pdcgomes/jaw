//go:build darwin

package main

import (
	"errors"
	"os"

	"golang.org/x/sys/unix"
)

// watchParent calls gone once the process parent has exited, on a goroutine of its own that a
// kqueue wakes only then; at once if it has already.
func watchParent(parent int, gone func()) error {
	kq, err := unix.Kqueue()
	if err != nil {
		return err
	}
	exit := []unix.Kevent_t{{
		Ident:  uint64(parent),
		Filter: unix.EVFILT_PROC,
		Flags:  unix.EV_ADD | unix.EV_ONESHOT,
		Fflags: unix.NOTE_EXIT,
	}}
	if _, err := unix.Kevent(kq, exit, nil, nil); err != nil {
		_ = unix.Close(kq)
		if errors.Is(err, unix.ESRCH) {
			gone()
			return nil
		}
		return err
	}
	// Its children are handed to another parent as it exits, so one that exited before the watch
	// began is seen here.
	if os.Getppid() != parent {
		gone()
		return nil
	}
	go func() {
		events := make([]unix.Kevent_t, 1)
		for {
			n, err := unix.Kevent(kq, nil, events, nil)
			if errors.Is(err, unix.EINTR) {
				continue
			}
			if err == nil && n > 0 {
				gone()
			}
			return
		}
	}()
	return nil
}
