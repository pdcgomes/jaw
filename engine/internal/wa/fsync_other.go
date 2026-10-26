//go:build !darwin

package wa

import "os"

// fullSync makes what's written to f reach the disk.
func fullSync(f *os.File) error {
	return f.Sync()
}

// barrierSync makes what's written to f reach the disk, before anything written after it: without
// a lighter barrier than fsync, it's a full one.
func barrierSync(f *os.File) error {
	return f.Sync()
}
