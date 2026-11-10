// codealong-ear: hears for Codealong. It captures audio (the microphone, what the Mac plays, or
// what some apps play), turns speech into text on this Mac with Apple's SpeechAnalyzer, and says
// what it heard as JSON lines. Codealong's engine starts one for each thing it listens to
// (src-tauri/src/ear.rs) and talks to it over stdin and stdout, as rpc.rs has every such child:
//
//   → {"id":1,"method":"start","params":{"locale":"en-US","sources":[{"name":"you","kind":"mic"}]}}
//   ← {"id":1,"result":{"locale":"en-US"}}
//   ← {"method":"partial","params":{"source":"you","text":"hello or"}}
//   ← {"method":"final","params":{"source":"you","text":"Hello, orchestrator.","at":1762700000.1,"until":1762700001.4}}
//   → {"id":2,"method":"stop"}
//   ← {"id":2,"result":{}}
//
// It's Swift, and a process of its own, because SpeechAnalyzer has no interface but Swift's, and
// so that audio that goes wrong takes down this and not the engine. No audio is written anywhere:
// what leaves this process is text. It ends when its stdin does, so it never outlives its engine.
//
// A source is one stream of audio with a name, transcribed by itself, so that who spoke is known
// by where the sound came from: the microphone is you, what the Mac plays is everyone else.
//
// What was found out about the system here (macOS 26.6, 2026-11-09) is in .claude/rules/voice.md.

import AVFoundation
import AppKit
import AudioToolbox
import CoreAudio
import Foundation
import Speech

// MARK: - Lines out

let lineLock = NSLock()

/// Writes one JSON line. A parent that's gone ends us: nobody is left to hear.
func writeLine(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.withoutEscapingSlashes]) else { return }
    lineLock.lock()
    defer { lineLock.unlock() }
    do {
        try FileHandle.standardOutput.write(contentsOf: data + Data([0x0a]))
    } catch {
        exit(0)
    }
}

func note(_ method: String, _ params: [String: Any]) {
    writeLine(["method": method, "params": params])
}

/// Why something asked for can't be done, with a word the engine can tell cases apart by
/// (`code`): what to say to the person depends on it.
struct Refusal: Error {
    let code: String
    let message: String
}

func words(_ error: Error) -> String {
    if let refusal = error as? Refusal { return refusal.message }
    return (error as NSError).localizedDescription
}

// MARK: - Audio as the transcriber takes it

/// One channel of 32-bit floats out of whatever a source gives: the first two channels averaged.
/// A device with more (a voice-processing input has up to nine) keeps its speech in the first,
/// and the rest aren't mixed in. Nil for a layout this doesn't read, which goes to the converter
/// as it is.
func mono(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
    let format = buffer.format
    let frames = Int(buffer.frameLength)
    let channels = Int(format.channelCount)
    guard format.commonFormat == .pcmFormatFloat32, channels > 0, frames > 0,
        let monoFormat = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: format.sampleRate, channels: 1, interleaved: false),
        let out = AVAudioPCMBuffer(pcmFormat: monoFormat, frameCapacity: AVAudioFrameCount(frames)),
        let to = out.floatChannelData?[0]
    else { return nil }
    out.frameLength = AVAudioFrameCount(frames)
    let mixed = channels == 2
    if format.isInterleaved {
        guard let raw = buffer.audioBufferList.pointee.mBuffers.mData else { return nil }
        let from = raw.assumingMemoryBound(to: Float.self)
        for i in 0..<frames {
            to[i] = mixed ? (from[i * channels] + from[i * channels + 1]) * 0.5 : from[i * channels]
        }
    } else {
        guard let from = buffer.floatChannelData else { return nil }
        for i in 0..<frames {
            to[i] = mixed ? (from[0][i] + from[1][i]) * 0.5 : from[0][i]
        }
    }
    return out
}

/// How loud a buffer of one float channel is, 0 to 1: its root mean square.
func level(_ buffer: AVAudioPCMBuffer) -> Double {
    guard let data = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return 0 }
    var sum = 0.0
    for i in 0..<Int(buffer.frameLength) {
        sum += Double(data[i]) * Double(data[i])
    }
    return min(1, (sum / Double(buffer.frameLength)).squareRoot())
}

// MARK: - One source's speech into text

/// The transcriber every source gets, and the one whose model is looked for and warmed.
func transcriber(_ locale: Locale) -> SpeechTranscriber {
    SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [.volatileResults, .fastResults], attributeOptions: [])
}

