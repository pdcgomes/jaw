import Foundation

/// WCAG's contrast on colours as `0xRRGGBB`, and the straight line the terminal's `theme.rs` moves a
/// colour along until it reads, ported as it is: `mix` rounds each channel half away from zero, as
/// Rust's `round` does, and `fit` halves the line 30 times, so the Mac lands on the terminal's colours.
public enum Contrast {
    /// WCAG's relative luminance of a colour.
    public static func luminance(_ c: UInt32) -> Double {
        func channel(_ shift: UInt32) -> Double {
            let v = Double((c >> shift) & 0xFF) / 255
            return v <= 0.039_28 ? v / 12.92 : pow((v + 0.055) / 1.055, 2.4)
        }
        return 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    /// WCAG's contrast between two colours, from 1 to 21.
    public static func ratio(_ a: UInt32, _ b: UInt32) -> Double {
        let (la, lb) = (luminance(a), luminance(b))
        return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)
    }

    /// `a` moved `t` of the way to `b`, channel by channel; past either end of the line a channel stops at
    /// 0 or 255, and a `t` that isn't a number gives 0, as Rust's cast and `min(255)` have it.
    public static func mix(_ a: UInt32, _ b: UInt32, _ t: Double) -> UInt32 {
        func channel(_ shift: UInt32) -> UInt32 {
            let (x, y) = (Double((a >> shift) & 0xFF), Double((b >> shift) & 0xFF))
            let v = (x + (y - x) * t).rounded()
            return (v.isNaN ? 0 : UInt32(min(max(v, 0), 255))) << shift
        }
        return channel(16) | channel(8) | channel(0)
    }

    /// `colour` as it is when its contrast with `ground` is from `low` to `high`, and otherwise moved
    /// toward `end`, if it reads too little, or toward `ground`, if too much, until its contrast is
    /// `target`, so it keeps its hue.
    public static func fit(_ colour: UInt32, on ground: UInt32, toward end: UInt32, low: Double, high: Double = .infinity,
                           target: Double) -> UInt32
    {
        let now = ratio(colour, ground)
        if now >= low, now <= high { return colour }
        let goal = now < low ? end : ground
        var (near, far) = (0.0, 1.0)
        for _ in 0 ..< 30 {
            let t = (near + far) / 2
            let reached = ratio(mix(colour, goal, t), ground)
            if (now < low && reached < target) || (now > high && reached > target) {
                near = t
            } else {
                far = t
            }
        }
        return mix(colour, goal, far)
    }
}
