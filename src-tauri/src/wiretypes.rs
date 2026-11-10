//! Holds `src/types.ts` to the engine's model, by name.
//!
//! The windows' types are written by hand to mirror what serde writes, with the comments the
//! windows need, and nothing else says when a field is added, dropped or renamed on one side
//! only. A field the engine sends that no interface declares is one a window can't read without
//! a cast; a member the engine never sends is always undefined, whatever its type says. Here
//! each struct of the model is paired with the interface that mirrors it (`pairs`), and the two
//! must name the same fields but for what's listed as apart on purpose (`APART`), which has to
//! stay true: an entry that no longer differs fails too.
//!
//! Every struct in the model's files (`FILES`) is paired or said to have no interface
//! (`UNMIRRORED`), so a new one has to say which. Names only: what type each field is, and
//! whether it may be missing, is still kept in step by hand.
//!
//! A struct's names are the ones serde reads it by (`names`), which are the ones it writes only
//! while nothing in the model's files names a field one way alone: an alias, a field skipped or
//! renamed in one direction, a flattened one. `misnames` says which lines would, and a test
//! holds the files to none. An interface is read from types.ts by a small reader of its own
//! (`Ts`), which fails on a parent it can't read rather than taking it to have no members.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::{self, DeserializeOwned, Deserializer, Visitor};

use crate::model;

const TYPES: &str = include_str!("../../src/types.ts");

/// The files the board's model is in.
const FILES: [(&str, &str); 2] = [("peer/src/model.rs", include_str!("../peer/src/model.rs")), ("src/model.rs", include_str!("model.rs"))];

/// A struct and the interface that mirrors it.
struct Pair {
    rust: &'static str,
    ts: &'static str,
    /// The names serde knows its fields by (`names`).
    names: BTreeSet<&'static str>,
}

fn pair<T: DeserializeOwned>(ts: &'static str) -> Pair {
    let rust = std::any::type_name::<T>().rsplit("::").next().unwrap_or_default();
    Pair { rust, ts, names: names::<T>().unwrap_or_else(|| panic!("{rust} doesn't read as a struct with named fields")) }
}

/// The model's structs, each with the interface of types.ts that mirrors it: most by the same
/// name.
fn pairs() -> Vec<Pair> {
    macro_rules! same {
        ($($t:ident),* $(,)?) => { vec![$(pair::<model::$t>(stringify!($t))),*] };
    }
    same![
        Session, Emblem, Artifact, Helper, SessionMode, ContextUse, QueuedMessage, SessionLimit, ConnectionLost, Suggestions, SideAnswer, Raised, AgentSeen, RunningIn, SessionOrigin, ChromeTabs,
        SessionInsight, Checks, Label, PrLabel, Pr, StaleApproval, GitInfo, Target, Lane, Cleanup, Preview, PreviewServer, DeviceTag, Meter, Usage, ProjectSummary, AccountInfo, Notice, Board,
        TodoHint, PrItem, LastReview, AutoReview, ReviewPart, PrSession,
    ]
}

/// The model's structs no interface mirrors, each with why.
const UNMIRRORED: [(&str, &str); 2] = [
    ("RepoSummary", "typed in place, as `ProjectSummary.repos`"),
    ("AppNames", "typed in place, as `Board.apps`"),
];

/// What a struct and its interface name differently on purpose: the struct, what only the
/// engine names, what only the windows name, and why.
const APART: [(&str, &[&str], &[&str], &str); 1] =
    [("Target", &[], &["image"], "an editor's own icon, which the windows put on its target from the icons they ask for apart (src/editors.ts)")];

