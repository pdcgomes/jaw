import Foundation

/// Times, days and lengths as the terminal's `fmt.rs` says them, in the Mac's time zone.
public enum Format {
    static let minute: Int64 = 60000
    static let day: Int64 = 24 * 60 * minute

    static func wall(_ at: Int64, _ tz: TimeZone) -> Int64 {
        at + Int64(tz.secondsFromGMT(for: Date(timeIntervalSince1970: Double(at) / 1000))) * 1000
    }

    static func daysBefore(_ at: Int64, _ now: Int64, _ tz: TimeZone) -> Int64 {
        floorDiv(wall(now, tz), day) - floorDiv(wall(at, tz), day)
    }

    static func floorDiv(_ a: Int64, _ b: Int64) -> Int64 {
        a >= 0 ? a / b : (a - b + 1) / b
    }

    public static func sameMinute(_ a: Int64, _ b: Int64, tz: TimeZone = .current) -> Bool {
        floorDiv(wall(a, tz), minute) == floorDiv(wall(b, tz), minute)
    }

    public static func sameDay(_ a: Int64, _ b: Int64, tz: TimeZone = .current) -> Bool {
        daysBefore(a, b, tz) == 0
    }

    /// `HH:MM`.
    public static func clock(_ at: Int64, tz: TimeZone = .current) -> String {
        let minutes = (wall(at, tz) % day + day) % day / minute
        return String(format: "%02d:%02d", minutes / 60, minutes % 60)
    }

    static let weekdays = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"]

    static func weekday(_ at: Int64, _ tz: TimeZone) -> String {
        weekdays[Int((floorDiv(wall(at, tz), day) % 7 + 7) % 7)]
    }

    static func formatted(_ at: Int64, _ format: String, _ tz: TimeZone) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_GB")
        f.timeZone = tz
        f.dateFormat = format
        return f.string(from: Date(timeIntervalSince1970: Double(at) / 1000))
    }

    static func year(_ at: Int64, _ tz: TimeZone) -> Int {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = tz
        return calendar.component(.year, from: Date(timeIntervalSince1970: Double(at) / 1000))
    }

    /// A day separator: Today, Yesterday, a weekday within the week, then the date.
    public static func day(_ at: Int64, now: Int64, tz: TimeZone = .current) -> String {
        switch daysBefore(at, now, tz) {
        case 0: "Today"
        case 1: "Yesterday"
        case 2 ... 6: weekday(at, tz)
        default: year(at, tz) == year(now, tz) ? formatted(at, "EEEE d MMMM", tz) : formatted(at, "d MMMM yyyy", tz)
        }
    }

    /// A chat's time in the list: the clock today, then yesterday, a weekday's first three letters,
    /// the date.
    public static func when(_ at: Int64, now: Int64, tz: TimeZone = .current) -> String {
        switch daysBefore(at, now, tz) {
        case 0: clock(at, tz: tz)
        case 1: "yesterday"
        case 2 ... 6: String(weekday(at, tz).prefix(3))
        default: year(at, tz) == year(now, tz) ? formatted(at, "d MMM", tz) : formatted(at, "d MMM yyyy", tz)
        }
    }

    public static func ago(_ at: Int64, now: Int64, tz: TimeZone = .current) -> String {
        let minutes = max(now - at, 0) / minute
        switch minutes {
        case 0: return "just now"
        case 1 ... 59: return "\(minutes) min ago"
        default: return daysBefore(at, now, tz) == 0 ? "\(minutes / 60) h ago" : when(at, now: now, tz: tz)
        }
    }

    /// A message's whole time, for its info.
    public static func full(_ at: Int64, tz: TimeZone = .current) -> String {
        formatted(at, "EEEE d MMMM yyyy, HH:mm:ss", tz)
    }

    /// Until when, as a ban says it.
    public static func until(_ at: Int64, tz: TimeZone = .current) -> String {
        formatted(at, "d MMMM, HH:mm", tz)
    }

    /// `1,204`.
    public static func num<N: BinaryInteger>(_ n: N) -> String {
        let s = String(n)
        var out = ""
        for (i, c) in s.enumerated() {
            if i > 0, (s.count - i) % 3 == 0 { out.append(",") }
            out.append(c)
        }
        return out
    }

    /// `0:14`, or `1:02:03`.
    public static func length(_ seconds: Double) -> String {
        let s = Int(max(seconds, 0))
        return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, s / 60 % 60, s % 60)
            : String(format: "%d:%02d", s / 60, s % 60)
    }

    public static func now() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }
}
