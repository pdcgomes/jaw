//go:build !darwin

package wa

import "os"

// fullSync makes what's written to f reach the disk.
func fullSync(f *os.File) error {
	return f.Sync()
}
