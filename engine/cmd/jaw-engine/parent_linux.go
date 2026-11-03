//go:build linux

package main

import (
	"errors"
	"os"

	"golang.org/x/sys/unix"
)

// watchParent calls gone once the process parent has exited, on a goroutine of its own that a
// pidfd wakes only then; at once if it has already.
func watchParent(parent int, gone func()) error {
	fd, err := unix.PidfdOpen(parent, 0)
	if errors.Is(err, unix.ESRCH) {
		gone()
		return nil
	} else if err != nil {
		return err
	}
	// Its children are handed to another parent as it exits, so one that exited before the watch
	// began is seen here.
	if os.Getppid() != parent {
		gone()
		return nil
	}
	go func() {
		exit := []unix.PollFd{{Fd: int32(fd), Events: unix.POLLIN}}
		for {
			n, err := unix.Poll(exit, -1)
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
