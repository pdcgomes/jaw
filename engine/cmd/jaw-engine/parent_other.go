//go:build !darwin && !linux

package main

import "errors"

func watchParent(int, func()) error {
	return errors.New("this system has no way to watch for a process exiting")
}
