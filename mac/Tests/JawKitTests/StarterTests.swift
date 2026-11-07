import Foundation
@testable import JawKit
import Testing

/// What a Jaw knows at launch: a release built from main, on jaw's own folder, unless told otherwise.
func facts(debug: Bool = false, demo: Bool = false, own: Bool = true, account: Bool = false, from: String? = "main",
           jawd: String? = "/Jaw.app/Contents/Helpers/jawd", terminalAgent: Bool = false) -> Starter.Here
{
    Starter.Here(debug: debug, demo: demo, home: own ? "/u/Library/Application Support/jaw" : "/tmp/jaw-x", jawsOwn: own, account: account,
                 build: Build(from: from, at: from.map { "\($0) at 1a2b3c4" }), jawd: jawd,
                 environment: ["PATH": "/usr/bin:/bin"], terminalAgent: terminalAgent)
}

/// The moment of each test's launch.
let t0 = Date(timeIntervalSince1970: 1_800_000_000)

@Test func `a release from main on jaw's own folder starts the jawd inside it at once, in a group of its own, and only then asks for an earlier Jaw's agent to be unregistered`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    let spawn = Spawn(program: "/Jaw.app/Contents/Helpers/jawd", arguments: [],
                      environment: ["PATH": "/usr/bin:/bin", "JAW_HOME": "/u/Library/Application Support/jaw"],
                      log: "/u/Library/Application Support/jaw/jawd.log", ownGroup: true)
    #expect(first == [.start(spawn), .unregister] && shown == .starting, "jawd first, then SMAppService, which nothing waits on")
    let answered = starter.unregistered(removed: false, answers: false, now: t0 + 0.2)
    #expect(answered.isEmpty, "with nothing found, the jawd already starting is the one")
    let again = starter.unregistered(removed: true, answers: false, now: t0 + 0.3)
    #expect(again.isEmpty, "SMAppService's answer counts once")
    #expect(starter.unreachable(now: t0 + 14) == .starting)
    var starting = Starter(facts())
    _ = starting.launched(answers: false, now: t0)
    #expect(starting.unregistered(removed: true, answers: false, now: t0 + 0.2)
        == [.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)],
        "the jawd already starting is the one, so none is started beside it")
}

@Test func `an earlier Jaw's agent whose jawd answered is said once SMAppService has removed it, and a jawd of the app's takes its place unless another answers`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: true, now: t0)
    #expect(first == [.unregister] && shown == .waiting)
    #expect(starter.unreachable(now: t0 + 0.1) == .unreachable, "until SMAppService says the agent has gone, it's jaw-client's to say")
    let steps = starter.unregistered(removed: true, answers: false, now: t0 + 0.3)
    #expect(steps.first == .say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false))
    #expect(steps.count == 2 && starter.spawn.map { steps.last == .start($0) } == true)
    #expect(starter.unreachable(now: t0 + 0.4) == .starting)
    var other = Starter(facts())
    _ = other.launched(answers: true, now: t0)
    #expect(other.unregistered(removed: true, answers: true, now: t0 + 0.3).count == 1, "another jawd answers, so none is started")
    var nothing = Starter(facts())
    _ = nothing.launched(answers: true, now: t0)
    #expect(nothing.unregistered(removed: false, answers: false, now: t0 + 0.3).isEmpty,
            "with no agent removed, a jawd that went is jaw-client's to say, as in the terminal")
}

@Test func `a jawd the app started has the terminal's 15 s to answer, after which jaw-client's words show`() {
    var starter = Starter(facts(own: false))
    let (steps, shown) = starter.launched(answers: false, now: t0)
    #expect(steps.count == 1 && shown == .starting, "on a folder that isn't jaw's own, nothing's unregistered")
    #expect(starter.unreachable(now: t0 + 14.9) == .starting && starter.unreachable(now: t0 + 15) == .unreachable)
}

