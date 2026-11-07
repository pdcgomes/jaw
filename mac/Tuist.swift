import ProjectDescription

let config = Config(
    project: .tuist(
        compatibleXcodeVersions: .list([.upToNextMajor("26.0"), .upToNextMajor("27.0")]),
        swiftVersion: "6.2",
        generationOptions: .options(),
    ),
)
