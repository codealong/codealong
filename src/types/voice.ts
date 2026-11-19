// Voice: the meetings in your calendar and the ones kept (meetings.rs, ear.rs), sessions that
// listen to one (listen.rs), and the keys held to talk (dictation.rs). `[voice]` is in config.ts.

/** An event in your calendar with a time of day (ear.rs `Meeting`): times in milliseconds. */
export interface Meeting {
  /** Its id in the calendar with when it starts, so the occurrences of one that repeats differ. */
  id: string;
  title: string;
  start: number;
  end: number;
  /** The calendar it's in, by name, and by the id that stays when it's renamed. */
  calendar: string;
  calendarId?: string;
  /** The others in it, by name, or by address where the calendar has no name. */
  attendees: string[];
  /** What you answered in this calendar's copy of it; neither while you haven't. */
  declined?: boolean;
  accepted?: boolean;
  /** The link to its call. */
  link?: string;
  /** For one that repeats, what its occurrences share. */
  series?: string;
  /** Not from the calendar: one you had Codealong listen to as it began (Listen now), which has
   *  when it started and no end to go by. Missing for the calendar's. */
  unplanned?: boolean;
}

/** The meeting on offer (meetings.rs `Offer`): one about to start or just begun, to take its
 *  notes. `taking`: you took it, and its session is being started. */
export interface MeetingOffer extends Meeting {
  taking?: boolean;
}

/** A meeting kept among your meetings (meetings.rs `Kept`): what its folder says of it, the
 *  listening that heard it and the session that took its notes, and what's in it. */
export interface KeptMeeting extends Meeting {
  folder: string;
  /** The folder's name: when it was and what it was called. */
  name: string;
  listening: string;
  session: string;
  role: ListenRole;
  takenAt: number;
  /** Whether notes were kept of it, and how many lines were heard. */
  notes: boolean;
  lines: number;
}

/** A kept meeting's notes and transcript, as its files have them (meetings.rs `Files`). */
export interface MeetingFiles {
  notes: string | null;
  transcript: string | null;
}

/** Where your calendar stands (meetings.rs `Status`): whether it's read at all (`on`), whether
 *  Codealong may, the accounts it has calendars from, and how many meetings are in the day ahead
 *  with when the next starts. */
export interface CalendarStatus {
  on: boolean;
  access: string;
  sources: { title: string; kind: string }[];
  meetings: number;
  nextAt?: number;
  problem?: string;
  /** Where your meetings are kept, as a path from your home. */
  folder: string;
  /** Every calendar, by account then name, for Settings to offer: `id`, its name (`title`) and
   *  the account it's from (`source`). Missing from engines before they were listed. */
  calendars?: CalendarInfo[];
}

/** One of your calendars (ear.rs `CalendarInfo`). */
export interface CalendarInfo {
  id: string;
  title: string;
  source: string;
}

/** A meeting in the day ahead (meetings.rs `Ahead`): whether it can be taken now (it's going on,
 *  or about to), whether you answered its offer, and whether a session is hearing it, with the
 *  listening that does (`listening`, which its folder among your meetings names too). */
export interface MeetingAhead extends Meeting {
  takeable: boolean;
  answered?: boolean;
  heard?: boolean;
  listening?: string;
}

/** What a listening session makes of what it hears (listen.rs `Role`). */
export type ListenRole = 'notes' | 'coach' | 'requests' | 'custom';

/** What to listen to and what for (listen.rs `Ask`). */
export interface ListenAsk {
  mic: boolean;
  system: boolean;
  /** Only what these apps play, by bundle id, in place of everything the Mac plays. */
  apps: string[];
  role: ListenRole;
  /** For `custom`, what to do with what's heard; for the others, anything to add. */
  prompt: string;
  /** For `requests`: a session is started on each todo as it's filed. */
  start: boolean;
}

