package wa

import (
	"os"

	"golang.org/x/sys/unix"
)

// fullSync makes what's written to f reach the disk. On macOS fsync stops at the drive's cache;
// F_FULLFSYNC asks the drive to write it out.
func fullSync(f *os.File) error {
	if _, err := unix.FcntlInt(f.Fd(), unix.F_FULLFSYNC, 0); err == nil {
		return nil
	}
	return f.Sync()
}