@Test func `a jawd that stopped because another held the folder is started again a second later while nothing answers, whatever the window does, for 15 s`() {
    let held = "another jawd is running on /tmp/jaw-x"
    var starter = Starter(facts(own: false))
    _ = starter.launched(answers: false, now: t0)
    let stopped = starter.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the app waits a second on a timer of its own, not for jaw-client's next try")
    #expect(starter.unreachable(now: t0 + 0.5) == .starting)
    let again = starter.waited(answers: false, now: t0 + 1.1)
    #expect(starter.spawn.map { again == [.start($0)] } == true, "then starts it again")
    let twice = starter.waited(answers: false, now: t0 + 1.2)
    #expect(twice.isEmpty, "once for each wait")
    let late = starter.exited(status: 1 << 8, said: held, now: t0 + 15.1)
    #expect(late.isEmpty && starter.unreachable(now: t0 + 16.5) == .failed("jawd stopped as it started (exit status: 1): \(held)"),
            "past the 15 s it says why")
    var answering = Starter(facts(own: false))
    _ = answering.launched(answers: false, now: t0)
    _ = answering.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    let taken = answering.waited(answers: true, now: t0 + 1.1)
    #expect(taken.isEmpty, "not while another jawd answers")
    var greeted = Starter(facts(own: false))
    _ = greeted.launched(answers: false, now: t0)
    _ = greeted.exited(status: 1 << 8, said: held, now: t0 + 0.1)
    greeted.answered()
    let after = greeted.waited(answers: false, now: t0 + 1.1)
    #expect(after.isEmpty && greeted.unreachable(now: t0 + 1.2) == .unreachable, "nor once a jawd has said hello")
}

@Test func `a jawd refused by jaw.lock while an earlier Jaw's agent's was stopping is started again once after the wait, and none beside it when SMAppService says it removed that agent`() {
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting,
            "nothing answers as the agent's jawd stops, so the app's starts before SMAppService is asked")
    let stopped = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the agent's jawd held jaw.lock for the moment its stop took")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 0.2)
    #expect(removed == [.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)],
            "the app's is already to be started again, so none is started beside it")
    let again = starter.waited(answers: false, now: t0 + 1.1)
    #expect(starter.spawn.map { again == [.start($0)] } == true, "then the app's is started again, the one start since it stopped")
    let twice = starter.waited(answers: false, now: t0 + 1.2)
    #expect(twice.isEmpty, "and only once")
}

@Test func `an earlier Jaw's agent whose jawd held jaw.lock against the app's as it started, then answered, is said once SMAppService has removed it, and a jawd of the app's takes its place, with 15 s of its own, unless another answers`() {
    let said = Starter.Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
    var starter = Starter(facts())
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting,
            "nothing answers while the agent's jawd starts, so the app's starts before SMAppService is asked")
    let stopped = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 0.1)
    #expect(stopped == [.wait(Starter.again)], "the agent's jawd held jaw.lock as it started")
    let taken = starter.waited(answers: true, now: t0 + 1.1)
    #expect(taken.isEmpty, "by the wait's end the agent's jawd answers, so the app's isn't started again")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 20)
    #expect(starter.spawn.map { removed == [said, .start($0)] } == true,
            "SMAppService has stopped the agent's jawd, and none of the app's is on its way, so the app's starts")
    #expect(starter.unreachable(now: t0 + 20.1) == .starting, "its 15 s count from this start, though more than 15 have passed since the first")
    let refused = starter.exited(status: 1 << 8, said: Starter.held + starter.here.home, now: t0 + 20.2)
    #expect(refused == [.wait(Starter.again)], "and one refused by jaw.lock within them is to be started again, as any is")
    var other = Starter(facts())
    _ = other.launched(answers: false, now: t0)
    _ = other.exited(status: 1 << 8, said: Starter.held + other.here.home, now: t0 + 0.1)
    _ = other.waited(answers: true, now: t0 + 1.1)
    #expect(other.unregistered(removed: true, answers: true, now: t0 + 20) == [said], "another jawd answers by then, so none is started")
}