/// A source's audio into the transcriber, and its results out as lines. It takes audio from the
/// moment it's made, before the speech model is ready (`prepare`), and keeps it meanwhile: what's
/// said as a key goes down is heard, however long the model takes to load.
final class Pipeline: @unchecked Sendable {
    let name: String
    private let lock = NSLock()
    /// Audio heard before the model was ready, with when: at most `KEPT` seconds of it.
    private var waiting: [(buffer: AVAudioPCMBuffer, at: Double)] = []
    private var waitingSeconds = 0.0
    private static let kept = 120.0
    private var analyzer: SpeechAnalyzer?
    private var format: AVAudioFormat?
    private var input: AsyncStream<AnalyzerInput>.Continuation?
    private var results: Task<Void, Never>?
    private var converter: AVAudioConverter?
    private var convertsFrom: AVAudioFormat?
    /// Frames given to the analyzer so far, in its format: where its results' times count from.
    private var fed: Int64 = 0
    /// When some of those frames were heard, by the clock, one a second: a result's time in the
    /// audio becomes a time of day through the nearest before it, so a source that stops and
    /// starts (a device changed, nothing playing) doesn't drift.
    private var anchors: [(frame: Int64, at: Double)] = []
    private var lastLevel = 0.0
    private var loudest = 0.0
    private var finished = false
    private var unread = false

    init(name: String) {
        self.name = name
    }

