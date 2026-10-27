package wa

import (
	"errors"
	"fmt"
	"time"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// appStatePatch sends one app-state mutation jawd built, such as a chat pinned or a message deleted
// for you, which your other devices then apply. whatsmeow fetches the collection again afterwards,
// so the mutation comes back as an AppState event, which changes nothing jawd hasn't done already.
func (e *Engine) appStatePatch(c *enginepb.AppStatePatch) *enginepb.Reply {
	patch, err := patchInfo(c)
	if err != nil {
		return failure(err.Error(), false)
	}
	if err := e.cli().SendAppState(e.ctx, patch); err != nil {
		return failure(fmt.Sprintf("can't change %s for your other devices: %v", c.Index[0], err), retryable(err))
	}
	return done()
}

// patchInfo is the patch whatsmeow sends for jawd's mutation. whatsmeow writes the patch's time into
// every mutation it encodes, so the patch takes the time jawd wrote, which a pinned chat's row holds.
func patchInfo(c *enginepb.AppStatePatch) (appstate.PatchInfo, error) {
	switch appstate.WAPatchName(c.Name) {
	case appstate.WAPatchRegular, appstate.WAPatchRegularLow, appstate.WAPatchRegularHigh,
		appstate.WAPatchCriticalBlock, appstate.WAPatchCriticalUnblockLow:
	default:
		return appstate.PatchInfo{}, fmt.Errorf("there's no app-state collection %q", c.Name)
	}
	if len(c.Index) == 0 {
		return appstate.PatchInfo{}, errors.New("an app-state mutation needs its index")
	}
	value := &waSyncAction.SyncActionValue{}
	if err := proto.Unmarshal(c.Value, value); err != nil {
		return appstate.PatchInfo{}, fmt.Errorf("that isn't an app-state value: %v", err)
	}
	patch := appstate.PatchInfo{
		Type:      appstate.WAPatchName(c.Name),
		Mutations: []appstate.MutationInfo{{Index: c.Index, Version: c.Version, Value: value}},
	}
	if value.GetTimestamp() > 0 {
		patch.Timestamp = time.UnixMilli(value.GetTimestamp())
	}
	return patch, nil
}
