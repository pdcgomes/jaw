import Foundation
@testable import JawKit
import Testing

@Test func `only a Jaw built from main or a release's tag on it starts jawd on an account's folder, and the others say what they were built from`() {
    let main = Build(info: ["JawBuiltFrom": "main", "JawBuiltAt": "c50a2ba3 on main", "CFBundleName": "Jaw"])
    #expect(main == Build(from: "main", at: "c50a2ba3 on main") && main.startsOnAccounts)
    #expect(Build(from: "release", at: "v0.2.0").startsOnAccounts)
    for from in ["branch", "changed", nil] as [String?] {
        #expect(!Build(from: from, at: "mac-start-jawd at 1a2b3c4").startsOnAccounts, "from \(from ?? "nothing")")
    }
    #expect(Build(from: "branch", at: "main at cc471e2, not on origin/main").refusal
        == "this Jaw was built from main at cc471e2, not on origin/main, so it doesn't start jawd on jaw's own folder or another account's, "
        + "where a jawd from anything but origin/main or a release's tag could move jaw.db to a version origin/main's can't open: "
        + "start one with jaw in a terminal, or open a Jaw built from origin/main")
    #expect(Build(info: [:]) == Build(from: nil, at: nil))
    #expect(Build(info: [:]).refusal.hasPrefix("this Jaw doesn't say what it was built from, as make mac's builds do, so it doesn't start jawd"))
}
