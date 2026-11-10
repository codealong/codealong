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
/// a plain struct (one with a flattened field reads as a map, which says no names).
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
            (None, '\'' | '"' | '`') => {
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

/// The names of an interface's own members: what starts a line at the top level of its body
/// and is followed by `:` or `?:`.
fn members(body: &str) -> BTreeSet<String> {
    let mut depth = 0;
    let mut out = BTreeSet::new();
    for line in body.lines() {
        let t = line.trim();
        if depth == 0 {
            let t = t.strip_prefix("readonly ").unwrap_or(t);
            let name: String = t.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '$')).collect();
            let after = t[name.len()..].trim_start();
            if !name.is_empty() && (after.starts_with(':') || after.starts_with("?:")) {
                out.insert(name);
            }
        }
        for c in t.chars() {
            match c {
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                _ => {}
            }
        }
    }
    out
}

/// Each `export interface` of TypeScript `ts` with the names of its members: its own and those
/// of the interfaces it extends.
fn interfaces(ts: &str) -> BTreeMap<String, BTreeSet<String>> {
    let text = uncommented(ts);
    let ident = |s: &str| s.trim().chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '$')).collect::<String>();
    let mut own: BTreeMap<String, (Vec<String>, BTreeSet<String>)> = BTreeMap::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("export interface ") {
        rest = &rest[at + "export interface ".len()..];
        let Some(open) = rest.find('{') else { break };
        let head = &rest[..open];
        let parents = head.split_once(" extends ").map(|(_, p)| p.split(',').map(ident).filter(|p| !p.is_empty()).collect()).unwrap_or_default();
        let mut depth = 0;
        let close = rest[open..].char_indices().find_map(|(i, c)| {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            (depth == 0).then_some(open + i)
        });
        let Some(close) = close else { break };
        own.insert(ident(head), (parents, members(&rest[open + 1..close])));
        rest = &rest[close..];
    }
    fn all(name: &str, own: &BTreeMap<String, (Vec<String>, BTreeSet<String>)>) -> BTreeSet<String> {
        let Some((parents, mine)) = own.get(name) else { return BTreeSet::new() };
        parents.iter().flat_map(|p| all(p, own)).chain(mine.iter().cloned()).collect()
    }
    own.keys().map(|name| (name.clone(), all(name, &own))).collect()
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
        let Some(theirs) = ts.get(p.ts) else {
            wrong.push(format!("{}: types.ts has no `export interface {}`", p.rust, p.ts));
            continue;
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

#[test]
fn an_interfaces_members_are_read_past_comments_nested_types_and_what_it_extends() {
    let ts = r#"
        /** Not this one: { a: string } */
        export interface Base {
          id: string;
          // gone: number;
        }
        export interface Wide<T> extends Base, Other {
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
          run(): void;
        }
    "#;
    let got = interfaces(ts);
    assert_eq!(got["Base"], BTreeSet::from(["id".to_string()]));
    assert_eq!(got["Wide"].iter().map(String::as_str).collect::<Vec<_>>(), ["id", "kind", "label", "list", "nested", "url"]);
}

#[test]
fn a_structs_names_are_the_ones_serde_writes() {
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
    }
    assert_eq!(names::<Sample>(), Some(BTreeSet::from(["plain", "twoWords", "other"])));
    assert_eq!(names::<String>(), None);
    assert_eq!(structs("pub struct One {\n    pub a: u8,\n}\npub struct Two;\nstruct Private;\n"), ["One", "Two"]);
}
