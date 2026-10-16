package wa

import (
	"testing"

	"go.mau.fi/whatsmeow/store"
)

func TestConfigure(t *testing.T) {
	Configure([3]uint32{0, 1, 0})
	if got := store.DeviceProps.GetOs(); got != "jaw" {
		t.Errorf("the phone would list %q, not jaw", got)
	}
	if !store.DeviceProps.GetRequireFullSync() {
		t.Error("linking wouldn't ask for the full history")
	}
	v := store.DeviceProps.GetVersion()
	if v.GetPrimary() != 0 || v.GetSecondary() != 1 || v.GetTertiary() != 0 {
		t.Errorf("version %d.%d.%d, not 0.1.0", v.GetPrimary(), v.GetSecondary(), v.GetTertiary())
	}
}
