// Package wa is jaw-engine's use of whatsmeow: what it tells WhatsApp, the events it forwards to
// jawd, and the commands it carries out for it.
package wa

import (
	"go.mau.fi/whatsmeow/store"
	"google.golang.org/protobuf/proto"
)

// Configure sets what whatsmeow tells WhatsApp about this device, and must run before the first
// connection: the phone lists it as "jaw" under Linked devices, and linking asks for the full
// history rather than the recent months.
func Configure(version [3]uint32) {
	store.SetOSInfo("jaw", version)
	store.DeviceProps.RequireFullSync = proto.Bool(true)
}
