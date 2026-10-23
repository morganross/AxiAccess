import Foundation

// Native user-session helper used by ServiceManagement; no shell expansion or
// other user's HOME is involved. Setup is explicit in the sender application.
let own = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
let binary = own.deletingLastPathComponent().appendingPathComponent("axiaccess")
let config = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/AxiAccess/sender.json")
guard FileManager.default.fileExists(atPath: config.path) else { exit(78) }
let process = Process()
process.executableURL = binary
let listen = UserDefaults(suiteName: "com.axiowl.axiaccess.sender")?.string(forKey: "listenAddress") ?? "127.0.0.1:8765"
process.arguments = ["sender", "--config", config.path, "--listen", listen]
process.standardOutput = FileHandle.nullDevice
process.standardError = FileHandle.nullDevice
do { try process.run(); process.waitUntilExit(); exit(process.terminationStatus) }
catch { exit(1) }