@Test func `a jawd of the app's refused by jaw.lock past its 15 s, or one that couldn't be started, is tried again once SMAppService has removed an earlier Jaw's agent, the window saying Starting jawd, and its words again if it fails again`() {
    let said = Starter.Step.say("an earlier Jaw started jawd at login; it doesn't now, and Jaw starts it as it opens", warn: false)
    var starter = Starter(facts())
    let held = Starter.held + starter.here.home
    let (first, shown) = starter.launched(answers: false, now: t0)
    #expect(starter.spawn.map { first == [.start($0), .unregister] } == true && shown == .starting)
    let late = starter.exited(status: 1 << 8, said: held, now: t0 + 15.1)
    #expect(late.isEmpty && starter.unreachable(now: t0 + 16) == .failed("jawd stopped as it started (exit status: 1): \(held)"),
            "an agent's jawd slow to listen held jaw.lock past the 15 s, so it says why")
    let removed = starter.unregistered(removed: true, answers: false, now: t0 + 30)
    #expect(starter.spawn.map { removed == [said, .start($0)] } == true,
            "SMAppService has stopped that agent's jawd, and the app's that failed isn't on its way, so another starts")
    #expect(starter.unreachable(now: t0 + 30.1) == .starting, "the failure is no longer true, and this start has 15 s of its own")
    var unstarted = Starter(facts())
    _ = unstarted.launched(answers: false, now: t0)
    unstarted.didntStart("can't start jawd: Permission denied")
    let again = unstarted.unregistered(removed: true, answers: false, now: t0 + 0.2)
    #expect(unstarted.spawn.map { again == [said, .start($0)] } == true, "one that couldn't be started is tried once more")
    unstarted.didntStart("can't start jawd: Permission denied")
    #expect(unstarted.unreachable(now: t0 + 0.3) == .failed("can't start jawd: Permission denied"), "and says its words again")
}

@Test func `a jawd that stopped as it started for anything else says why until a jawd answers, and one that stops after it answered is jaw-client's to say`() {
    var starter = Starter(facts(own: false))
    _ = starter.launched(answers: false, now: t0)
    let newer = starter.exited(status: 1 << 8, said: "jaw.db is from a newer jawd (schema 19, this jawd knows 17)", now: t0 + 0.2)
    #expect(newer.isEmpty)
    #expect(starter.unreachable(now: t0 + 30)
        == .failed("jawd stopped as it started (exit status: 1): jaw.db is from a newer jawd (schema 19, this jawd knows 17)"))
    starter.answered()
    #expect(starter.unreachable(now: t0 + 31) == .unreachable)
    var killed = Starter(facts(own: false))
    _ = killed.launched(answers: false, now: t0)
    _ = killed.exited(status: 9, said: "", now: t0 + 0.2)
    #expect(killed.unreachable(now: t0 + 0.3) == .failed("jawd stopped as it started (signal: 9 (SIGKILL))"))
    var crashed = Starter(facts(own: false))
    _ = crashed.launched(answers: false, now: t0)
    crashed.answered()
    let steps = crashed.exited(status: 9, said: "", now: t0 + 600)
    #expect(steps.isEmpty && crashed.unreachable(now: t0 + 601) == .unreachable, "a jawd that crashed after it answered isn't started again")
    var unstarted = Starter(facts(own: false))
    _ = unstarted.launched(answers: false, now: t0)
    unstarted.didntStart("can't start jawd: Permission denied")
    #expect(unstarted.unreachable(now: t0 + 1) == .failed("can't start jawd: Permission denied"))
}

@Test func `on an account's folder a Jaw from a branch, or one that doesn't say, starts no jawd and unregisters nothing, saying why`() {
    for from in ["branch", "changed", nil] as [String?] {
        for (own, account) in [(true, false), (false, true)] {
            var starter = Starter(facts(own: own, account: account, from: from))
            let (steps, shown) = starter.launched(answers: false, now: t0)
            let why = Build(from: from, at: from.map { "\($0) at 1a2b3c4" }).refusal
            #expect(steps.isEmpty && shown == .failed(why), "from \(from ?? "nothing"), on \(own ? "jaw's own folder" : "one holding a session.db")")
            #expect(starter.unreachable(now: t0 + 30) == .failed(why), "said until a jawd answers")
        }
    }
    var answered = Starter(facts(from: "branch"))
    let (none, waiting) = answered.launched(answers: true, now: t0)
    #expect(none.isEmpty && waiting == .waiting, "a jawd that answers there is used, as any is")
    var elsewhere = Starter(facts(own: false, from: "branch"))
    let (_, starting) = elsewhere.launched(answers: false, now: t0)
    #expect(starting == .starting, "on a folder of its own with no session.db, a branch's starts its jawd")
    var empty = Starter(facts(jawd: nil))
    let (steps, shown) = empty.launched(answers: false, now: t0)
    #expect(steps.isEmpty && shown == .failed("there's no jawd inside this Jaw: mise run mac puts one in its Contents/Helpers"),
            "with nothing to take its place, an earlier agent isn't unregistered")
}

