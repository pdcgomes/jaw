import Foundation
@testable import JawUI
import Testing

/// The time `Format` reckons days and times from: the system's clock, unless the harness or a test
/// sets another, so what's drawn can be drawn as of a fixed time.

@MainActor @Test func `now is the time Format's clock says, asked each time, and the clock is put back after`() {
    let before = Format.currentTime
    defer { Format.currentTime = before }
    Format.currentTime = { Samples.now }
    #expect(Format.now() == Samples.now)
    Format.currentTime = { Samples.now + 60000 }
    #expect(Format.now() == Samples.now + 60000, "asked each time, not kept from the first")
}

@MainActor @Test func `by default now is the system's clock`() {
    let system = Int64(Date().timeIntervalSince1970 * 1000)
    #expect(abs(Format.now() - system) < 1000, "within a second of Date()")
}
