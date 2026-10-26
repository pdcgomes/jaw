package wa

import (
	"os"

	"golang.org/x/sys/unix"
)

// fcntl is the system's, or a test's that sees what's asked of it.
var fcntl = unix.FcntlInt

// fullSync makes what's written to f reach the disk. On macOS fsync stops at the drive's cache;
// F_FULLFSYNC asks the drive to write it out.
func fullSync(f *os.File) error {
	if _, err := fcntl(f.Fd(), unix.F_FULLFSYNC, 0); err == nil {
		return nil
	}
	return f.Sync()
}

// barrierSync sends what's written to f to the drive, ordered before everything written after it,
// without waiting for the drive to empty its cache: whatever follows, the commit naming the file
// included, can't reach the disk before it, and the next F_FULLFSYNC, by anyone, makes it durable.
// A drive without barriers gets a full flush.
func barrierSync(f *os.File) error {
	if _, err := fcntl(f.Fd(), unix.F_BARRIERFSYNC, 0); err == nil {
		return nil
	}
	return fullSync(f)
}
