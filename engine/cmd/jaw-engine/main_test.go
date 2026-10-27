package main

import "testing"

func TestTheDeviceIsListedWithTheReleaseItsVersionIsOrFollows(t *testing.T) {
	for v, want := range map[string][3]uint32{
		"0.2.0":             {0, 2, 0},
		"1.12.3-3-g4d2c1e9": {1, 12, 3},
		"32b8eed":           {0, 1, 0},
		"unknown":           {0, 1, 0},
		"1.2":               {0, 1, 0},
	} {
		if got := versionParts(v); got != want {
			t.Errorf("versionParts(%q) = %v, want %v", v, got, want)
		}
	}
}