/// The names serde reads a struct's fields by, as its derive hands them to a deserializer:
/// renamed as `rename_all` and `rename` have them, without what's skipped. None for what isn't
/// a plain struct (one with a flattened field reads as a map, which says no names). They're the
/// names it writes too only where no field is named one way alone (`misnames`).
fn names<T: DeserializeOwned>() -> Option<BTreeSet<&'static str>> {
    struct Names<'a>(&'a mut Option<&'static [&'static str]>);
    impl<'de> Deserializer<'de> for Names<'_> {
        type Error = de::value::Error;
        fn deserialize_any<V: Visitor<'de>>(self, _: V) -> Result<V::Value, Self::Error> {
            Err(de::Error::custom("not a struct"))
        }
        fn deserialize_struct<V: Visitor<'de>>(self, _: &'static str, fields: &'static [&'static str], _: V) -> Result<V::Value, Self::Error> {
            *self.0 = Some(fields);
            Err(de::Error::custom("only its names were asked for"))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
        }
    }
    let mut got = None;
    let _ = T::deserialize(Names(&mut got));
    got.map(|fields| fields.iter().copied().collect())
}

/// Why a line of Rust gives a field a name serde reads and doesn't write, or the other way
/// round, so that `names` wouldn't be what the windows are sent; None for a line that doesn't.
fn misnames(line: &str) -> Option<&'static str> {
    let line = line.trim();
    if !line.starts_with("#[") || !line.contains("serde(") {
        return None;
    }
    let skipped_one_way = line.match_indices("skip_serializing").any(|(at, word)| !line[at + word.len()..].starts_with("_if"));
    if line.contains("alias") {
        Some("an alias is a name read and never written")
    } else if line.contains("flatten") {
        Some("a flattened field's names aren't said to what reads it")
    } else if skipped_one_way {
        Some("a field skipped only as it's written is read and never sent")
    } else if line.contains("skip_deserializing") {
        Some("a field skipped only as it's read is sent and never named here")
    } else if line.contains("rename(") || line.contains("rename_all(") {
        Some("a name given one way only is read as one and written as another")
    } else if line.contains("from =") || line.contains("try_from") || line.contains("into =") {
        Some("a type read or written as another has that one's fields")
    } else {
        None
    }
}

fn is_quote(c: char) -> bool {
    matches!(c, '\'' | '"' | '`')
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// TypeScript without its comments, which may hold anything, a brace or a `name:` among it.
fn uncommented(ts: &str) -> String {
    let mut out = String::with_capacity(ts.len());
    let mut chars = ts.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), _) => {
                out.push(c);
                if c == '\\' {
                    out.extend(chars.next());
                } else if c == q {
                    quote = None;
                }
            }
            (None, c) if is_quote(c) => {
                quote = Some(c);
                out.push(c);
            }
            (None, '/') if chars.peek() == Some(&'/') => {
                while chars.peek().is_some_and(|&n| n != '\n') {
                    chars.next();
                }
            }
            (None, '/') if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
            }
            (None, _) => out.push(c),
        }
    }
    out
}

/// An interface as types.ts declares it.
#[derive(Debug, Default, PartialEq)]
struct Interface {
    /// What it extends, by name.
    parents: Vec<String>,
    /// The names of its own members.
    own: BTreeSet<String>,
}

/// TypeScript as characters, without its comments. What reads it steps over each string whole,
/// so a brace or a `name:` inside one is never taken for code.
struct Ts(Vec<char>);

impl Ts {
    fn new(ts: &str) -> Ts {
        Ts(uncommented(ts).chars().collect())
    }

    fn at(&self, i: usize) -> Option<char> {
        self.0.get(i).copied()
    }

    /// Past the string whose opening quote is at `i`.
    fn past_string(&self, i: usize) -> usize {
        let quote = self.0[i];
        let mut j = i + 1;
        while j < self.0.len() {
            match self.0[j] {
                '\\' => j += 2,
                c if c == quote => return j + 1,
                _ => j += 1,
            }
        }
        self.0.len()
    }

