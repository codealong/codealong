---
paths:
  - "src-tauri/ear/**"
  - "src-tauri/src/ear.rs"
  - "src-tauri/src/listen.rs"
  - "src-tauri/src/dictation.rs"
  - "scripts/ear.sh"
---

# Voice

What macOS's speech recognition and audio capture take and give, as codealong-ear
(`src-tauri/ear/main.swift`) relies on them: from the SDK's own interfaces (Xcode 26.5,
`Speech.framework/Modules/Speech.swiftmodule/*.swiftinterface`, `CoreAudio`'s
`AudioHardwareTapping.h` and `CATapDescription.h`) and runs on this Mac (macOS 26.6.2,
2026-11-09). What was run is marked so; the rest is read, not run, and listed under *Unchecked*.

## Speech

- SpeechAnalyzer and SpeechTranscriber (macOS 26) have a Swift interface only: actors and async
  sequences, nothing Objective-C can call, so objc2 can't reach them. That's why hearing is a
  helper process in Swift, and why the engine needs nothing of Swift to build.
- Run: a recording goes through `SpeechAnalyzer(inputSequence:modules:options:analysisContext:)`
  fed `AnalyzerInput(buffer:)` in the format `SpeechAnalyzer.bestAvailableAudioFormat(
  compatibleWith:)` names, with `reportingOptions: [.volatileResults, .fastResults]`. Results
  come from `transcriber.results`: several that aren't final for a stretch (`isFinal` false, each
  the whole stretch so far, about a second behind the audio), then one that is, punctuated and
  capitalized, and then the next stretch's begin. A stretch is a sentence or so.
- Run: the model loads once for the system, not for the process. The first helper after a while
  took 5.6 s from `start` to ready; one right after took 70 ms. `modelRetention: .lingering`
  asks the system to keep it; `ear::warm` loads it as the app launches. Audio fed before the
  model is ready isn't lost when it's kept and given after (`Pipeline.waiting`): a stop 60 ms
  after a start still transcribed all six seconds of a file.
- Run: `finalizeAndFinishThroughEndOfInput()` after the input sequence ends settles the last
  words in about 60 ms; the results sequence then ends.
- Run: `AssetInventory.status(forModules:)` said not installed for a locale
  `SpeechTranscriber.installedLocales` listed, and `assetInstallationRequest(supporting:)` gave a
  request whose `downloadAndInstall()` returned at once. So "downloading" is said only once that
  has taken a moment (`ready`).
- Run: no authorization prompt for speech recognition itself: SpeechAnalyzer isn't behind
  `SFSpeechRecognizer.requestAuthorization`, and the helper asks for none. Transcribing a file
  needs no permission at all.
- `AnalysisContext.contextualStrings[.general]` takes words the model wouldn't guess (project
  names). "main" was still heard as "Maine" in a text-to-speech sample without them.
- The helper is built with Swift 5's rules (`-swift-version 5`): under Swift 6's, handing an
  `AVAudioPCMBuffer` from an audio thread to the transcriber needs `Sendable` proofs AVFAudio's
  types don't give.

## The microphone and the Mac's audio

- The microphone is `AVAudioEngine`'s input node with a tap. macOS asks through
  `AVCaptureDevice.requestAccess(for: .audio)`; `authorizationStatus` says where it stands without
  asking. The prompt is attributed to the responsible process, which for a helper is whatever
  started it: Codealong's app, or its server (the app's own binary, run by launchd), so the
  usage text is `NSMicrophoneUsageDescription` in `src-tauri/Info.plist`, and a helper started
  from a terminal or an agent's shell asks as that terminal (Run: `status` from an agent's shell
  under Rex said `unasked`, for Rex).
- Under the hardened runtime (a Developer ID build) the microphone also needs the
  `com.apple.security.device.audio-input` entitlement. It's on the app and on the helper, which
  release.sh signs itself after Tauri's build, since the helper isn't Tauri's to bundle.
- What the Mac plays comes from a Core Audio process tap (macOS 14.2): `CATapDescription`
  (`stereoGlobalTapButExcludeProcesses: []` for everything, `stereoMixdownOfProcesses:` for some),
  `AudioHardwareCreateProcessTap`, then a private aggregate device with the tap in its
  `kAudioAggregateDeviceTapListKey`, read with `AudioDeviceCreateIOProcIDWithBlock`. The tap's
  format is `kAudioTapPropertyFormat`. It asks for *System Audio Recording Only* (Privacy &
  Security › Screen & System Audio Recording), with `NSAudioCaptureUsageDescription`; this is not
  Screen Recording, and puts no recording mark on the screen.