/** A listening as the board lists it, under its session's key (listen.rs `Glance`). */
export interface ListeningGlance {
  id: string;
  startedAt: number;
  role: ListenRole;
  mic: boolean;
  playback: boolean;
  apps?: string[];
  /** You're on speakers, as Codealong heard it to be (echo.rs): the microphone hears the others
   *  too, and what of You's repeats Them's is left out of the transcript. Missing while it isn't. */
  speakers?: boolean;
  transcript: string;
  notes: string;
  /** Something wrong that it goes on through. */
  problem?: string;
  /** Something it waits on that passes: the speech model downloading. */
  waiting?: string;
  /** The latest thing its session said that wasn't nothing, and when. */
  cue?: string;
  cueAt: number;
  /** It was started ahead of its session, which is still on its way (Listen now, a meeting just
   *  taken): what it's called until then, its meeting's name. The board lists it under no
   *  session's key, an empty one. Missing once its session has it. */
  starting?: string;
}

/** A listening as its file keeps it (listen.rs `Listening`). */
export interface Listening {
  id: string;
  session: string;
  /** What it hears from: who's taken to be speaking (`name`), and where the sound comes from,
   *  by `kind` with whatever that kind carries beside it (ear.rs `Source`, whose `From` is
   *  written flat). No window reads it. */
  sources: ({ name: string; kind: string } & Record<string, unknown>)[];
  role: ListenRole;
  /** What was asked of it (`ListenAsk.prompt`, `ListenAsk.start`), and the language it hears. */
  prompt: string;
  start?: boolean;
  locale: string;
  startedAt: number;
  endedAt?: number;
  whyEnded?: string;
  lines: number;
  told: number;
  /** When its session was last told, or last tried to be, and when the last line was said. */
  toldAt: number;
  heardAt: number;
  /** Tries to tell its session that failed, since the last that went through. */
  failures: number;
  done: boolean;
  /** The meeting it's of, when it was taken from the calendar, and the folder its transcript
   *  and notes are in when that isn't its own among Codealong's files. */
  meeting?: Meeting;
  folder?: string;
}

/** Where a session's listening stands (listen.rs `Tail`): what its session hasn't been told
 *  yet, `first` the number of the first of those lines, and its notes as they are. */
export interface ListeningTail {
  listening: Listening;
  first: number;
  untold: string[];
  notes?: string;
}

/** What a listening hears, as it's heard (listen.rs `Hearing`): the words being said (`who`,
 *  `text`, empty once settled), a line written to the transcript (`line`, `said`), or how far its
 *  session has been told (`told`). */
export interface Hearing {
  session: string;
  who?: string;
  text?: string;
  /** The words of `who` being said were the Mac's, heard through the microphone: they go at
   *  once, with no line to come for them (echo.rs `Repeats::shown`). */
  gone?: boolean;
  line?: number;
  said?: string;
  told?: number;
}

/** What this Mac can hear with (ear.rs `Status`), and the apps that record or play now. */
export interface HearingStatus {
  status: { available: boolean; why?: string; microphone: string; systemAudio?: string; locale?: string; installed: boolean };
  apps: { bundle: string; name: string; recording: boolean; playing: boolean }[];
  /** Where your calendar stands, as last read. Missing from older engines. */
  calendar?: CalendarStatus;
}

/** A key held to talk, as the notch is told of it (dictation.rs `Talk`). */
export interface Talk {
  id: number;
  phase: 'listening' | 'settling' | 'typed' | 'sent' | 'asked' | 'started' | 'nothing' | 'failed';
  to: 'looking' | 'orchestrator';
  title?: string;
  text: string;
  level: number;
  message?: string;
  /** Told to the window the words are for where there's no bar to show them, which says them at
   *  its foot instead (talk.ts `hear`). */
  standsIn?: boolean;
}

/** What a window is handed to put where its caret is (dictation.rs `Talked`): `id` is the press
 *  it was said in, as `Talk.id` has it; `key` when it's for one session's box whatever the window
 *  shows; `start` for the quick panel alone, when Shift was down as the key came up, which
 *  starts a session with it at once. */
export interface Talked {
  id?: number;
  text: string;
  key?: string;
  start?: boolean;
}
