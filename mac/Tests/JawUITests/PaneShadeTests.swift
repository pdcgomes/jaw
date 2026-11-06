import AppKit
import JawThemes
@testable import JawUI
import Testing

/// The panes' shade, among several: nothing of the focused pane's shows outside its outline, which
/// has the ground outside it to the pane's edge, corners and all, while a pane without the keys, and
/// the focused one when there's no outline, shows its own to its edge.

/// Draws itself in `colour` from edge to edge, as a pane's chat does with a row scrolled past its foot.
final class EdgeToEdge: NSView {
    let colour: RGBA

    init(_ colour: RGBA) {
        self.colour = colour
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_: NSRect) {
        colour.nsColor.setFill()
        bounds.fill()
    }
}

@MainActor @Test func `nothing of the focused pane shows outside its outline, while the pane beside it, and one with no outline, show to their edges`() throws {
    let roles = Catalog.jaw.dark
    let red = RGBA(r: 1, g: 0, b: 0)
    let grid = PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
        .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1, chat: "ana"),
        .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1, chat: "rui"),
    ])
    let view = GridView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
    let panes: [Int: NSView] = [1: EdgeToEdge(red), 2: EdgeToEdge(red)]
    /// The grid drawn with the first pane's keys, as `look` says, and the colour it has at a point
    /// from its top left.
    func drawn(_ look: PaneLook) throws -> (CGFloat, CGFloat) -> RGBA {
        view.show(grid, views: panes, focused: 1, roles: roles, look: look)
        view.layoutSubtreeIfNeeded()
        let image = try picture(view, on: roles.ground)
        let p = pixels(image)
        return { x, y in
            let i = (Int(y * 2) * image.width + Int(x * 2)) * 4
            return RGBA(r: Double(p[i]) / 255, g: Double(p[i + 1]) / 255, b: Double(p[i + 2]) / 255)
        }
    }
    /// Within two steps a channel, as a picture's colour conversion rounds.
    func near(_ a: RGBA, _ b: RGBA) -> Bool {
        max(abs(a.r - b.r), abs(a.g - b.g), abs(a.b - b.b)) <= 2.0 / 255
    }
    // The first pane is 199 points wide, a hairline short of the second, its outline 4 inside it with
    // corners of 10, so 5 in from a corner each way is outside the outline's curve.
    let outlined = try drawn(PaneLook(dim: 0, outline: roles.accent))
    #expect(near(outlined(100, 150), red), "inside the outline, the pane's own")
    let outside: [(CGFloat, CGFloat, String)] = [
        (2, 150, "left edge"), (197, 150, "right edge"), (100, 2, "top"), (100, 298, "foot"), (5, 5, "top left corner"),
        (194, 295, "bottom right corner"),
    ]
    for (x, y, edge) in outside {
        #expect(near(outlined(x, y), roles.ground), "between the outline and the pane's \(edge), the ground")
    }
    #expect(near(outlined(202, 150), red), "the pane without the keys, to its edge")
    let plain = try drawn(PaneLook(dim: 0, outline: nil))
    #expect(near(plain(2, 150), red) && near(plain(5, 5), red), "with no outline, the focused pane's own to its edge")
}