- A tap that isn't allowed gives silence, as one is given when nothing plays, and Core Audio
  says nothing of which. The privacy database does: `TCCAccessPreflight("kTCCServiceAudioCapture",
  nil)` in the private `TCC.framework` (found by `dlsym`; 0 allowed, 1 denied, 2 not asked yet),
  answered for the responsible app as the tap is judged, without asking anyone; and
  `TCCAccessRequest(service, nil, block)` puts the prompt up and calls the block with the answer.
  Run (macOS 26.6.2): from a shell under Rex the preflight said 2 for audio capture and for the
  microphone, where `AVCaptureDevice` said not determined, and 0 for screen capture, which Rex
  has; the request for screen capture called its block back with true and showed nothing. The
  helper asks when it's 2, tells the engine what it is as a tap starts and for as long as it
  isn't allowed (`permission`), and makes the tap again once it turns to allowed; `listen.rs` `silent` words what a listening says
  of it, and `codealong listen status` says it (`systemAudio`).
- First real meeting (2026-11-10, a video call, the microphone and the Mac's audio): every line was
  You for a quarter of an hour while the call's app played, and the tap's IO proc was being called all the
  while (a `sample` of the helper), so a tap that gives nothing still hands over buffers, of
  silence. That build couldn't say whether it was allowed.
- Run (2026-11-13, macOS 26.6.2, a Teams call in Chrome, then `say` recordings played aloud
  through a dock's USB output under the server's helper): voice processing on the microphone
  (`AVAudioInputNode.setVoiceProcessingEnabled(true)`, with
  `voiceProcessingOtherAudioDuckingConfiguration` at `.min`) leaves a tap made in the same
  process silent, its levels exactly 0, while macOS's preflight said allowed. The same
  recording was heard as Them with the tap alone, and with the microphone without voice
  processing; with it, neither source wrote a word. On the call it also turned things down: the
  others said the person sounded faint, and both their microphone and what they heard got louder
  the moment the listening stopped. So the helper doesn't use voice processing (main.swift `Mic`),
  and nothing is taken out of the microphone on speakers. Whether a tap in another process than
  the voice processing stays audible wasn't tried; the turning down rules it out anyway.
- The camera, run once (2026-11-13, macOS 26.6.2, Teams' Mac app opened while a listening heard
  the microphone and the Mac's audio): Teams showed its camera before joining. The camera went off
  as it joined the call and didn't come back until Teams was quit and opened again, by then with
  the listening stopped. The unified log puts the hang inside Teams: going from its preview to the
  call, its media stack's `AVCaptureSession stopRunning` never finished (no
  `AVCaptureSessionDidStopRunningNotification`, no input cleanup), though the camera daemon had
  stopped the stream. The helper never opened the camera (it was never a `VDCAssistant` client)
  and logged nothing while Teams hung. With the listening still on, Chrome used the same camera,
  switched it off and on in 0.2 s, and stopped it cleanly. The load average was about 800 on 10
  cores, and frames reached Teams up to 0.6 s late (33 ms apart), in the run that worked too. So
  nothing ties it to the helper, though one run can't rule it out; if it's seen again, compare a
  call with and without a listening. What to read: `log show` for `appleh13camerad` (`start
  streaming for client`, `StopStreaming`), `VDCAssistant` (`Client Connect for PID`) and the
  app's own `AVCaptureSession` lines.
- A tap takes the processes it was made with, by their Core Audio object ids
  (`kAudioHardwarePropertyProcessObjectList`, each with `kAudioProcessPropertyPID`,
  `…BundleID`, `…IsRunningInput`, `…IsRunningOutput`). An app's audio often comes from a helper
  of its own (a browser's, and Safari's from a WebKit process not named after it), so the app a
  process's audio counts as is the one macOS holds responsible for it
  (`responsibility_get_pid_responsible_for_pid`, by `dlsym`). Run: the list on this Mac named
  the iOS Simulator as playing and Core Speech as recording.

- Speakers or headphones (echo.rs): what the Mac plays through says only so much.
  `kAudioDevicePropertyTransportType` tells Bluetooth, HDMI, DisplayPort and AirPlay; the Mac's
  own output says which of its two it is in `kAudioDevicePropertyDataSource` on the output scope
  (`hdpn` for the jack); a USB sound card or a dock's jack says nothing of what's plugged into it
  (`outputKind`). Run (2026-11-10): this Mac's output, a dock's Realtek USB2.0 Audio, came out
  unknown. So it's heard instead, from the loudness the helper sends for each source (to five
  places, since the Mac heard through a microphone is down among the thousandths): tried on two
  `say` voices mixed as a call, the microphone's quiet while the Mac was loud came out 21, 5 and
  1.9 times its quiet while the Mac was silent on speakers, and 1.0 to 1.1 on headphones, typing
  throughout included, where a correlation of the two loudnesses didn't tell them apart (your
  own voice is most of what a microphone hears). What the Mac plays through is the default
  output (`kAudioHardwarePropertyDefaultOutputDevice`), not where its alerts go
  (`…DefaultSystemOutputDevice`), which stays put when headphones are picked for the rest. Run
  (2026-11-13): a `say` recording played aloud through the dock's speakers was judged speakers
  within its second play, with the microphone writing it down as You as well as the tap as
  Them; not yet run through a real call's turns.

