package wa

import (
	"os"
	"path/filepath"
	"testing"

	"golang.org/x/sys/unix"
)

// A barrier that did nothing would pass every other test, since only a power cut can tell; this
// one sees what each call asks of the drive.
func TestABarrierAsksTheDriveForOneAndAFullFlushOnlyWhereItMust(t *testing.T) {
	f, err := os.Create(filepath.Join(t.TempDir(), "f"))
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	var asked []int
	refusing := false
	fcntl = func(fd uintptr, cmd int, arg int) (int, error) {
		asked = append(asked, cmd)
		if refusing && cmd == unix.F_BARRIERFSYNC {
			return -1, unix.ENOTSUP
		}
		return 0, nil
	}
	defer func() { fcntl = unix.FcntlInt }()
	if err := barrierSync(f); err != nil || len(asked) != 1 || asked[0] != unix.F_BARRIERFSYNC {
		t.Fatalf("a barrier asked %v, %v", asked, err)
	}
	asked, refusing = nil, true
	if err := barrierSync(f); err != nil || len(asked) != 2 || asked[1] != unix.F_FULLFSYNC {
		t.Fatalf("a barrier the drive refuses asked %v, %v", asked, err)
	}
	asked = nil
	if err := fullSync(f); err != nil || len(asked) != 1 || asked[0] != unix.F_FULLFSYNC {
		t.Fatalf("a full flush asked %v, %v", asked, err)
	}
}
