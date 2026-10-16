// jaw-engine speaks WhatsApp for jawd, through whatsmeow. jawd starts it and talks to it on its
// standard input and output, so it isn't meant to be run by hand; its logs go to standard error.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"runtime/debug"

	"golang.org/x/term"

	"github.com/pdcgomes/jaw/engine/internal/link"
	"github.com/pdcgomes/jaw/engine/internal/wa"
)

const version = "0.1.0"

var versionParts = [3]uint32{0, 1, 0}

func main() {
	showVersion := flag.Bool("version", false, "print jaw-engine's version and whatsmeow's")
	home := flag.String("home", "", "jaw's folder, where whatsmeow's session.db lives")
	level := flag.String("log", "info", "the lowest level logged: debug, info, warn or error")
	flag.Parse()
	if *showVersion {
		fmt.Printf("jaw-engine %s (whatsmeow %s)\n", version, whatsmeowVersion())
		return
	}
	if *home == "" || term.IsTerminal(int(os.Stdout.Fd())) {
		fmt.Fprintln(os.Stderr, "jaw-engine is started by jawd, which talks to it on its standard input and output")
		os.Exit(2)
	}
	wa.Configure(versionParts)
	log := wa.NewLogger("engine", *level)
	program := "jaw-engine " + version
	if err := wa.Run(context.Background(), link.New(os.Stdin, os.Stdout), *home, program, whatsmeowVersion(), log); err != nil {
		log.Errorf("%v", err)
		os.Exit(1)
	}
	// jawd closed the link. Exiting now, rather than letting whatsmeow's handlers finish, keeps it
	// from acknowledging messages jawd may not have stored.
	os.Exit(0)
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
