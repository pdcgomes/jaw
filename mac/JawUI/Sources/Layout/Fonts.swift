import AppKit
import CoreText
import Foundation

/// The fonts a style draws with, made once for it.
///
/// Core Text's fonts are immutable and safe to use from any thread, so a set made on the main
/// thread lays out messages on the layout queue.
public final class FontSet: @unchecked Sendable {
    public let size: CGFloat
    let body: CTFont
    let bold: CTFont
    let italic: CTFont
    let boldItalic: CTFont
    let mono: CTFont
    /// Names: 12-point semibold.
    let name: CTFont
    /// Labels around a message: 11 points.
    let small: CTFont
    let smallItalic: CTFont
    /// Times, in the system font with digits of one width, so times line up.
    let time: CTFont
    let reaction: CTFont
    /// A message of one to three emoji alone.
    let emoji: CTFont

    public init(size: CGFloat, mono useMono: Bool) {
        self.size = size
        let system = CTFontCreateUIFontForLanguage(.system, size, nil)!
        let monospaced = CTFontCreateUIFontForLanguage(.userFixedPitch, size - 1, nil)!
        let sfMono = Self.sfMono(size - 1) ?? monospaced
        let base = useMono ? Self.sfMono(size) ?? monospaced : system
        body = base
        bold = CTFontCreateCopyWithSymbolicTraits(base, size, nil, .traitBold, .traitBold) ?? base
        italic = CTFontCreateCopyWithSymbolicTraits(base, size, nil, .traitItalic, .traitItalic) ?? base
        boldItalic = CTFontCreateCopyWithSymbolicTraits(base, size, nil, [.traitBold, .traitItalic],
                                                        [.traitBold, .traitItalic]) ?? bold
        mono = sfMono
        name = Self.weighted(size - 1, 0.3)
        small = CTFontCreateUIFontForLanguage(.system, size - 2, nil)!
        smallItalic = CTFontCreateCopyWithSymbolicTraits(small, size - 2, nil, .traitItalic, .traitItalic) ?? small
        time = NSFont.monospacedDigitSystemFont(ofSize: size - 2, weight: .regular) as CTFont
        reaction = CTFontCreateUIFontForLanguage(.system, size - 1, nil)!
        emoji = CTFontCreateUIFontForLanguage(.system, size * 3, nil)!
    }

    static func sfMono(_ size: CGFloat) -> CTFont? {
        let font = CTFontCreateWithName("SFMono-Regular" as CFString, size, nil)
        return (CTFontCopyPostScriptName(font) as String) == "SFMono-Regular" ? font : nil
    }

    static func weighted(_ size: CGFloat, _ weight: CGFloat) -> CTFont {
        let traits = [kCTFontWeightTrait: weight] as CFDictionary
        let attributes = [kCTFontTraitsAttribute: traits, kCTFontFamilyNameAttribute: ".AppleSystemUIFont"] as CFDictionary
        let descriptor = CTFontDescriptorCreateWithAttributes(attributes)
        return CTFontCreateWithFontDescriptor(descriptor, size, nil)
    }
}
