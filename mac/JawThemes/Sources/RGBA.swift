import CoreGraphics
import Foundation

/// A colour in sRGB, the one form every role takes, so AppKit, Core Text and SwiftUI resolve it to
/// the same pixels.
public struct RGBA: Sendable, Hashable {
    public var r: Double
    public var g: Double
    public var b: Double
    public var a: Double

    public init(r: Double, g: Double, b: Double, a: Double = 1) {
        self.r = r
        self.g = g
        self.b = b
        self.a = a
    }

    /// From `0xRRGGBB`.
    public init(_ hex: UInt32, alpha: Double = 1) {
        self.init(r: Double((hex >> 16) & 0xFF) / 255, g: Double((hex >> 8) & 0xFF) / 255,
                  b: Double(hex & 0xFF) / 255, a: alpha)
    }

    /// As `0xRRGGBB`, its opacity left out.
    public var hex: UInt32 {
        func byte(_ v: Double) -> UInt32 {
            UInt32((min(max(v, 0), 1) * 255).rounded())
        }
        return byte(r) << 16 | byte(g) << 8 | byte(b)
    }

    public func opacity(_ a: Double) -> RGBA {
        RGBA(r: r, g: g, b: b, a: a)
    }

    /// This colour laid over `ground` at its own opacity, as one opaque colour.
    public func over(_ ground: RGBA) -> RGBA {
        RGBA(r: r * a + ground.r * (1 - a), g: g * a + ground.g * (1 - a), b: b * a + ground.b * (1 - a), a: 1)
    }

    /// Part of the way to `other`.
    public func mixed(with other: RGBA, _ amount: Double) -> RGBA {
        RGBA(r: r + (other.r - r) * amount, g: g + (other.g - g) * amount, b: b + (other.b - b) * amount,
             a: a + (other.a - a) * amount)
    }

    public var cgColor: CGColor {
        CGColor(srgbRed: r, green: g, blue: b, alpha: a)
    }

    /// WCAG's relative luminance.
    public var luminance: Double {
        func linear(_ c: Double) -> Double {
            c <= 0.03928 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
    }

    /// WCAG's contrast ratio against another colour, both taken as opaque.
    public func contrast(with other: RGBA) -> Double {
        let (x, y) = (luminance, other.luminance)
        return (max(x, y) + 0.05) / (min(x, y) + 0.05)
    }
}