    /// Where the bracket that opens at `i` closes; none when it never does. An arrow's `>`
    /// closes nothing.
    fn close_of(&self, i: usize) -> Option<usize> {
        let (open, close) = match self.at(i)? {
            '{' => ('{', '}'),
            '<' => ('<', '>'),
            '(' => ('(', ')'),
            '[' => ('[', ']'),
            _ => return None,
        };
        let (mut depth, mut j) = (0, i);
        while j < self.0.len() {
            let c = self.0[j];
            if is_quote(c) {
                j = self.past_string(j);
                continue;
            }
            if c == open {
                depth += 1;
            } else if c == close && !(close == '>' && j > 0 && self.0[j - 1] == '=') {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            j += 1;
        }
        None
    }

    fn past_space(&self, mut i: usize) -> usize {
        while self.at(i).is_some_and(char::is_whitespace) {
            i += 1;
        }
        i
    }

    /// The name that starts at `i` (dotted, as a parent's may be), and where it ends.
    fn word(&self, i: usize) -> (String, usize) {
        let end = (i..self.0.len()).find(|&j| !is_word(self.0[j]) && self.0[j] != '.').unwrap_or(self.0.len());
        (self.0[i..end].iter().collect(), end)
    }

    /// The interface declared from `i`, just past the word `interface`: its name, the
    /// interface, and where its body ends. None when what follows isn't a declaration.
    fn interface(&self, i: usize) -> Option<(String, Interface, usize)> {
        let (name, after) = self.word(self.past_space(i));
        if name.is_empty() {
            return None;
        }
        // Its type parameters, whose bounds may hold braces of their own.
        let past_parameters = |i: usize| -> Option<usize> {
            let i = self.past_space(i);
            Some(if self.at(i) == Some('<') { self.past_space(self.close_of(i)? + 1) } else { i })
        };
        let mut i = past_parameters(after)?;
        let mut parents = Vec::new();
        let (word, after) = self.word(i);
        if word == "extends" {
            i = after;
            loop {
                let (parent, after) = self.word(self.past_space(i));
                if parent.is_empty() {
                    return None;
                }
                parents.push(parent);
                i = past_parameters(after)?;
                if self.at(i) != Some(',') {
                    break;
                }
                i += 1;
            }
        }
        if self.at(i) != Some('{') {
            return None;
        }
        let close = self.close_of(i)?;
        Some((name, Interface { parents, own: self.members(i + 1, close) }, close + 1))
    }

    /// The names of the members between `from` and `to`, an interface's body: each name, quoted
    /// or not, that starts a member at the body's own level and is followed by `:` or `?:`.
    /// A member starts the body, a line, or what follows a `;` or a `,`; whatever is nested in
    /// brackets is stepped over whole.
    fn members(&self, from: usize, to: usize) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let (mut i, mut starts) = (from, true);
        while i < to {
            let c = self.0[i];
            if c.is_whitespace() {
                starts |= c == '\n';
                i += 1;
                continue;
            }
            if std::mem::take(&mut starts) && (is_quote(c) || is_word(c)) {
                let (name, after) = if is_quote(c) {
                    let end = self.past_string(i);
                    (self.0[i + 1..end.saturating_sub(1).max(i + 1)].iter().collect(), end)
                } else {
                    // `readonly name:` names `name`; `readonly:` is a member called that.
                    let (word, after) = self.word(i);
                    let (next, past_next) = self.word(self.past_space(after));
                    if word == "readonly" && !next.is_empty() { (next, past_next) } else { (word, after) }
                };
                let mut j = self.past_space(after);
                if self.at(j) == Some('?') {
                    j += 1;
                }
                if self.at(j) == Some(':') {
                    out.insert(name);
                }
                i = after;
                continue;
            }
            if is_quote(c) {
                i = self.past_string(i);
                continue;
            }
            if matches!(c, '{' | '(' | '[') {
                i = self.close_of(i).map_or(to, |close| close + 1);
                continue;
            }
            starts = matches!(c, ';' | ',');
            i += 1;
        }
        out
    }
}

