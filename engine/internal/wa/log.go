package wa

import (
	"fmt"
	"os"
	"strings"

	waLog "go.mau.fi/whatsmeow/util/log"
)

var levels = map[string]int{"debug": 0, "info": 1, "warn": 2, "error": 3}

// Logger writes the engine's logs and whatsmeow's to standard error, which jawd keeps in its own
// log. Standard output is the link, so nothing else may write there.
type Logger struct {
	module string
	min    int
}

// NewLogger logs at `level` (debug, info, warn or error) and above.
func NewLogger(module, level string) waLog.Logger {
	min, ok := levels[strings.ToLower(level)]
	if !ok {
		min = levels["info"]
	}
	return Logger{module: module, min: min}
}

func (l Logger) Errorf(msg string, args ...any) { l.out(3, "ERROR", msg, args) }
func (l Logger) Warnf(msg string, args ...any)  { l.out(2, "WARN", msg, args) }
func (l Logger) Infof(msg string, args ...any)  { l.out(1, "INFO", msg, args) }
func (l Logger) Debugf(msg string, args ...any) { l.out(0, "DEBUG", msg, args) }

func (l Logger) Sub(module string) waLog.Logger {
	return Logger{module: l.module + "/" + module, min: l.min}
}

func (l Logger) out(level int, name, msg string, args []any) {
	if level < l.min {
		return
	}
	fmt.Fprintf(os.Stderr, "%s %s: %s\n", name, l.module, fmt.Sprintf(msg, args...))
}
