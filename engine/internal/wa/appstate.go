package wa

import (
	"fmt"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// appStatePatch sends one app-state mutation jawd built, such as a message deleted for you, which
// your other devices then apply. whatsmeow fetches the collection again afterwards, so the mutation
// comes back as an AppState event, which changes nothing jawd hasn't done already.
func (e *Engine) appStatePatch(c *enginepb.AppStatePatch) *enginepb.Reply {
	switch appstate.WAPatchName(c.Name) {
	case appstate.WAPatchRegular, appstate.WAPatchRegularLow, appstate.WAPatchRegularHigh,
		appstate.WAPatchCriticalBlock, appstate.WAPatchCriticalUnblockLow:
	default:
		return failure(fmt.Sprintf("there's no app-state collection %q", c.Name), false)
	}
	if len(c.Index) == 0 {
		return failure("an app-state mutation needs its index", false)
	}
	value := &waSyncAction.SyncActionValue{}
	if err := proto.Unmarshal(c.Value, value); err != nil {
		return failure(fmt.Sprintf("that isn't an app-state value: %v", err), false)
	}
	patch := appstate.PatchInfo{
		Type:      appstate.WAPatchName(c.Name),
		Mutations: []appstate.MutationInfo{{Index: c.Index, Version: c.Version, Value: value}},
	}
	if err := e.client.SendAppState(e.ctx, patch); err != nil {
		return failure(fmt.Sprintf("can't change %s for your other devices: %v", c.Index[0], err), retryable(err))
	}
	return done()
}
