import JawThemes
import JawUI
import SwiftUI

extension HarnessScene {
    static var roles: HarnessScene {
        HarnessScene(
            id: "roles", title: "Roles", symbol: "swatchpalette",
            synopsis: "Every colour the app asks for by what it's for, with the contrast of what's drawn on it",
            section: .foundations, covers: [],
        ) { RolesScene(roles: $0.roles) }
    }

    static var mimic: HarnessScene {
        HarnessScene(
            id: "mimic", title: "The Mimic", symbol: "mouth",
            synopsis: "Jaw's creature awake, mid-chomp and asleep, at whole multiples of its pixels",
            section: .foundations, covers: [MimicImage.self, MimicView.self],
        ) { MimicScene(roles: $0.roles) }
    }
}

/// The theme's roles as swatches, each with "Aa" in the text colour and the contrast it reads at; the
/// colours text is drawn in, each on the ground and on the surface with the contrast it reads at
/// there; and the senders' colours with the contrast they read at on the surface their names sit on.
private struct RolesScene: View {
    let roles: Roles

    var named: [(String, RGBA)] {
        [
            ("ground", roles.ground), ("surface", roles.surface), ("selection", roles.selection), ("text", roles.text),
            ("secondary", roles.secondary), ("tertiary", roles.tertiary), ("separator", roles.separator),
            ("accent", roles.accent), ("accent as text", roles.accentText), ("link", roles.link), ("yours", roles.yours),
            ("mention", roles.mention), ("online", roles.online), ("selected online", roles.selectedOnline), ("failed", roles.failed),
        ]
    }

    var textColours: [(String, RGBA)] {
        [
            ("text", roles.text), ("secondary", roles.secondary), ("tertiary", roles.tertiary), ("accent as text", roles.accentText),
            ("link", roles.link), ("mention", roles.mention), ("online", roles.online), ("failed", roles.failed),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "The roles",
                note: "Each with \"Aa\" in the text colour. The grounds text sits on (ground, surface, selection, yours) want 4.5:1 or more; the others aren't grounds for text, so a low ratio on them is right.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: Space.l)], spacing: Space.l) {
                    ForEach(named, id: \.0) { name, colour in
                        Specimen(caption: "\(name) · " + String(format: "%.1f:1", roles.text.contrast(with: colour)), roles: roles) {
                            RoundedRectangle(cornerRadius: Radius.control).fill(colour.color).frame(height: 50)
                                .overlay(Text("Aa").textStyle(.body).tone(.primary, roles))
                        }
                    }
                }
            }
            SpecimenGroup(
                title: "The text colours",
                note: "The text, secondary and tertiary text, the accent drawn as text, the links and read ticks, mentions, online and failed, each as it's drawn on the ground and on the surface. The text holds at 4.5:1 on both, the tertiary text at 2.5 to 4.5:1 and the rest at 3:1, as every theme's are held; one under its floor is a colour to look at.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 400), spacing: Space.l)], spacing: Space.l) {
                    ForEach(textColours, id: \.0) { name, colour in
                        let ground = String(format: "%.1f:1", colour.contrast(with: roles.ground))
                        let surface = String(format: "%.1f:1", colour.contrast(with: roles.surface))
                        Specimen(caption: "\(name) · \(ground) on the ground · \(surface) on the surface", roles: roles) {
                            HStack(spacing: Space.s) {
                                swatch(name, colour, on: roles.ground)
                                swatch(name, colour, on: roles.surface)
                            }
                        }
                    }
                }
            }
            SpecimenGroup(
                title: "The senders",
                note: "Names in a group, by the index jawd gives each member, on the surface their bubbles are. Each should read at 3:1 or more there, as every theme's are held; one under it is a name that's hard to read.",
                roles: roles,
            ) {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: Space.l)], spacing: Space.l) {
                    ForEach(Array(roles.senders.enumerated()), id: \.offset) { index, colour in
                        Specimen(caption: "sender \(index) · " + String(format: "%.1f:1", colour.contrast(with: roles.surface)), roles: roles) {
                            RoundedRectangle(cornerRadius: Radius.control).fill(roles.surface.color).frame(height: 50)
                                .overlay(Text("Ana Ribeiro").textStyle(.body).foregroundStyle(colour.color))
                        }
                    }
                }
            }
        }
    }

    /// `name` in `colour` on a swatch of `ground`.
    func swatch(_ name: String, _ colour: RGBA, on ground: RGBA) -> some View {
        RoundedRectangle(cornerRadius: Radius.control).fill(ground.color).frame(height: 50)
            .overlay(Text(name).textStyle(.body).foregroundStyle(colour.color))
    }
}

/// The Mimic's three looks at 4×, as empty states and linking draw it, and the scales it's drawn at,
/// with a button that makes it chomp.
private struct MimicScene: View {
    let roles: Roles
    @State private var chomps = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SpecimenGroup(
                title: "Its looks",
                note: "Awake in empty chats and linking, mid-chomp when a message comes into an empty chat, asleep (grey, eyes shut) when jawd is out of reach. Every pixel is a whole square: a soft edge means it was scaled by something other than a whole number.",
                roles: roles,
            ) {
                HStack(alignment: .bottom, spacing: Space.xxl) {
                    Specimen(caption: "awake", roles: roles) { MimicImage(look: .awake, scale: 4, chomps: chomps) }
                    Specimen(caption: "mid-chomp", roles: roles) { MimicImage(look: .chomp, scale: 4) }
                    Specimen(caption: "asleep", roles: roles) { MimicImage(look: .asleep, scale: 4) }
                    Button("Chomp") { chomps += 1 }
                }
            }
            SpecimenGroup(
                title: "Its scales",
                note: "1×, 2× and 8× its pixels. Each is crisp at its own size, with no pixel wider than its neighbours.",
                roles: roles,
            ) {
                HStack(alignment: .bottom, spacing: Space.xxl) {
                    Specimen(caption: "1×", roles: roles) { MimicImage(look: .awake, scale: 1) }
                    Specimen(caption: "2×", roles: roles) { MimicImage(look: .awake, scale: 2) }
                    Specimen(caption: "8×", roles: roles) { MimicImage(look: .awake, scale: 8) }
                }
            }
        }
    }
}
