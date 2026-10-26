import JawThemes
import Observation
import SwiftUI

/// The layout editor's state: the grid on its cells, a cell cursor and a selection, the name.
/// Every change has a key as well as a mouse gesture: the arrows move the cursor, ⇧ and the
/// arrows grow the selection, M merges it, S splits the pane under the cursor into cells, 1 to 5
/// pick a preset, ⌥ and the arrows change the columns and rows, Return saves and Esc cancels.
@MainActor
@Observable
public final class LayoutEditorModel {
    public struct Cell: Hashable, Sendable {
        public var c: Int
        public var r: Int

        public init(_ c: Int, _ r: Int) {
            self.c = c
            self.r = r
        }
    }

    public enum Mode: Sendable {
        /// Saving the panes as a new layout: the name is asked for.
        case saveAs
        /// Editing the layout on screen, named or not.
        case edit
    }

    public let mode: Mode
    public private(set) var grid: PaneGrid
    public private(set) var columns: Int
    public private(set) var rows: Int
    public var cursor = Cell(0, 0)
    /// Where a selection started, while ⇧ is held or the pointer drags.
    public var anchor: Cell?
    public var name: String
    public private(set) var said: String?
    let original: String?
    let taken: [String]
    /// A chat's name, by its id, for the panes' labels.
    let chatName: (String) -> String

    public init(mode: Mode, grid: PaneGrid?, name: String?, taken: [String], chatName: @escaping (String) -> String) {
        self.mode = mode
        let start = grid ?? Self.presets[0].grid
        let columns = max(PaneGrid.columnsNew, start.cols.count - 1), rows = max(PaneGrid.rowsNew, start.rows.count - 1)
        self.columns = columns
        self.rows = rows
        self.grid = start.onCells(columns: columns, rows: rows)
        self.name = mode == .saveAs ? "" : name ?? ""
        original = mode == .saveAs ? nil : name
        self.taken = taken
        self.chatName = chatName
    }

    // MARK: Presets

    public struct Preset: Sendable {
        public let name: String
        public let grid: PaneGrid
    }

