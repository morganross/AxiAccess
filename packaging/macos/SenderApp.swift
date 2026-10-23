import AppKit
import ServiceManagement

// This UI configures AxiAccess itself. It never drives another application's UI.
final class SenderApp: NSObject, NSApplicationDelegate, NSTextFieldDelegate {
    private var window: NSWindow!
    private var stateLabel: NSTextField!
    private var startup: NSButton!
    private var endpoint: NSTextField!
    private var sender: Process?
    private var output: Pipe?
    private var logText = ""
    private var busy = false
    private var endpointHasEdits = false
    private let queue = DispatchQueue(label: "com.axiowl.axiaccess.management")
    private var binary: URL { Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/axiaccess") }
    private var config: URL { FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/AxiAccess/sender.json") }
    private let loginLabel = "com.axiowl.axiaccess.sender"

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 620, height: 410), styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "AxiAccess Sender"
        window.center()
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false
        let title = NSTextField(labelWithString: "Read this desktop from AxiOwl IDE")
        title.font = .boldSystemFont(ofSize: 20)
        let explanation = NSTextField(wrappingLabelWithString: "AxiAccess reads app windows and accessibility text. It cannot click, type, focus, scroll or change observed apps. Connections require your sender credential and certificate trust.")
        stateLabel = NSTextField(wrappingLabelWithString: "Stopped. Start creates private sender configuration if needed. No existing credentials or chats are modified.")
        let endpointLabel = NSTextField(labelWithString: "Listen address (loopback by default; set a LAN/VM address for direct TLS):")
        endpoint = NSTextField(string: "127.0.0.1:8765")
        endpoint.placeholderString = "127.0.0.1:8765"
        endpoint.delegate = self
        let controls = NSStackView()
        controls.orientation = .horizontal
        for (name, selector) in [("Start sender", #selector(startSender)), ("Stop sender", #selector(stopSender)), ("Accessibility settings", #selector(openAccessibility))] {
            controls.addArrangedSubview(NSButton(title: name, target: self, action: selector))
        }
        startup = NSButton(checkboxWithTitle: "Start sender when I log in", target: self, action: #selector(setStartup))
        startup.state = SMAppService.agent(plistName: "com.axiowl.axiaccess.sender.plist").status == .enabled ? .on : .off
        let configButton = NSButton(title: "Show private configuration folder", target: self, action: #selector(showConfig))
        for view in [title, explanation, endpointLabel, endpoint!, stateLabel!, controls, startup!, configButton] { stack.addArrangedSubview(view) }
        let content = window.contentView!
        content.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 24), stack.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -24), stack.topAnchor.constraint(equalTo: content.topAnchor, constant: 24)])
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        loadStoredEndpoint()
    }

    func controlTextDidChange(_ notification: Notification) {
        if let field = notification.object as? NSTextField, field === endpoint { endpointHasEdits = true }
    }
    // Management commands own config validation and atomic replacement. The UI
    // never reads a private config or serializes pairing keys/tokens itself.
    private func management(_ arguments: [String], publicOutput: Bool = false) throws -> Data {
        let process = Process()
        process.executableURL = binary
        process.arguments = arguments + ["--config", config.path]
        let pipe = publicOutput ? Pipe() : nil
        if let pipe { process.standardOutput = pipe } else { process.standardOutput = FileHandle.nullDevice }
        process.standardError = FileHandle.nullDevice
        try process.run()
        let data = pipe?.fileHandleForReading.readDataToEndOfFile() ?? Data()
        process.waitUntilExit()
        guard process.terminationStatus == 0 && data.count <= 262144 else {
            throw NSError(domain: "AxiAccess", code: Int(process.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "AxiAccess configuration failed. Existing identity and credentials were retained; check the installed CLI status."])
        }
        return data
    }
    private func persistListen(_ listen: String) throws {
        _ = try management(["configure", "--listen", listen])
    }
    private func loadStoredEndpoint() {
        guard FileManager.default.fileExists(atPath: config.path) else { return }
        busy = true
        stateLabel.stringValue = "Loading the saved sender address…"
        queue.async {
            do {
                let data = try self.management(["status"], publicOutput: true)
                let value = try JSONSerialization.jsonObject(with: data) as? [String: Any]
                let listen = value?["listen"] as? String
                DispatchQueue.main.async {
                    self.busy = false
                    if !self.endpointHasEdits, let listen { self.endpoint.stringValue = listen }
                    self.stateLabel.stringValue = "Saved configuration loaded. This does not claim that a sender is running or Accessibility permission is granted."
                }
            } catch { DispatchQueue.main.async { self.busy = false; self.stateLabel.stringValue = error.localizedDescription } }
        }
    }

    @objc private func startSender() {
        if busy || sender?.isRunning == true { return }
        if SMAppService.agent(plistName: "com.axiowl.axiaccess.sender.plist").status == .enabled {
            stateLabel.stringValue = "Login startup is enabled. This window will not start a second sender; turn off login startup to use a window-owned process."
            return
        }
        let listen = endpoint.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !listen.isEmpty && listen.utf8.count < 256 else { stateLabel.stringValue = "Enter an IP address and port, for example 127.0.0.1:8765."; return }
        busy = true
        stateLabel.stringValue = "Preparing private AxiAccess sender…"
        queue.async {
            do {
                if !FileManager.default.fileExists(atPath: self.config.path) {
                    let setup = Process()
                    setup.executableURL = self.binary
                    setup.arguments = ["setup", "--quiet", "--config", self.config.path]
                    // Setup can print pairing credentials: never place its output
                    // in logs, diagnostic text, or system notifications.
                    setup.standardOutput = FileHandle.nullDevice
                    setup.standardError = FileHandle.nullDevice
                    try setup.run()
                    setup.waitUntilExit()
                    guard setup.terminationStatus == 0 else { throw NSError(domain: "AxiAccess", code: Int(setup.terminationStatus), userInfo: [NSLocalizedDescriptionKey: "Private sender setup failed. Use the installed CLI for its typed status."]) }
                }
                try self.persistListen(listen)
                DispatchQueue.main.async {
                    self.busy = false
                    do {
                        let process = Process()
                        process.executableURL = self.binary
                        process.arguments = ["sender", "--config", self.config.path]
                        process.standardOutput = FileHandle.nullDevice
                        process.standardError = FileHandle.nullDevice
                        process.terminationHandler = { process in DispatchQueue.main.async {
                            if self.sender === process { self.sender = nil; self.stateLabel.stringValue = "Sender stopped (exit \(process.terminationStatus)). Existing configuration is retained." }
                        } }
                        try process.run()
                        self.sender = process
                        self.stateLabel.stringValue = "Sender process started. Connections and reads require authorization. macOS may require Accessibility permission for this installed app."
                    } catch { self.stateLabel.stringValue = error.localizedDescription }
                }
            } catch { DispatchQueue.main.async { self.busy = false; self.stateLabel.stringValue = error.localizedDescription } }
        }
    }
    @objc private func stopSender() {
        if let process = sender, process.isRunning { process.terminate() }
        else { stateLabel.stringValue = "This window owns no running sender. Login-started senders are managed by the login option below." }
    }
    @objc private func setStartup() {
        let enable = startup.state == .on
        if enable && !FileManager.default.fileExists(atPath: config.path) {
            startup.state = .off
            stateLabel.stringValue = "Start sender once to create its private configuration, then enable login startup."
            return
        }
        if !enable { applyStartup(enable: false); return }
        let listen = endpoint.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        let owned = sender
        busy = true
        startup.isEnabled = false
        stateLabel.stringValue = "Saving the sender address before enabling login startup…"
        if let owned, owned.isRunning { owned.terminate() }
        queue.async {
            if let owned, owned.isRunning { owned.waitUntilExit() }
            do {
                try self.persistListen(listen)
                DispatchQueue.main.async { self.busy = false; self.startup.isEnabled = true; self.applyStartup(enable: true) }
            } catch { DispatchQueue.main.async { self.busy = false; self.startup.isEnabled = true; self.startup.state = .off; self.stateLabel.stringValue = error.localizedDescription } }
        }
    }
    private func applyStartup(enable: Bool) {
        let service = SMAppService.agent(plistName: "com.axiowl.axiaccess.sender.plist")
        do {
            if enable { try service.register(); stateLabel.stringValue = "Login startup registered for this user. Configure the sender first; macOS may require approval under Login Items." }
            else { try service.unregister(); stateLabel.stringValue = "Login startup disabled. Private pairing/configuration is retained." }
        } catch { stateLabel.stringValue = error.localizedDescription; startup.state = service.status == .enabled ? .on : .off }
    }
    @objc private func openAccessibility() { if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") { NSWorkspace.shared.open(url) } }
    @objc private func showConfig() { NSWorkspace.shared.activateFileViewerSelecting([config.deletingLastPathComponent()]) }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
    func applicationWillTerminate(_ notification: Notification) { if let process = sender, process.isRunning { process.terminate() } }
}
if CommandLine.arguments.contains("--disable-login") {
    let service = SMAppService.agent(plistName: "com.axiowl.axiaccess.sender.plist")
    if service.status == .notRegistered || service.status == .notFound { exit(0) }
    do { try service.unregister(); exit(0) }
    catch { fputs("AxiAccess login item could not be unregistered. Disable it in the sender app or macOS Login Items.\n", stderr); exit(1) }
}
let app = NSApplication.shared
let delegate = SenderApp()
app.delegate = delegate
app.run()
