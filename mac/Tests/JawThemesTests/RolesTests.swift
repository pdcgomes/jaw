@testable import JawThemes
import Testing

@Test func `jaw's theme is the terminal's Tokyo Night by role`() {
    let dark = Catalog.jaw.dark
    #expect(dark.ground == RGBA(0x1A1B26) && dark.text == RGBA(0xC0CAF5) && dark.accent == RGBA(0x7AA2F7))
    #expect(dark.senders.count == 8, "eight senders' colours, as theme.rs has them")
    #expect(dark.sender(9) == dark.sender(1) && dark.sender(-1) == dark.sender(7), "jawd's index wraps")
    #expect(dark.link == dark.accent && dark.accentText == dark.accent, "Tokyo Night's links and its accent drawn as text are its blue")
    #expect(dark.secondary.hex == 0x7E87AF && dark.tertiary.hex == 0x66709A,
            "its secondary text a step lighter for a card's well in your bubble, its dim text a shade lighter for its surface")
    let light = Catalog.jaw.light
    #expect(light.accent == RGBA(0x2E7DE9) && light.accentText.hex == 0x2D68C4 && light.link.hex == 0x2D68C4,
            "Tokyo Night Day's blue fills as it's written, and is a step deeper where it's drawn as text and as links, to read on its selection")
}

@Test func `text reads on every ground it's drawn on, dark and light`() {
    for roles in [Catalog.jaw.dark, Catalog.jaw.light] {
        for ground in [roles.ground, roles.surface, roles.yours, roles.selection] {
            #expect(roles.text.contrast(with: ground) >= 4.5, "text on \(ground), dark \(roles.isDark)")
        }
        #expect(roles.secondary.contrast(with: roles.ground) >= 3, "secondary text, dark \(roles.isDark)")
    }
}

@Test func `typing reads where the chat list and the conversation draw it, a selected row's on the selection, dark and light`() {
    for roles in [Catalog.jaw.dark, Catalog.jaw.light] {
        for ground in [roles.ground, roles.surface] {
            #expect(roles.online.contrast(with: ground) >= 3, "online on \(ground), dark \(roles.isDark), the settings design's band")
        }
        let selected = roles.selectedOnline
        #expect(selected.contrast(with: roles.selection) >= 4.5, "a selected row's typing on its selection, dark \(roles.isDark)")
        #expect(selected.g > selected.r && selected.g > selected.b, "still green, dark \(roles.isDark)")
    }
    #expect(Catalog.jaw.dark.selectedOnline == Catalog.jaw.dark.online, "Tokyo Night's green reads on its selection as it is")
}

@Test func `a colour over another is what the eye sees`() {
    let half = RGBA(r: 1, g: 1, b: 1, a: 0.5).over(RGBA(r: 0, g: 0, b: 0))
    #expect(abs(half.r - 0.5) < 1e-9 && half.a == 1)
    #expect(RGBA(0xFFFFFF).contrast(with: RGBA(0x000000)) > 20.9)
}

/// Tokyo Night Day reads as held: its dim text, raised from 2.54:1 on its ground to read at 3:1 there,
/// at 3.01:1, and its red, moved from F52A65 to C0316D to read at 3:1 on its selection, at 4.16:1 on
/// its ground.
@Test func `Jaw's Tokyo Night reads as every theme must, in both halves`() {
    reads(Catalog.jaw.dark, "Tokyo Night")
    reads(Catalog.jaw.light, "Tokyo Night Day")
}