    /// Gets the model ready and gives it what was heard meanwhile.
    func prepare(locale: Locale, hints: [String], failed: @escaping @Sendable (String) -> Void) async throws {
        let transcriber = transcriber(locale)
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            throw Refusal(code: "unavailable", message: "This Mac has no speech model for \(locale.identifier).")
        }
        let context = AnalysisContext()
        if !hints.isEmpty {
            context.contextualStrings[.general] = hints
        }
        let (stream, input) = AsyncStream<AnalyzerInput>.makeStream()
        let analyzer = SpeechAnalyzer(inputSequence: stream, modules: [transcriber], options: .init(priority: .userInitiated, modelRetention: .lingering), analysisContext: context)
        try await analyzer.prepareToAnalyze(in: format)
        let name = self.name
        let results = Task {
            do {
                for try await result in transcriber.results {
                    let text = String(result.text.characters).trimmingCharacters(in: .whitespacesAndNewlines)
                    if result.isFinal {
                        // The words still being made up belonged to this stretch, and are settled.
                        note("partial", ["source": name, "text": ""])
                        if !text.isEmpty {
                            let from = self.clock(result.range.start.seconds)
                            note("final", ["source": name, "text": text, "at": from, "until": max(from, self.clock(result.range.end.seconds))])
                        }
                    } else {
                        note("partial", ["source": name, "text": text])
                    }
                }
            } catch {
                if !self.isFinished { failed(words(error)) }
            }
        }
        adopt(analyzer, format, input, results)
    }

    /// Takes the model that's ready, and gives it what was heard while it loaded. Finished
    /// meanwhile, `finish` is waiting for this, and ends the input.
    private func adopt(_ analyzer: SpeechAnalyzer, _ format: AVAudioFormat, _ input: AsyncStream<AnalyzerInput>.Continuation, _ results: Task<Void, Never>) {
        lock.lock()
        defer { lock.unlock() }
        self.format = format
        self.analyzer = analyzer
        self.input = input
        self.results = results
        for (buffer, at) in waiting {
            give(buffer, heardAt: at)
        }
        waiting = []
    }

    private var isFinished: Bool {
        lock.lock()
        defer { lock.unlock() }
        return finished
    }

    /// Whether it was finished already.
    private func markFinished() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        let already = finished
        finished = true
        return already
    }

    /// The time of day a moment in the audio was heard at, in seconds since 1970.
    private func clock(_ seconds: Double) -> Double {
        lock.lock()
        defer { lock.unlock() }
        guard let rate = format?.sampleRate, let first = anchors.first else { return Date().timeIntervalSince1970 }
        let frame = Int64((seconds.isFinite ? seconds : 0) * rate)
        let anchor = anchors.last(where: { $0.frame <= frame }) ?? first
        return anchor.at + Double(frame - anchor.frame) / rate
    }

    /// Takes a buffer from the source, on whatever thread it captures on.
    func feed(_ buffer: AVAudioPCMBuffer) {
        guard buffer.frameLength > 0 else { return }
        let seconds = Double(buffer.frameLength) / buffer.format.sampleRate
        let heardAt = Date().timeIntervalSince1970 - seconds
        // Always a copy: a tap's buffer is the system's memory, gone when this returns.
        guard let single = mono(buffer) else {
            lock.lock()
            let said = unread
            unread = true
            lock.unlock()
            if !said { note("lost", ["source": name, "message": "Its audio comes in a format Codealong can't read."]) }
            return
        }
        lock.lock()
        defer { lock.unlock() }
        if finished { return }
        loudest = max(loudest, level(single))
        if heardAt - lastLevel >= 0.1 {
            note("level", ["source": name, "level": (loudest * 1000).rounded() / 1000])
            lastLevel = heardAt
            loudest = 0
        }
        if input == nil {
            waiting.append((single, heardAt))
            waitingSeconds += seconds
            while waitingSeconds > Pipeline.kept, let first = waiting.first {
                waitingSeconds -= Double(first.buffer.frameLength) / first.buffer.format.sampleRate
                waiting.removeFirst()
            }
            return
        }
        give(single, heardAt: heardAt)
    }

    /// One buffer to the analyzer. Called with the lock held.
    private func give(_ buffer: AVAudioPCMBuffer, heardAt: Double) {
        guard let converted = convert(buffer) else { return }
        if anchors.last.map({ heardAt - $0.at >= 1 }) ?? true {
            anchors.append((fed, heardAt))
            if anchors.count > 600 { anchors.removeFirst(anchors.count - 600) }
        }
        fed += Int64(converted.frameLength)
        input?.yield(AnalyzerInput(buffer: converted))
    }

    /// The buffer in the analyzer's format. One converter is kept for as long as the source's
    /// format stays, since changing the sample rate carries a little from each buffer to the next.
    private func convert(_ buffer: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        guard let format else { return nil }
        if buffer.format == format { return buffer }
        if convertsFrom != buffer.format {
            converter = AVAudioConverter(from: buffer.format, to: format)
            converter?.primeMethod = .none
            convertsFrom = buffer.format
        }
        guard let converter else { return nil }
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
        guard let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity) else { return nil }
        var given = false
        var error: NSError?
        let status = converter.convert(to: out, error: &error) { _, state in
            if given {
                state.pointee = .noDataNow
                return nil
            }
            given = true
            state.pointee = .haveData
            return buffer
        }
        guard status != .error, out.frameLength > 0 else { return nil }
        return out
    }

    private func parts() -> (SpeechAnalyzer?, AsyncStream<AnalyzerInput>.Continuation?, Task<Void, Never>?) {
        lock.lock()
        defer { lock.unlock() }
        return (analyzer, input, results)
    }

    /// Ends the audio and waits for the last words, which the analyzer settles as it finishes. It
    /// gets a few seconds: a release of the key isn't kept waiting on a model that's stuck. Call it
    /// once the source has stopped and `prepare` has returned or been given up on.
    func finish() async {
        if markFinished() { return }
        let (analyzer, input, results) = parts()
        guard let analyzer else { return }
        input?.finish()
        let done = Task {
            try? await analyzer.finalizeAndFinishThroughEndOfInput()
            await results?.value
        }
        let limit = Task {
            try? await Task.sleep(nanoseconds: 6_000_000_000)
            if Task.isCancelled { return }
            await analyzer.cancelAndFinishNow()
            results?.cancel()
        }
        await done.value
        limit.cancel()
    }
}

// MARK: - Sources

/// Something that captures audio and hands on its buffers until it's stopped.
protocol Capture: AnyObject {
    func start() throws
    func stop()
}