    public static let presets: [Preset] = [
        Preset(name: "Two", grid: PaneGrid(cols: [0, 0.5, 1], rows: [0, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1),
        ])),
        Preset(name: "Three", grid: PaneGrid(cols: [0, 1.0 / 3, 2.0 / 3, 1], rows: [0, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1), .init(id: 3, c0: 2, c1: 3, r0: 0, r1: 1),
        ])),
        Preset(name: "One and two", grid: PaneGrid(cols: [0, 2.0 / 3, 1], rows: [0, 0.5, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 2), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1), .init(id: 3, c0: 1, c1: 2, r0: 1, r1: 2),
        ])),
        Preset(name: "Four", grid: PaneGrid(cols: [0, 0.5, 1], rows: [0, 0.5, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 1), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1),
            .init(id: 3, c0: 0, c1: 1, r0: 1, r1: 2), .init(id: 4, c0: 1, c1: 2, r0: 1, r1: 2),
        ])),
        Preset(name: "One and three", grid: PaneGrid(cols: [0, 2.0 / 3, 1], rows: [0, 1.0 / 3, 2.0 / 3, 1], slots: [
            .init(id: 1, c0: 0, c1: 1, r0: 0, r1: 3), .init(id: 2, c0: 1, c1: 2, r0: 0, r1: 1),
            .init(id: 3, c0: 1, c1: 2, r0: 1, r1: 2), .init(id: 4, c0: 1, c1: 2, r0: 2, r1: 3),
        ])),
    ]

    /// A preset's shape, the chats on screen put in its panes in reading order.
    public func preset(_ i: Int) {
        guard Self.presets.indices.contains(i) else { return }
        let chats = grid.chats
        var g = Self.presets[i].grid
        for (slot, chat) in zip(g.reading, chats) {
            g = g.with(chat: chat, in: slot.id)
        }
        grid = g.onCells(columns: columns, rows: rows)
        anchor = nil
        said = nil
    }

    // MARK: The cursor and the selection

    /// The cells chosen: from the anchor to the cursor, or the cursor's cell.
    public var selection: PaneGrid.Cells {
        let a = anchor ?? cursor
        return PaneGrid.Cells(c0: a.c, r0: a.r, c1: cursor.c, r1: cursor.r)
    }

    public func move(_ dc: Int, _ dr: Int, extend: Bool) {
        if extend, anchor == nil { anchor = cursor }
        if !extend { anchor = nil }
        cursor = Cell(min(max(cursor.c + dc, 0), columns - 1), min(max(cursor.r + dr, 0), rows - 1))
    }

    /// The pane over a cell.
    public func slot(at cell: Cell) -> PaneGrid.Slot? {
        grid.slots.first { cell.c >= $0.c0 && cell.c < $0.c1 && cell.r >= $0.r0 && cell.r < $0.r1 }
    }

    // MARK: Changing it

    /// Merges the chosen cells, and whatever panes they cut, into one pane.
    public func merge() {
        let before = grid.slots.count
        grid = grid.merging(selection)
        said = grid.slots.count == before ? "choose cells of more than one pane: drag across them, or ⇧ and the arrows" : nil
        anchor = nil
    }

    /// Splits the pane under the cursor, or `id`, back into one pane a cell.
    public func split(_ id: Int? = nil) {
        guard let id = id ?? slot(at: cursor)?.id else { return }
        switch grid.intoCells(id) {
        case let .success(g):
            grid = g
            said = nil
        case let .failure(refusal):
            said = refusal.said
        }
    }

    /// The least columns and rows the panes need.
    public var least: (columns: Int, rows: Int) {
        let pruned = grid.pruned()
        return (pruned.cols.count - 1, pruned.rows.count - 1)
    }

    public func setColumns(_ n: Int) {
        let n = min(max(n, least.columns, 1), PaneGrid.columnsMost)
        guard n != columns else { return }
        columns = n
        grid = grid.pruned().onCells(columns: columns, rows: rows)
        cursor = Cell(min(cursor.c, columns - 1), cursor.r)
        anchor = nil
    }

    public func setRows(_ n: Int) {
        let n = min(max(n, least.rows, 1), PaneGrid.rowsMost)
        guard n != rows else { return }
        rows = n
        grid = grid.pruned().onCells(columns: columns, rows: rows)
        cursor = Cell(cursor.c, min(cursor.r, rows - 1))
        anchor = nil
    }

    // MARK: Saving

    /// Why Save can't go yet, or nil when it can.
    public var notReady: String? {
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if mode == .saveAs, n.isEmpty { return "a layout needs a name" }
        if !n.isEmpty, n.caseInsensitiveCompare(original ?? "") != .orderedSame,
           taken.contains(where: { $0.caseInsensitiveCompare(n) == .orderedSame })
        {
            return "there's a layout by that name already"
        }
        if grid.slots.count > PaneGrid.most { return "\(PaneGrid.most) panes at most" }
        return nil
    }

    /// The layout to keep: the grid on its own lines again.
    public var result: PaneGrid {
        grid.pruned()
    }

    public func label(_ slot: PaneGrid.Slot) -> String {
        slot.chat.map(chatName) ?? "empty"
    }

    /// A key pressed while the grid has the keys; true when it took it.
    public func key(_ key: KeyEquivalent, characters: String, modifiers: EventModifiers) -> Bool {
        let shift = modifiers.contains(.shift)
        if modifiers.contains(.option) {
            switch key {
            case .leftArrow: setColumns(columns - 1)
            case .rightArrow: setColumns(columns + 1)
            case .upArrow: setRows(rows - 1)
            case .downArrow: setRows(rows + 1)
            default: return false
            }
            return true
        }
        switch key {
        case .leftArrow: move(-1, 0, extend: shift)
        case .rightArrow: move(1, 0, extend: shift)
        case .upArrow: move(0, -1, extend: shift)
        case .downArrow: move(0, 1, extend: shift)
        default:
            switch characters.lowercased() {
            case "m": merge()
            case "s": split()
            case "1", "2", "3", "4", "5": preset(Int(characters)! - 1)
            default: return false
            }
        }
        return true
    }
}

