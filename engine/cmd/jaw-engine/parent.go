package main

import (
	"os"

	waLog "go.mau.fi/whatsmeow/util/log"
)

// exitWithJawd exits the engine as soon as jawd, the process that started it, has gone. jawd going
// closes the engine's standard input, which the engine exits on, unless another process holds the
// other end too, as one jawd starts at the wrong moment can; then jawd going says so alone.
func exitWithJawd(jawd int, log waLog.Logger) {
	err := watchParent(jawd, func() {
		log.Warnf("jawd has gone, so jaw-engine exits")
		os.Exit(0)
	})
	if err != nil {
		log.Warnf("can't watch for jawd going, so jaw-engine exits only once its standard input closes: %v", err)
	}
}