/// The microphone, through AVAudioEngine. When the Mac's audio changes under it (another input
/// picked, headphones plugged in) the engine stops, and this starts it again in the new format;
/// and since an engine can go quiet without saying so (the Mac slept, a device went away
/// mid-buffer), it's also started again whenever nothing has come from it for a few seconds.
final class Mic: Capture {
    private var engine = AVAudioEngine()
    private let cancelEcho: Bool
    private let sink: (AVAudioPCMBuffer) -> Void
    private let lost: (String) -> Void
    private var watching: NSObjectProtocol?
    private var timer: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "codealong.ear.mic")
    private var stopped = false
    private let lock = NSLock()
    private var lastHeard = Date()
    private var failures = 0

    init(cancelEcho: Bool, sink: @escaping (AVAudioPCMBuffer) -> Void, lost: @escaping (String) -> Void) {
        self.cancelEcho = cancelEcho
        self.sink = sink
        self.lost = lost
    }

    func start() throws {
        try queue.sync { try self.run() }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 2, repeating: 2)
        timer.setEventHandler { [weak self] in self?.look() }
        timer.resume()
        self.timer = timer
    }

    private func heard() {
        lock.lock()
        lastHeard = Date()
        lock.unlock()
    }

    private var quietFor: TimeInterval {
        lock.lock()
        defer { lock.unlock() }
        return Date().timeIntervalSince(lastHeard)
    }

    private func run() throws {
        if let watching { NotificationCenter.default.removeObserver(watching) }
        // A new engine each time: one that stopped under a changed device keeps its old format.
        engine = AVAudioEngine()
        let input = engine.inputNode
        if cancelEcho {
            // What the Mac plays is taken out of what the microphone hears, so that with
            // speakers the others in a meeting aren't heard twice, once as you. It would also
            // turn everything else down while it listens, which is told to do so as little as it can.
            try? input.setVoiceProcessingEnabled(true)
            input.voiceProcessingOtherAudioDuckingConfiguration = .init(enableAdvancedDucking: false, duckingLevel: .min)
        }
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0 else {
            throw Refusal(code: "no-microphone", message: "This Mac has no microphone to listen to.")
        }
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self] buffer, _ in
            self?.heard()
            self?.sink(buffer)
        }
        engine.prepare()
        try engine.start()
        heard()
        watching = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self] _ in
            self?.queue.async { self?.again() }
        }
    }

    private func look() {
        if !stopped && (!engine.isRunning || quietFor > 4) { again() }
    }

    /// Starts over. A microphone that stays away is said once, and looked for on.
    private func again() {
        if stopped { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        do {
            try run()
            failures = 0
        } catch {
            failures += 1
            if failures == 3 { lost(words(error)) }
        }
    }

    func stop() {
        timer?.cancel()
        queue.sync {
            stopped = true
            if let watching { NotificationCenter.default.removeObserver(watching) }
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
    }
}

/// A sound file, read as fast as it can be or at the speed it plays: what the checks listen to,
/// since they have no microphone.
final class FileSource: Capture {
    private let path: String
    private let paced: Bool
    private let sink: (AVAudioPCMBuffer) -> Void
    private let ended: () -> Void
    private var stopped = false
    private let lock = NSLock()

    init(path: String, paced: Bool, sink: @escaping (AVAudioPCMBuffer) -> Void, ended: @escaping () -> Void) {
        self.path = path
        self.paced = paced
        self.sink = sink
        self.ended = ended
    }

    func start() throws {
        let file: AVAudioFile
        do {
            file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
        } catch {
            throw Refusal(code: "no-file", message: "\(path) can't be read as audio: \(words(error))")
        }
        let format = file.processingFormat
        let chunk = AVAudioFrameCount(format.sampleRate / 10)
        Thread.detachNewThread { [self] in
            while !isStopped, let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: chunk) {
                do { try file.read(into: buffer, frameCount: chunk) } catch { break }
                if buffer.frameLength == 0 { break }
                sink(buffer)
                if paced { Thread.sleep(forTimeInterval: 0.1) }
            }
            if !isStopped { ended() }
        }
    }

    private var isStopped: Bool {
        lock.lock()
        defer { lock.unlock() }
        return stopped
    }

    func stop() {
        lock.lock()
        stopped = true
        lock.unlock()
    }
}

// MARK: - What the Mac plays

func address(_ selector: AudioObjectPropertySelector, _ scope: AudioObjectPropertyScope = kAudioObjectPropertyScopeGlobal) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress(mSelector: selector, mScope: scope, mElement: kAudioObjectPropertyElementMain)
}

func property<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector, _ start: T) -> T? {
    var at = address(selector)
    var size = UInt32(MemoryLayout<T>.size)
    var value = start
    let status = withUnsafeMutablePointer(to: &value) { AudioObjectGetPropertyData(object, &at, 0, nil, &size, $0) }
    return status == noErr ? value : nil
}