/// The editor's sheet: the name, the presets, the grid's size, the grid of cells with its panes,
/// and Save and Cancel. Drawn from the component library.
public struct LayoutEditorView: View {
    @Bindable var model: LayoutEditorModel
    let roles: Roles
    let save: (String, PaneGrid) -> Void
    let cancel: () -> Void
    @FocusState var gridFocused: Bool
    @State var dragFrom: LayoutEditorModel.Cell?

    public init(model: LayoutEditorModel, roles: Roles, save: @escaping (String, PaneGrid) -> Void, cancel: @escaping () -> Void) {
        self.model = model
        self.roles = roles
        self.save = save
        self.cancel = cancel
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: Space.l) {
            Text(model.mode == .saveAs ? "Save These Panes as a Layout" : "Edit Layout").textStyle(.heading).tone(.primary, roles)
            HStack(spacing: Space.l) {
                TextField(model.mode == .saveAs ? "Name: Work, Family" : "Name, to keep it as a layout", text: $model.name)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 260)
                    .onSubmit { gridFocused = true }
                Spacer()
                stepper("Columns", model.columns, keys: "⌥← ⌥→") { model.setColumns(model.columns + $0) }
                stepper("Rows", model.rows, keys: "⌥↑ ⌥↓") { model.setRows(model.rows + $0) }
            }
            HStack(spacing: Space.m) {
                ForEach(Array(LayoutEditorModel.presets.enumerated()), id: \.offset) { i, preset in
                    Button { model.preset(i) } label: {
                        VStack(spacing: Space.xs) {
                            thumb(preset.grid)
                            HStack(spacing: Space.xs) {
                                Text(preset.name).textStyle(.caption).tone(.secondary, roles)
                                KeyCap("\(i + 1)", roles: roles, tone: .tertiary)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .help("\(preset.name) (\(i + 1))")
                }
            }
            grid
                .focusable()
                .focusEffectDisabled()
                .focused($gridFocused)
                .onKeyPress(phases: .down) { press in
                    model.key(press.key, characters: press.characters, modifiers: press.modifiers) ? .handled : .ignored
                }
            HStack(spacing: Space.m) {
                Text(model.said ?? model.notReady ?? "Drag across cells, or ⇧ and the arrows then M, to merge them; S or ⊟ splits a pane back.")
                    .textStyle(.caption).tone(model.said != nil || model.notReady != nil ? .warn : .tertiary, roles)
                    .lineLimit(2)
                Spacer()
                Button("Cancel", action: cancel).keyboardShortcut(.cancelAction)
                Button("Save") { save(model.name, model.result) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.notReady != nil)
            }
        }
        .padding(Space.xxl)
        .frame(width: 700, height: 560)
        .background(roles.ground.color)
        .onAppear { gridFocused = model.mode == .edit }
    }

    func stepper(_ title: String, _ value: Int, keys: String, _ by: @escaping (Int) -> Void) -> some View {
        HStack(spacing: Space.xs) {
            Text(title).textStyle(.callout).tone(.secondary, roles)
            Button { by(-1) } label: { Image(systemName: "minus") }.buttonStyle(.borderless)
            Text("\(value)").textStyle(.body).monospacedDigit().tone(.primary, roles).frame(minWidth: 18)
            Button { by(1) } label: { Image(systemName: "plus") }.buttonStyle(.borderless)
        }
        .help("\(title) (\(keys))")
    }

    func thumb(_ g: PaneGrid) -> some View {
        Canvas { context, size in
            for slot in g.slots {
                let r = g.frames(in: CGRect(origin: .zero, size: size))[slot.id]!.insetBy(dx: 1, dy: 1)
                context.fill(Path(roundedRect: r, cornerRadius: Radius.small / 2), with: .color(roles.surface.color))
                context.stroke(Path(roundedRect: r, cornerRadius: Radius.small / 2), with: .color(roles.separator.color), lineWidth: Stroke.hairline)
            }
        }
        .frame(width: 64, height: 40)
    }

    var grid: some View {
        GeometryReader { geo in
            let size = geo.size
            let cw = size.width / CGFloat(model.columns), rh = size.height / CGFloat(model.rows)
            let cell = { (p: CGPoint) in
                LayoutEditorModel.Cell(min(max(Int(p.x / cw), 0), model.columns - 1), min(max(Int(p.y / rh), 0), model.rows - 1))
            }
            let unit = CGRect(origin: .zero, size: size)
            ZStack(alignment: .topLeading) {
                ForEach(0 ..< model.columns * model.rows, id: \.self) { i in
                    RoundedRectangle(cornerRadius: Radius.small)
                        .stroke(roles.separator.color.opacity(0.5), style: StrokeStyle(lineWidth: Stroke.hairline, dash: [3, 3]))
                        .frame(width: cw - Space.xs, height: rh - Space.xs)
                        .offset(x: CGFloat(i % model.columns) * cw + Space.xxs, y: CGFloat(i / model.columns) * rh + Space.xxs)
                }
                ForEach(model.grid.slots) { slot in
                    let f = model.grid.frames(in: unit)[slot.id]!.insetBy(dx: Space.xxs, dy: Space.xxs)
                    pane(slot)
                        .frame(width: f.width, height: f.height)
                        .offset(x: f.minX, y: f.minY)
                }
                let sel = model.selection
                RoundedRectangle(cornerRadius: Radius.small)
                    .stroke(roles.accent.color, lineWidth: Stroke.focus)
                    .background(RoundedRectangle(cornerRadius: Radius.small).fill(roles.accent.color.opacity(model.anchor == nil ? 0 : 0.15)))
                    .frame(width: CGFloat(sel.c1 - sel.c0 + 1) * cw - Space.xs, height: CGFloat(sel.r1 - sel.r0 + 1) * rh - Space.xs)
                    .offset(x: CGFloat(sel.c0) * cw + Space.xxs, y: CGFloat(sel.r0) * rh + Space.xxs)
                    .opacity(gridFocused || model.anchor != nil ? 1 : 0.4)
                    .allowsHitTesting(false)
            }
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0)
                .onChanged { drag in
                    gridFocused = true
                    let from = dragFrom ?? cell(drag.startLocation)
                    dragFrom = from
                    model.cursor = cell(drag.location)
                    model.anchor = from == model.cursor ? nil : from
                }
                .onEnded { _ in
                    if model.anchor != nil { model.merge() }
                    dragFrom = nil
                })
        }
    }

    func pane(_ slot: PaneGrid.Slot) -> some View {
        let big = (slot.c1 - slot.c0) * (slot.r1 - slot.r0) > 1
        return ZStack(alignment: .topTrailing) {
            RoundedRectangle(cornerRadius: Radius.control).fill(roles.surface.color)
            RoundedRectangle(cornerRadius: Radius.control).stroke(roles.separator.color, lineWidth: Stroke.hairline)
            Text(model.label(slot))
                .textStyle(.callout)
                .tone(slot.chat == nil ? .tertiary : .primary, roles)
                .lineLimit(1)
                .padding(Space.s)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            if big {
                Button { model.split(slot.id) } label: { Image(systemName: "rectangle.split.3x3").textStyle(.caption) }
                    .buttonStyle(.borderless)
                    .help("Split back into cells (S)")
                    .padding(Space.xs)
            }
        }
    }
}
