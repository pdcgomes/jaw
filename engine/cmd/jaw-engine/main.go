// jaw-engine speaks WhatsApp for jawd, through whatsmeow. jawd starts it and talks to it on its
// standard input and output, so it isn't meant to be run by hand.
package main

import (
	"flag"
	"fmt"
	"os"
	"runtime/debug"

	"github.com/pdcgomes/jaw/engine/internal/wa"
)

const version = "0.1.0"

var versionParts = [3]uint32{0, 1, 0}

func main() {
	showVersion := flag.Bool("version", false, "print jaw-engine's version and whatsmeow's")
	flag.Parse()
	if *showVersion {
		fmt.Printf("jaw-engine %s (whatsmeow %s)\n", version, whatsmeowVersion())
		return
	}
	wa.Configure(versionParts)
	fmt.Fprintln(os.Stderr, "jaw-engine is started by jawd, which talks to it on its standard input and output")
	os.Exit(2)
}

// whatsmeowVersion is the whatsmeow commit this binary was built with, as go.mod pins it.
func whatsmeowVersion() string {
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "unknown"
	}
	for _, dep := range info.Deps {
		if dep.Path == "go.mau.fi/whatsmeow" {
			return dep.Version
		}
	}
	return "unknown"
}
