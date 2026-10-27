package wa

import (
	"errors"
	"fmt"
	"slices"
	"sync"
	"time"

	"go.mau.fi/whatsmeow/appstate"
	"go.mau.fi/whatsmeow/proto/waSyncAction"
	"google.golang.org/protobuf/proto"

	"github.com/pdcgomes/jaw/engine/internal/link/enginepb"
)

// echoWait is how long a patch WhatsApp has taken waits for jawd to keep its echo.
const echoWait = 30 * time.Second

// appStatePatch sends one app-state mutation jawd built, such as a chat pinned or a message deleted
// for you, which your other devices then apply. Once WhatsApp has taken it, whatsmeow fetches the
// collection again and hands the mutation back as an AppState event, from a goroutine of its own
// after SendAppState has returned; the answer waits until jawd has kept that echo, known by its
// index and the patch's time, so the echo is in jawd's inbox before jawd hears the patch went. An
// echo that doesn't come back within echoWait is a failure to try again, as is a fetch after the
// send that fails: app state is the same however often it's sent.
func (e *Engine) appStatePatch(c *enginepb.AppStatePatch) *enginepb.Reply {
	patch, err := patchInfo(c)
	if err != nil {
		return failure(err.Error(), false)
	}
	// The echo is known by the patch's time, which whatsmeow would otherwise pick as it sends.
	if patch.Timestamp.IsZero() {
		patch.Timestamp = time.UnixMilli(time.Now().UnixMilli())
	}
	echo := e.echoes.expect(c.Index, patch.Timestamp.UnixMilli())
	defer e.echoes.forget(echo)
	if err := e.cli().SendAppState(e.ctx, patch); err != nil {
		return failure(fmt.Sprintf("can't change %s for your other devices: %v", c.Index[0], err), retryable(err))
	}
	return e.echoed(echo, c.Index[0], echoWait)
}

// echoed waits until jawd has kept echo, for at most wait, or until jawd has gone.
func (e *Engine) echoed(echo *echo, what string, wait time.Duration) *enginepb.Reply {
	timer := time.NewTimer(wait)
	defer timer.Stop()
	select {
	case <-echo.back:
		return done()
	case <-timer.C:
		return failure(fmt.Sprintf("WhatsApp took the %s change, but its echo didn't come back within %v", what, wait), true)
	case <-e.acks.Gone():
		return failure(fmt.Sprintf("jawd went before the %s change's echo came back", what), true)
	case <-e.ctx.Done():
		return failure(fmt.Sprintf("the engine stopped before the %s change's echo came back", what), true)
	}
}

// echoes are the echoes of app-state patches waiting to be kept, each known by its index and the
// time its value carries, which whatsmeow writes from the patch's.
type echoes struct {
	mu      sync.Mutex
	waiting map[echoKey][]*echo
}

type echoKey struct {
	// The index's strings, each quoted, so no two indexes share one.
	index string
	at    int64
}

// echo is one patch's wait for its echo: back closes once jawd has kept it.
type echo struct {
	key  echoKey
	back chan struct{}
}

func keyOf(index []string, at int64) echoKey {
	return echoKey{index: fmt.Sprintf("%q", index), at: at}
}

// expect starts a wait for the echo of index at at.
func (r *echoes) expect(index []string, at int64) *echo {
	w := &echo{key: keyOf(index, at), back: make(chan struct{})}
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.waiting == nil {
		r.waiting = make(map[echoKey][]*echo)
	}
	r.waiting[w.key] = append(r.waiting[w.key], w)
	return w
}

// kept ends every wait for the echo of state, which jawd has kept: its index at the time its value
// carries.
func (r *echoes) kept(state *enginepb.AppState) {
	value := &waSyncAction.SyncActionValue{}
	if err := proto.Unmarshal(state.Value, value); err != nil {
		return
	}
	key := keyOf(state.Index, value.GetTimestamp())
	r.mu.Lock()
	woken := r.waiting[key]
	delete(r.waiting, key)
	r.mu.Unlock()
	for _, w := range woken {
		close(w.back)
	}
}

// forget drops w's wait, if its echo hasn't ended it.
func (r *echoes) forget(w *echo) {
	r.mu.Lock()
	defer r.mu.Unlock()
	left := slices.DeleteFunc(r.waiting[w.key], func(other *echo) bool { return other == w })
	if len(left) == 0 {
		delete(r.waiting, w.key)
	} else {
		r.waiting[w.key] = left
	}
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