func text(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> String? {
    var at = address(selector)
    var size = UInt32(MemoryLayout<CFString?>.size)
    var value: Unmanaged<CFString>?
    let status = withUnsafeMutablePointer(to: &value) { AudioObjectGetPropertyData(object, &at, 0, nil, &size, $0) }
    guard status == noErr, let value else { return nil }
    return value.takeRetainedValue() as String
}

/// A process Core Audio knows: one that plays or records, or has.
struct AudioProcess {
    let object: AudioObjectID
    let pid: pid_t
    let bundle: String
    let recording: Bool
    let playing: Bool
}

func audioProcesses() -> [AudioProcess] {
    var at = address(kAudioHardwarePropertyProcessObjectList)
    var size: UInt32 = 0
    let system = AudioObjectID(kAudioObjectSystemObject)
    guard AudioObjectGetPropertyDataSize(system, &at, 0, nil, &size) == noErr, size > 0 else { return [] }
    var objects = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
    guard AudioObjectGetPropertyData(system, &at, 0, nil, &size, &objects) == noErr else { return [] }
    return objects.prefix(Int(size) / MemoryLayout<AudioObjectID>.size).compactMap { object in
        guard let pid = property(object, kAudioProcessPropertyPID, pid_t(0)), pid > 0 else { return nil }
        return AudioProcess(
            object: object, pid: pid, bundle: text(object, kAudioProcessPropertyBundleID) ?? "",
            recording: (property(object, kAudioProcessPropertyIsRunningInput, UInt32(0)) ?? 0) != 0,
            playing: (property(object, kAudioProcessPropertyIsRunningOutput, UInt32(0)) ?? 0) != 0)
    }
}

/// The process macOS holds responsible for another: the app a helper process works for (a
/// browser's audio comes from a helper of its own, Safari's from a WebKit process that isn't even
/// named after it). The call isn't in the headers; where it's missing a process answers for itself.
func responsible(for pid: pid_t) -> pid_t {
    typealias Call = @convention(c) (pid_t) -> pid_t
    guard let symbol = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid") else { return pid }
    let answer = unsafeBitCast(symbol, to: Call.self)(pid)
    return answer > 0 ? answer : pid
}

/// The app a process's audio is counted as: its bundle id and name.
func app(of process: AudioProcess) -> (bundle: String, name: String)? {
    for pid in [responsible(for: process.pid), process.pid] {
        if let app = NSRunningApplication(processIdentifier: pid), let bundle = app.bundleIdentifier {
            return (bundle, app.localizedName ?? bundle)
        }
    }
    return process.bundle.isEmpty ? nil : (process.bundle, process.bundle)
}

/// What the Mac plays, or what some apps play, through a Core Audio tap: a private device that
/// carries the tap's audio and nothing else, read on a queue of ours. Asking for one is what
/// makes macOS ask the person whether Codealong may record the Mac's audio; until they've said
/// yes the tap gives silence, and nothing says which it is.
final class Tap: Capture {
    /// The apps whose audio is wanted, by bundle id. Empty for everything the Mac plays.
    private let bundles: [String]
    private let sink: (AVAudioPCMBuffer) -> Void
    private let lost: (String) -> Void
    private let queue = DispatchQueue(label: "codealong.ear.tap")
    private var tap = AudioObjectID(kAudioObjectUnknown)
    private var device = AudioObjectID(kAudioObjectUnknown)
    private var proc: AudioDeviceIOProcID?
    private var tapped: [AudioObjectID] = []
    private var timer: DispatchSourceTimer?
    private var stopped = false

    init(bundles: [String], sink: @escaping (AVAudioPCMBuffer) -> Void, lost: @escaping (String) -> Void) {
        self.bundles = bundles
        self.sink = sink
        self.lost = lost
    }

    /// The processes to tap for the apps asked for: an app's own, and those it answers for.
    private func wanted() -> [AudioObjectID] {
        audioProcesses().filter { process in
            guard let app = app(of: process) else { return false }
            return bundles.contains { $0 == app.bundle || process.bundle == $0 || process.bundle.hasPrefix($0 + ".") }
        }.map(\.object).sorted()
    }

    func start() throws {
        try queue.sync { try self.build() }
        // An app's helpers come and go (a call started in another tab, a meeting's window opened
        // after the app), and a tap takes the processes it was made with. So the list is read
        // again every so often, and the tap rebuilt when it changed. The output device changing
        // (headphones) leaves the tap's device without a clock; the same rebuild mends it.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 2, repeating: 2)
        timer.setEventHandler { [weak self] in self?.look() }
        timer.resume()
        self.timer = timer
    }

    private var outputDevice: String? {
        guard let id = property(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice, AudioObjectID(0)) else { return nil }
        return text(id, kAudioDevicePropertyDeviceUID)
    }

    private var builtOn: String?
    /// When the tap last handed anything over, silence included. Read and set on `queue`.
    private var lastRead = Date()

    private func look() {
        if stopped { return }
        let processes = bundles.isEmpty ? [] : wanted()
        // Apps that play nothing yet have nothing to tap: looked for again next time.
        let idle = !bundles.isEmpty && processes.isEmpty
        // A tap that hands over nothing at all while something plays has lost its device without
        // the default changing (the Mac slept): the same rebuild.
        let stalled = device != kAudioObjectUnknown && Date().timeIntervalSince(lastRead) > 10 && audioProcesses().contains { $0.playing }
        if processes == tapped && outputDevice == builtOn && (device != kAudioObjectUnknown || idle) && !stalled { return }
        tearDown()
        do {
            try build()
        } catch {
            // Left for the next look: a device that's just gone is often back in a moment. Said
            // where the engine keeps what the helper last said, for when it never is.
            FileHandle.standardError.write(Data("\(words(error))\n".utf8))
        }
    }

    private func build() throws {
        let processes = bundles.isEmpty ? [] : wanted()
        tapped = processes
        builtOn = outputDevice
        // None of the apps asked for plays or records yet: nothing to tap until one does (`look`).
        if !bundles.isEmpty && processes.isEmpty { return }
        let description = bundles.isEmpty
            ? CATapDescription(stereoGlobalTapButExcludeProcesses: [])
            : CATapDescription(stereoMixdownOfProcesses: processes)
        description.uuid = UUID()
        description.name = "Codealong"
        description.isPrivate = true
        description.muteBehavior = .unmuted
        var tap = AudioObjectID(kAudioObjectUnknown)
        var status = AudioHardwareCreateProcessTap(description, &tap)
        guard status == noErr else {
            throw Refusal(code: "no-tap", message: "macOS wouldn't let the Mac's audio be heard (error \(status)).")
        }
        self.tap = tap
        guard var streamFormat = property(tap, kAudioTapPropertyFormat, AudioStreamBasicDescription()),
            let format = AVAudioFormat(streamDescription: &streamFormat)
        else {
            tearDown()
            throw Refusal(code: "no-tap", message: "The Mac's audio came in a format Codealong can't read.")
        }
        var aggregate: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Codealong",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: description.uuid.uuidString]],
        ]
        if let output = builtOn {
            aggregate[kAudioAggregateDeviceMainSubDeviceKey] = output
            aggregate[kAudioAggregateDeviceSubDeviceListKey] = [[kAudioSubDeviceUIDKey: output]]
        }
        var device = AudioObjectID(kAudioObjectUnknown)
        status = AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &device)
        guard status == noErr else {
            tearDown()
            throw Refusal(code: "no-tap", message: "The Mac's audio couldn't be read (error \(status)).")
        }
        self.device = device
        lastRead = Date()
        status = AudioDeviceCreateIOProcIDWithBlock(&proc, device, queue) { [weak self, sink] _, input, _, _, _ in
            self?.lastRead = Date()
            guard let buffer = AVAudioPCMBuffer(pcmFormat: format, bufferListNoCopy: input, deallocator: nil) else { return }
            sink(buffer)
        }
        if status == noErr { status = AudioDeviceStart(device, proc) }
        guard status == noErr else {
            tearDown()
            throw Refusal(code: "no-tap", message: "The Mac's audio couldn't be started (error \(status)).")
        }
    }

    private func tearDown() {
        if device != kAudioObjectUnknown {
            if let proc {
                AudioDeviceStop(device, proc)
                AudioDeviceDestroyIOProcID(device, proc)
            }
            AudioHardwareDestroyAggregateDevice(device)
        }
        if tap != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tap) }
        proc = nil
        device = AudioObjectID(kAudioObjectUnknown)
        tap = AudioObjectID(kAudioObjectUnknown)
    }

    func stop() {
        timer?.cancel()
        queue.sync {
            stopped = true
            tearDown()
        }
    }
}