## The calendar

- macOS's calendar is read through EventKit (`EKEventStore`), which has whichever accounts the
  person added in System Settings › Internet Accounts (a Google account's calendars come as a
  CalDAV source, named for the account), with no sign-in of ours. It asks with
  `requestFullAccessToEvents` (macOS 14) and `NSCalendarsFullAccessUsageDescription`, and under
  the hardened runtime needs `com.apple.security.personal-information.calendars`;
  `authorizationStatus(for: .event)` says where it stands without asking (`fullAccess`,
  `writeOnly`, which reads nothing, `denied`, `restricted`, `notDetermined`). Like the microphone
  it's asked as whatever started the helper. Run (macOS 26.6.2): from a shell under Rex the
  status was `notDetermined` and the store listed no sources and no calendars, so nothing of the
  calendar is seen before access is given; the Calendar app's own settings (`defaults read
  com.apple.iCal AccountDisplayOrder`) listed 18 accounts on this Mac, which is all a shell can
  say of it.
- An occurrence of an event that repeats has its series' identifiers, so `meetings.rs` tells
  them apart by one with when each starts. An event's link to its call is wherever its calendar
  put it: `url`, `location` or `notes` (`callLink`).
- Someone with several accounts on their Mac is often invited to a meeting at more than one of
  their addresses, and each account's calendar then has the event: seen 2026-11-11, where five
  of the eight meetings in the day ahead were listed two or three times. The copies are
  separate events to EventKit (`calendarItemIdentifier` differs); what they share is
  `calendarItemExternalIdentifier`, the UID their server gave the event, which is what a
  meeting's `id` is made from (`name(of:)` in main.swift) so that the engine counts it once.
- `EKParticipant.isCurrentUser` marks only the address of the account the event is in (Apple's
  header: "whether this participant represents the owner of this account"): in each copy of
  such a meeting the person's other addresses were ordinary guests, and the offer said the
  meeting was with themselves. Nothing public lists the person's addresses, so the engine
  gathers them from the marks themselves, across every event read (ear.rs `meetings`). A
  calendar's or an account's name is no evidence: on this Mac one of five accounts and five of
  eighteen calendars were named for an address, and a calendar someone else shares is named
  for theirs. Not seen: whether an event in a calendar shared by someone marks them or the
  account's owner; were it them, they'd be taken for the person.

## Unchecked

- The calendar was read for real on 2026-11-10, by the helper under the server once its user
  turned meetings on and answered macOS's prompt: five sources (three CalDAV, which is how
  Google's accounts come, the birthdays and a subscribed one) with eighteen calendars between
  them, and eleven meetings in the day ahead. A meeting was offered for real on 2026-11-11, its
  card under the notch a minute before it started. The same day, with the app built from
  c33d3d1e, the day ahead listed each meeting once and none of the person's own addresses among
  who else is in one, where the build before listed most two or three times: the copies of a
  Google event do share `calendarItemExternalIdentifier`, and each is marked `isCurrentUser`
  at its own account's address. Not yet seen: a meeting taken; and one you declined, which is
  read from the participant `isCurrentUser` marks, then from the addresses of yours in it.

- Not yet run for real (2026-11-11): Listen now, a listening started ahead of its session
  (listen.rs `start_ahead`): that two helpers hearing the microphone at once, the last meeting's
  and the next one's until its session shows, both hear; and how long a session takes to show,
  which is how long the transcript runs before anyone is told of it.
- The microphone ran for real on 2026-11-10, under the server (Codealong Dev was asked and
  allowed), and the tap on 2026-11-13, without voice processing (above). Not yet seen: the
  others on a real call written down as Them; whether making a tap asks the
  person by itself (the helper asks through `TCCAccessRequest` either way); whether a tap made
  before the person said yes hears once they have, or only one made after (the helper makes it
  again, which covers both), and whether coreaudiod needs a new process for it; nor that the
  entitlement is needed on the helper as well as the app, which is why both have it.
- Not yet run: an aggregate device across a change of output (headphones plugged in), sleep and
  wake, and whether a tap's IO proc is called while nothing plays. The helper rebuilds the tap
  when the default output or an app's processes change, and when nothing was handed over for ten
  seconds while something plays; the microphone's engine is made again when it stops, its
  configuration changes, or nothing comes from it for four seconds.
- Not yet run: an Intel Mac, where `SpeechTranscriber.isAvailable` may be false; the helper is
  built for both processors and says it can't transcribe where it can't.
