package link

import (
	"sync"
	"sync/atomic"
)

// Acks numbers the events jawd must store and lets their senders wait until it says it has.
type Acks struct {
	seq     atomic.Uint64
	mu      sync.Mutex
	waiting map[uint64]chan struct{}
	gone    chan struct{}
	once    sync.Once
}

func NewAcks() *Acks {
	return &Acks{waiting: make(map[uint64]chan struct{}), gone: make(chan struct{})}
}

// Next reserves a seq and the channel that closes when jawd acks it.
func (a *Acks) Next() (uint64, <-chan struct{}) {
	seq := a.seq.Add(1)
	done := make(chan struct{})
	a.mu.Lock()
	a.waiting[seq] = done
	a.mu.Unlock()
	return seq, done
}

// Acked releases whoever waits on seq. An ack for a seq nobody waits on is ignored.
func (a *Acks) Acked(seq uint64) {
	a.mu.Lock()
	done, ok := a.waiting[seq]
	delete(a.waiting, seq)
	a.mu.Unlock()
	if ok {
		close(done)
	}
}

// Gone closes when jawd has gone, after which no ack will come.
func (a *Acks) Gone() <-chan struct{} {
	return a.gone
}

// Close says jawd has gone.
func (a *Acks) Close() {
	a.once.Do(func() { close(a.gone) })
}

// Waiting is how many events jawd hasn't acked yet.
func (a *Acks) Waiting() int {
	a.mu.Lock()
	defer a.mu.Unlock()
	return len(a.waiting)
}
