import AppKit
import SwiftUI

/// The type scale every surface but the messages draws with: the messages' own fonts are the
/// layout's (`FontSet`), sized by the style picked in Settings.
public enum TextStyle: CaseIterable, Sendable {
    /// A screen's title: linking, a logged-out account.
    case title
    /// A state's or a sheet's heading.
    case heading
    /// Names in lists and headers, the box.
    case body
    /// A list's second line, a field's label.
    case callout
    /// Details, hints, footnotes.
    case caption
    /// The smallest hints, under a list.
    case micro
    /// The status line and jawd's own words.
    case mono
    /// A key, as ⌘K is shown beside what it does.
    case key
    /// A code to type on the phone.
    case display
    /// An emoji in a picker's grid.
    case emoji

    public var size: CGFloat {
        switch self {
        case .display: 34
        case .emoji: 22
        case .title: 17
        case .heading: 15
        case .body: 13
        case .callout: 12
        case .caption, .mono, .key: 11
        case .micro: 10
        }
    }

    public var weight: NSFont.Weight {
        switch self {
        case .title, .heading, .key, .display: .semibold
        case .body, .callout, .caption, .micro, .mono, .emoji: .regular
        }
    }

    public var monospaced: Bool {
        self == .mono || self == .key || self == .display
    }

    public var nsFont: NSFont {
        monospaced ? .monospacedSystemFont(ofSize: size, weight: weight) : .systemFont(ofSize: size, weight: weight)
    }

    /// The same style, bolder, for a name that's unread or a title in a row.
    public var emphasised: NSFont {
        monospaced ? .monospacedSystemFont(ofSize: size, weight: .semibold) : .systemFont(ofSize: size, weight: .semibold)
    }

    public var font: Font {
        .system(size: size, weight: Font.Weight(weight), design: monospaced ? .monospaced : .default)
    }
}

extension Font.Weight {
    init(_ weight: NSFont.Weight) {
        self = switch weight {
        case .semibold: .semibold
        case .bold: .bold
        case .medium: .medium
        default: .regular
        }
    }
}