/// Each interface TypeScript `ts` declares, exported or not.
fn interfaces(ts: &str) -> BTreeMap<String, Interface> {
    let ts = Ts::new(ts);
    let mut out = BTreeMap::new();
    let mut i = 0;
    while i < ts.0.len() {
        let c = ts.0[i];
        if is_quote(c) {
            i = ts.past_string(i);
        } else if is_word(c) {
            let (word, after) = ts.word(i);
            i = after;
            if word == "interface" {
                if let Some((name, interface, end)) = ts.interface(after) {
                    out.insert(name, interface);
                    i = end;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Every member `name` has: its own and those of what it extends. An error for one that isn't
/// an interface, or that extends something that isn't: its members can't be read, and taking
/// it to have none would let a field it does have go unseen.
fn members_of(name: &str, all: &BTreeMap<String, Interface>) -> Result<BTreeSet<String>, String> {
    let found = all.get(name).ok_or_else(|| format!("`{name}` isn't an interface (one written as a `type` isn't read)"))?;
    let mut out = found.own.clone();
    for parent in &found.parents {
        out.extend(members_of(parent, all).map_err(|why| format!("`{name}` extends `{parent}`, and {why}"))?);
    }
    Ok(out)
}

/// Every `pub struct` a Rust file declares.
fn structs(rust: &str) -> Vec<&str> {
    rust.lines().filter_map(|l| l.strip_prefix("pub struct ")).map(|l| l.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or_default()).collect()
}

#[test]
fn the_windows_types_name_what_the_engine_sends() {
    let ts = interfaces(TYPES);
    let mut wrong = Vec::new();
    for p in pairs() {
        let theirs = match members_of(p.ts, &ts) {
            Ok(theirs) => theirs,
            Err(why) => {
                wrong.push(format!("{}: in types.ts, {why}", p.rust));
                continue;
            }
        };
        let theirs: BTreeSet<&str> = theirs.iter().map(String::as_str).collect();
        let only_engine: Vec<&str> = p.names.difference(&theirs).copied().collect();
        let only_windows: Vec<&str> = theirs.difference(&p.names).copied().collect();
        let (said_engine, said_windows) = APART.iter().find(|a| a.0 == p.rust).map_or((&[][..], &[][..]), |a| (a.1, a.2));
        if only_engine != said_engine || only_windows != said_windows {
            wrong.push(format!(
                "{} and types.ts `{}`: only the engine names {only_engine:?} (APART says {said_engine:?}); only the windows name {only_windows:?} (APART says {said_windows:?})",
                p.rust, p.ts
            ));
        }
    }
    for a in APART {
        assert!(pairs().iter().any(|p| p.rust == a.0), "APART lists {}, which isn't paired", a.0);
        assert!(!a.3.is_empty(), "APART says why {} differs", a.0);
    }
    assert!(wrong.is_empty(), "types.ts and the engine's model differ. Name the field on the side that lacks it, or say why they're apart in wiretypes.rs `APART`:\n{}", wrong.join("\n"));
}

#[test]
fn every_struct_of_the_model_is_paired_or_said_to_have_no_interface() {
    let paired: BTreeSet<&str> = pairs().iter().map(|p| p.rust).collect();
    let declared: Vec<&str> = FILES.iter().flat_map(|(_, text)| structs(text)).collect();
    for (file, text) in FILES {
        for name in structs(text) {
            let unmirrored = UNMIRRORED.iter().any(|u| u.0 == name);
            assert!(paired.contains(name) != unmirrored, "{file} `{name}`: pair it with its interface in wiretypes.rs `pairs`, or list it in `UNMIRRORED` with why (and not both)");
        }
    }
    for (name, why) in UNMIRRORED {
        assert!(declared.contains(&name) && !why.is_empty(), "UNMIRRORED lists {name}, which the model's files don't declare");
    }
}

/// `names` is what the windows are sent only while the model names each field the same both
/// ways.
#[test]
fn the_model_names_no_field_one_way_only() {
    for (file, text) in FILES {
        for (n, line) in text.lines().enumerate() {
            if let Some(why) = misnames(line) {
                panic!("{file}:{}: {why}, and wiretypes.rs reads a struct's names as serde reads them. Have it read what's written there before the model does this.\n{line}", n + 1);
            }
        }
    }
}

#[test]
fn a_line_that_names_a_field_one_way_only_is_told() {
    for fine in [
        "#[serde(skip)]",
        "#[serde(default, skip_serializing_if = \"Option::is_none\")]",
        "#[serde(rename_all = \"camelCase\")]",
        "#[serde(rename = \"type\")]",
        "#[serde(default, skip_serializing_if = \"Option::is_none\", deserialize_with = \"emblem_or_none\")]",
        "/// An alias, with serde(alias) in a comment.",
        "pub alias: String,",
    ] {
        assert_eq!(misnames(fine), None, "{fine}");
    }
    for told in [
        "    #[serde(alias = \"old\")]",
        "#[serde(default, alias = \"old\")]",
        "#[serde(flatten)]",
        "#[serde(skip_serializing)]",
        "#[serde(skip_serializing, default)]",
        "#[serde(skip_serializing_if = \"Option::is_none\", skip_serializing)]",
        "#[serde(skip_deserializing)]",
        "#[serde(rename(serialize = \"sent\"))]",
        "#[serde(rename_all(deserialize = \"camelCase\"))]",
        "#[serde(from = \"Old\")]",
        "#[serde(try_from = \"Old\", into = \"Old\")]",
        "#[cfg_attr(test, serde(flatten))]",
    ] {
        assert!(misnames(told).is_some(), "{told}");
    }
}

#[test]
fn an_interfaces_members_are_read_whatever_its_types_hold() {
    let got = interfaces(
        r#"
        /** Not this one: interface Phantom { a: string } */
        export interface Base {
          id: string;
          // gone: number;
        }
        interface Unexported { hidden: boolean }
        export interface Wide<T extends { bound: string } = { bound: 'x' }> extends Base, Unexported, Map<string, { k: 1 }> {
          /** A doc with a name: in it, and a brace { too. */
          label?: string;
          nested: {
            inner: number;
          };
          kind:
            | 'a'
            | 'b';
          list: Array<{ deep: T }>;
          url: 'https://example.com' | null;
          closes: '}' | ')' | ']'; after: string;
          opens: '{' | '(' | '[';
          last: string;
          run(): void;
          call: (first: string, second: number) => Record<string, number>;
          tuple: [left: string, right: number];
          'quoted-key': string;
          readonly fixed: number;
          readonly: boolean;
          [index: string]: unknown;
        }
        export interface OneLine { a: string; b?: number, c: { d: string } }
        export type Alias = { aliased: string };
        export interface Lost extends Alias { mine: string }
        const text = 'interface InAString { no: string }';
    "#,
    );
    let own = |name: &str| got[name].own.iter().map(String::as_str).collect::<Vec<_>>();
    assert_eq!(got.keys().map(String::as_str).collect::<Vec<_>>(), ["Base", "Lost", "OneLine", "Unexported", "Wide"]);
    assert_eq!(own("Base"), ["id"]);
    assert_eq!(own("Wide"), ["after", "call", "closes", "fixed", "kind", "label", "last", "list", "nested", "opens", "quoted-key", "readonly", "tuple", "url"]);
    assert_eq!(got["Wide"].parents, ["Base", "Unexported", "Map"]);
    assert_eq!(own("OneLine"), ["a", "b", "c"]);

    // What it extends counts, and a parent that can't be read is said rather than taken to have
    // no members: a `type`, or something from elsewhere.
    assert_eq!(members_of("OneLine", &got).unwrap().len(), 3);
    assert_eq!(members_of("Lost", &got).unwrap_err(), "`Lost` extends `Alias`, and `Alias` isn't an interface (one written as a `type` isn't read)");
    assert!(members_of("Wide", &got).unwrap_err().starts_with("`Wide` extends `Map`"));
    assert!(members_of("Alias", &got).is_err());
    let narrow = interfaces("export interface Child extends Base { more: string }\nexport interface Base { id: string }");
    assert_eq!(members_of("Child", &narrow).unwrap().into_iter().collect::<Vec<_>>(), ["id", "more"]);
}

#[test]
fn a_structs_names_are_the_ones_serde_reads() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Sample {
        plain: u8,
        two_words: u8,
        #[serde(rename = "other")]
        renamed: u8,
        #[serde(skip)]
        skipped: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left_out_when_none: Option<u8>,
    }
    assert_eq!(names::<Sample>(), Some(BTreeSet::from(["plain", "twoWords", "other", "leftOutWhenNone"])));
    assert_eq!(names::<String>(), None);
    assert_eq!(structs("pub struct One {\n    pub a: u8,\n}\npub struct Two;\nstruct Private;\n"), ["One", "Two"]);
}