// MARK: - Listening

/// Whether the person has let Codealong use the microphone, asking them when they haven't been.
func microphoneAllowed() async -> Bool {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: return true
    case .notDetermined: return await AVCaptureDevice.requestAccess(for: .audio)
    default: return false
    }
}

func microphoneState() -> String {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: return "allowed"
    case .notDetermined: return "unasked"
    case .denied: return "denied"
    default: return "restricted"
    }
}

/// The locale the transcriber has for the one asked for, with its model on this Mac: downloaded
/// first when it isn't, which is said once it's taken a moment.
func ready(_ wanted: String?) async throws -> Locale {
    guard SpeechTranscriber.isAvailable else {
        throw Refusal(code: "unavailable", message: "This Mac can't transcribe speech itself.")
    }
    let asked = wanted.map(Locale.init(identifier:)) ?? Locale.current
    guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: asked) else {
        throw Refusal(code: "no-locale", message: "macOS has no speech model for \(asked.identifier).")
    }
    let module = transcriber(locale)
    if await AssetInventory.status(forModules: [module]) != .installed {
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [module]) {
            // A model that's here already still answers with a request, done in an instant.
            let slow = Task {
                try await Task.sleep(nanoseconds: 1_500_000_000)
                note("state", ["state": "downloading", "message": "Downloading the speech model for \(locale.identifier)"])
            }
            defer { slow.cancel() }
            do {
                try await request.downloadAndInstall()
            } catch {
                throw Refusal(code: "no-model", message: "The speech model for \(locale.identifier) couldn't be downloaded: \(words(error))")
            }
        }
    }
    return locale
}