@Test func `a build from source starts only the demo's jawd, which the app stops, never on an account's folder, and says how to start one otherwise`() {
    var demo = Starter(facts(debug: true, demo: true, own: false, jawd: "/repo/target/debug/jawd"))
    let (steps, shown) = demo.launched(answers: false, now: t0)
    #expect(steps == [.start(Spawn(program: "/repo/target/debug/jawd", arguments: ["--engine", "demo", "--no-notify"],
                                   environment: ["PATH": "/usr/bin:/bin", "JAW_HOME": "/tmp/jaw-x"], log: "/tmp/jaw-x/jawd.log", ownGroup: false))])
    #expect(shown == .starting)
    var home = Starter(facts(debug: true, own: false))
    let (none, said) = home.launched(answers: false, now: t0)
    #expect(none.isEmpty && said == .failed("nothing answers on /tmp/jaw-x/jaw.sock, and a build from source starts only --demo's jawd: "
            + "start one there with jaw, or open Jaw with --demo"))
    for (own, account) in [(true, false), (false, true)] {
        var refused = Starter(facts(debug: true, demo: true, own: own, account: account))
        let (steps, shown) = refused.launched(answers: false, now: t0)
        #expect(steps.isEmpty && shown == .failed("--demo runs a made-up account, so not on jaw's own folder or another account's: "
                + "give it a folder of its own with --home"), "on \(own ? "jaw's own folder" : "one holding a session.db")")
    }
    var nothing = Starter(facts(debug: true, demo: true, own: false, jawd: nil))
    let (_, missing) = nothing.launched(answers: false, now: t0)
    #expect(missing == .failed("there's no jawd to run: mise run build builds it"))
}

@Test func `the terminal's agent is named with what removes it, on jaw's own folder alone, and nothing is unregistered beside it`() {
    for from in ["main", "branch"] {
        var starter = Starter(facts(from: from, terminalAgent: true))
        let (steps, _) = starter.launched(answers: true, now: t0)
        let starts = from == "main" ? ", and Jaw starts jawd as it opens" : ""
        #expect(steps == [.say("jaw agent install's LaunchAgent still starts jawd at login: jaw agent uninstall removes it\(starts)", warn: true)],
                "from \(from): the label is that agent's, so SMAppService is asked nothing")
    }
    var unanswered = Starter(facts(terminalAgent: true))
    let (steps, shown) = unanswered.launched(answers: false, now: t0)
    #expect(steps.count == 2 && unanswered.spawn.map { steps.last == .start($0) } == true && shown == .starting,
            "with nothing answering, its own jawd starts beside that agent, as the terminal's does")
    var elsewhere = Starter(facts(own: false, terminalAgent: true))
    let (none, _) = elsewhere.launched(answers: true, now: t0)
    #expect(none.isEmpty)
}

@Test func `jaw's own folder is known however its path is written`() throws {
    let root = "/tmp/jaws-\(UInt32.random(in: 0 ... .max))"
    let own = root + "/Application Support/jaw"
    try FileManager.default.createDirectory(atPath: own + "/spool", withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: root) }
    try FileManager.default.createSymbolicLink(atPath: root + "/link", withDestinationPath: own)
    try FileManager.default.createSymbolicLink(atPath: root + "/spool", withDestinationPath: own + "/spool")
    #expect(Starter.same(own + "/", own))
    #expect(Starter.same(root + "/Application Support/../Application Support/jaw", own))
    #expect(Starter.same(root + "/link", own), "through a symlink")
    #expect(Starter.same(root + "/spool/..", own), "through a symlink followed by .., as jawd would open it")
    #expect(Starter.same("/private" + own, own), "/tmp is /private/tmp")
    #expect(Starter.same("/System/Volumes/Data/private" + own, own), "through the data volume's firmlink")
    #expect(Starter.same(root + "/application support/JAW", own), "whatever its case")
    #expect(Starter.same(root + "/not yet/jaw", root + "/not yet/jaw/"), "a folder that isn't there yet")
    #expect(!Starter.same(root + "/Application Support/jaw-demo", own) && !Starter.same(root, own))
}

@Test func `the words the starter knows another jawd by are jawd's own`() throws {
    let main = try String(contentsOf: repo.appending(path: "crates/jawd/src/main.rs"), encoding: .utf8)
    #expect(main.contains("\"\(Starter.held){}\""), "jawd's lock says \(Starter.held)…")
}
