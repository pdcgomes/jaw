import ProjectDescription

/// jaw for the Mac: the client of jawd's protocol, jaw's theme, the AppKit lists and text, the app
/// and its harness, each a target here, as Redlamp's are. Swift 6 language mode, macOS 26, Apple
/// Silicon, warnings as errors.

let macOS = "26.0"
let team = "3JP75Z3F98"
let prefix = "dev.badcoffee.jaw"

let base: SettingsDictionary = [
    "DEVELOPMENT_TEAM": .string(team),
    // Tuist's `swiftVersion` doesn't set the language mode; this does.
    "SWIFT_VERSION": "6.0",
    "ARCHS": "arm64",
    "SWIFT_TREAT_WARNINGS_AS_ERRORS": "YES",
    "GCC_TREAT_WARNINGS_AS_ERRORS": "YES",
    // Built here signed to run locally; `make mac` signs releases with the Developer ID.
    "CODE_SIGN_STYLE": "Manual",
    "CODE_SIGN_IDENTITY": "-",
    "ENABLE_USER_SCRIPT_SANDBOXING": "YES",
]

let release: SettingsDictionary = [
    "SWIFT_COMPILATION_MODE": "wholemodule",
    "DEAD_CODE_STRIPPING": "YES",
]

func settings(_ extra: SettingsDictionary = [:]) -> Settings {
    .settings(
        base: base.merging(extra) { $1 },
        configurations: [
            // Debug stays attachable by Instruments, so the hardened runtime is off there.
            .debug(name: .debug, settings: ["ENABLE_HARDENED_RUNTIME": "NO"], xcconfig: "Version.xcconfig"),
            .release(
                name: .release,
                settings: release.merging(["ENABLE_HARDENED_RUNTIME": "YES"]) { $1 },
                xcconfig: "Version.xcconfig",
            ),
        ],
    )
}

func framework(_ name: String, _ dependencies: [TargetDependency] = [],
               resources: ResourceFileElements? = nil) -> Target
{
    .target(
        name: name,
        destinations: [.mac],
        product: .framework,
        bundleId: "\(prefix).\(name)",
        deploymentTargets: .macOS(macOS),
        sources: ["\(name)/Sources/**"],
        resources: resources,
        dependencies: dependencies,
        settings: settings(),
    )
}

func tests(_ name: String, _ dependencies: [TargetDependency]) -> Target {
    .target(
        name: "\(name)Tests",
        destinations: [.mac],
        product: .unitTests,
        bundleId: "\(prefix).\(name)Tests",
        deploymentTargets: .macOS(macOS),
        sources: ["Tests/\(name)Tests/**"],
        dependencies: dependencies,
        settings: settings(),
    )
}

/// The app's Info.plist in full: Tuist's default names a storyboard the bundle doesn't have.
func appInfo(name: String, extra: [String: Plist.Value] = [:]) -> InfoPlist {
    .dictionary([
        "CFBundleDevelopmentRegion": "en",
        "CFBundleExecutable": "$(EXECUTABLE_NAME)",
        "CFBundleIdentifier": "$(PRODUCT_BUNDLE_IDENTIFIER)",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": .string(name),
        "CFBundleDisplayName": .string(name),
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": "$(MARKETING_VERSION)",
        "CFBundleVersion": "$(CURRENT_PROJECT_VERSION)",
        "CFBundleIconName": "AppIcon",
        "LSMinimumSystemVersion": "$(MACOSX_DEPLOYMENT_TARGET)",
        "LSApplicationCategoryType": "public.app-category.social-networking",
        "NSPrincipalClass": "NSApplication",
        "NSHighResolutionCapable": true,
        "NSHumanReadableCopyright": "Jaw isn't affiliated with WhatsApp or Meta.",
    ].merging(extra) { $1 })
}

let project = Project(
    name: "Jaw",
    settings: .settings(configurations: [
        .debug(name: .debug, xcconfig: "Version.xcconfig"),
        .release(name: .release, xcconfig: "Version.xcconfig"),
    ]),
    targets: [
        framework("JawKit"),
        framework("JawThemes"),
        // The Mimic's pixels, from the terminal's own copy.
        framework("JawUI", [.target(name: "JawKit"), .target(name: "JawThemes")],
                  resources: ["../creatures/mimic.png"]),
        .target(
            name: "jaw",
            destinations: [.mac],
            product: .app,
            productName: "Jaw",
            bundleId: prefix,
            deploymentTargets: .macOS(macOS),
            infoPlist: appInfo(name: "Jaw"),
            sources: ["JawApp/Sources/**"],
            resources: ["JawApp/Resources/**"],
            dependencies: [.target(name: "JawKit"), .target(name: "JawThemes"), .target(name: "JawUI")],
            settings: settings([
                "ASSETCATALOG_COMPILER_APPICON_NAME": "AppIcon",
                // Controls follow the user's system accent where the theme doesn't set one.
                "ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME": "",
            ]),
        ),
        .target(
            name: "JawHarness",
            destinations: [.mac],
            product: .app,
            bundleId: "\(prefix).harness",
            deploymentTargets: .macOS(macOS),
            infoPlist: appInfo(name: "Jaw Harness"),
            sources: ["JawHarness/Sources/**"],
            // The app's icon, and the protocol's fixtures, which the scenes decode as jawd sends them.
            resources: ["JawApp/Resources/**", "../protocol/fixtures/*.json"],
            dependencies: [.target(name: "JawKit"), .target(name: "JawThemes"), .target(name: "JawUI")],
            settings: settings([
                "ASSETCATALOG_COMPILER_APPICON_NAME": "AppIcon",
                // Controls follow the user's system accent where the theme doesn't set one.
                "ASSETCATALOG_COMPILER_GLOBAL_ACCENT_COLOR_NAME": "",
            ]),
        ),
        tests("JawKit", [.target(name: "JawKit")]),
        tests("JawThemes", [.target(name: "JawThemes")]),
        tests("JawUI", [.target(name: "JawUI"), .target(name: "JawKit"), .target(name: "JawThemes")]),
        // Hosted by the harness, so its scenes, their fixtures and its pictures are tested as it runs them.
        tests("JawHarness", [.target(name: "JawHarness"), .target(name: "JawKit"), .target(name: "JawThemes"),
                             .target(name: "JawUI")]),
    ],
    schemes: [
        // Tuist tests JawHarnessTests in JawHarness's scheme; make mac-test runs them in their own.
        .scheme(
            name: "JawHarnessTests",
            buildAction: .buildAction(targets: ["JawHarnessTests"]),
            testAction: .targets(["JawHarnessTests"]),
        ),
    ],
)