func refused(_ error: Error) -> [String: Any] {
    ["message": words(error), "code": (error as? Refusal)?.code ?? "failed"]
}

/// Waits for `task` to end, or for `seconds`, whichever comes first: a task that waits on a
/// download can't be hurried, and a stop isn't kept waiting on it.
func settled(_ task: Task<Void, Never>, within seconds: UInt64) async {
    final class Once: @unchecked Sendable {
        private let lock = NSLock()
        private var done = false
        func first() -> Bool {
            lock.lock()
            defer { lock.unlock() }
            if done { return false }
            done = true
            return true
        }
    }
    let once = Once()
    await withCheckedContinuation { (waiting: CheckedContinuation<Void, Never>) in
        Task {
            await task.value
            if once.first() { waiting.resume() }
        }
        Task {
            try? await Task.sleep(nanoseconds: seconds * 1_000_000_000)
            if once.first() { waiting.resume() }
        }
    }
}

/// Everything one `start` set going. `start` answers as soon as the sources are capturing; the
/// speech model is made ready after it (`ready` says so), and a model that can't be is `failed`,
/// after which nothing more is heard.
actor Listening {
    private var pipelines: [Pipeline] = []
    private var captures: [Capture] = []
    private var preparing: Task<Void, Never>?
    private var running = false

    func start(_ params: [String: Any]) async throws -> [String: Any] {
        if running { throw Refusal(code: "busy", message: "Already listening.") }
        guard let sources = params["sources"] as? [[String: Any]], !sources.isEmpty else {
            throw Refusal(code: "bad-request", message: "Nothing to listen to.")
        }
        if sources.contains(where: { $0["kind"] as? String == "mic" }), !(await microphoneAllowed()) {
            throw Refusal(code: "microphone-denied", message: "Codealong isn't allowed to use the microphone.")
        }
        if running { throw Refusal(code: "busy", message: "Already listening.") }
        running = true
        do {
            for source in sources {
                let name = source["name"] as? String ?? "audio"
                let pipeline = Pipeline(name: name)
                pipelines.append(pipeline)
                let sink: (AVAudioPCMBuffer) -> Void = { pipeline.feed($0) }
                let lost: (String) -> Void = { note("lost", ["source": name, "message": $0]) }
                let capture: Capture
                switch source["kind"] as? String ?? "" {
                case "mic":
                    capture = Mic(cancelEcho: source["cancel_echo"] as? Bool ?? false, sink: sink, lost: lost)
                case "system":
                    capture = Tap(bundles: [], sink: sink, lost: lost)
                case "apps":
                    let bundles = source["bundles"] as? [String] ?? []
                    if bundles.isEmpty { throw Refusal(code: "bad-request", message: "No app to listen to.") }
                    capture = Tap(bundles: bundles, sink: sink, lost: lost)
                case "file":
                    guard let path = source["path"] as? String else {
                        throw Refusal(code: "bad-request", message: "No file to listen to.")
                    }
                    capture = FileSource(path: path, paced: source["paced"] as? Bool ?? false, sink: sink) { [weak self] in
                        Task { await self?.ended(pipeline) }
                    }
                case let kind:
                    throw Refusal(code: "bad-request", message: "\(kind) isn't something to listen to.")
                }
                try capture.start()
                captures.append(capture)
            }
        } catch {
            await stop()
            throw error
        }
        let pipelines = self.pipelines
        let locale = params["locale"] as? String
        let hints = params["hints"] as? [String] ?? []
        preparing = Task {
            do {
                let locale = try await ready(locale)
                for pipeline in pipelines {
                    let name = pipeline.name
                    try await pipeline.prepare(locale: locale, hints: hints) { why in
                        note("lost", ["source": name, "message": why])
                    }
                }
                note("ready", ["locale": locale.identifier])
            } catch {
                note("failed", refused(error))
            }
        }
        return [:]
    }

    /// A source that ran out by itself (a file): its last words, then that it ended.
    private func ended(_ pipeline: Pipeline) async {
        await preparing?.value
        await pipeline.finish()
        note("ended", ["source": pipeline.name])
    }

    /// Stops capturing, and answers once the last words are out. A model still loading is waited
    /// for, for what was said meanwhile; one still downloading isn't.
    func stop() async {
        for capture in captures { capture.stop() }
        captures = []
        if let preparing {
            await settled(preparing, within: 15)
        }
        preparing = nil
        await withTaskGroup(of: Void.self) { group in
            for pipeline in pipelines {
                group.addTask { await pipeline.finish() }
            }
        }
        pipelines = []
        running = false
    }
}

/// Loads the speech model, so that the first words of the next thing said aren't kept waiting
/// on it: the system keeps a model a while after its last use.
func warm(_ params: [String: Any]) async throws -> [String: Any] {
    let locale = try await ready(params["locale"] as? String)
    let module = transcriber(locale)
    let analyzer = SpeechAnalyzer(modules: [module], options: .init(priority: .utility, modelRetention: .lingering))
    try await analyzer.prepareToAnalyze(in: await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [module]))
    await analyzer.cancelAndFinishNow()
    return ["locale": locale.identifier]
}

/// What this Mac can hear with, without asking anyone anything.
func status(_ params: [String: Any]) async -> [String: Any] {
    var answer: [String: Any] = ["available": SpeechTranscriber.isAvailable, "microphone": microphoneState()]
    let asked = (params["locale"] as? String).map(Locale.init(identifier:)) ?? Locale.current
    if SpeechTranscriber.isAvailable, let locale = await SpeechTranscriber.supportedLocale(equivalentTo: asked) {
        answer["locale"] = locale.identifier
        answer["installed"] = await SpeechTranscriber.installedLocales.contains { $0.identifier == locale.identifier }
    }
    return answer
}

/// The apps that record or play now, one line an app: what a meeting can be told from, and what
/// there is to pick from when only some apps should be heard.
func apps() -> [[String: Any]] {
    var found: [String: [String: Any]] = [:]
    let me = Bundle.main.bundleIdentifier
    for process in audioProcesses() where process.recording || process.playing {
        guard let app = app(of: process), app.bundle != me else { continue }
        var entry = found[app.bundle] ?? ["bundle": app.bundle, "name": app.name, "recording": false, "playing": false]
        if process.recording { entry["recording"] = true }
        if process.playing { entry["playing"] = true }
        found[app.bundle] = entry
    }
    return found.values.sorted { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
}

// MARK: - Lines in

signal(SIGPIPE, SIG_IGN)
let listening = Listening()

func answer(_ id: Any?, _ work: @escaping () async throws -> [String: Any]) {
    Task {
        do {
            let result = try await work()
            if let id { writeLine(["id": id, "result": result]) }
        } catch {
            let refusal = error as? Refusal
            if let id {
                writeLine(["id": id, "error": ["code": -32000, "message": words(error), "data": ["code": refusal?.code ?? "failed"]]])
            }
        }
    }
}

Thread.detachNewThread {
    while let line = readLine(strippingNewline: true) {
        guard let data = line.data(using: .utf8),
            let message = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let method = message["method"] as? String
        else { continue }
        let id = message["id"]
        let params = message["params"] as? [String: Any] ?? [:]
        switch method {
        case "start": answer(id) { try await listening.start(params) }
        case "stop": answer(id) { await listening.stop(); return [:] }
        case "status": answer(id) { await status(params) }
        case "warm": answer(id) { try await warm(params) }
        case "apps": answer(id) { ["apps": apps()] }
        default:
            if let id { writeLine(["id": id, "error": ["code": -32601, "message": "\(method)?"]]) }
        }
    }
    // Our engine is gone, or has closed our input to say it's done with us.
    Task {
        await listening.stop()
        exit(0)
    }
}

dispatchMain()
