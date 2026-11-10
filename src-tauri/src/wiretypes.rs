//! Holds `src/types.ts` to the engine's model, by name.
//!
//! The windows' types are written by hand to mirror what serde writes, with the comments the
//! windows need, and nothing else says when a field is added, dropped or renamed on one side
//! only. A field the engine sends that no interface declares is one a window can't read without
//! a cast; a member the engine never sends is always undefined, whatever its type says.
//!
//! Each struct of the model is paired here with the interface that mirrors it (`pairs`), and
//! the names it writes its fields by go into `src/testing/modelFields.ts` (`fixture`), where
//! the TypeScript compiler holds each interface's own names to them: `npm run typecheck` fails
//! on the line of one that differs, saying which names only one side has. A test here keeps
//! that file current, as windowfeed.rs keeps its fixture. What differs on purpose is listed
//! with why (`APART`), and has to stay true. Every struct in the model's files (`FILES`) is
//! paired or said to have no interface (`UNMIRRORED`), so a new one has to say which. What the
//! windows are sent besides the board is paired too, as far as that's been done (`others`):
//! a type added there is held from then on.
//!
//! Names only: what type each field is, and whether it may be missing, is still kept in step by
//! hand. The names are the ones serde writes, read from a value as it's written (`written`),
//! which `sample` makes of any type that reads: so a field read by another name than it's
//! written by, skipped one way, or written with a tag, comes out as the windows get it.

use std::collections::BTreeSet;

use serde::de::value::{Error, StrDeserializer};
use serde::de::{self, DeserializeOwned, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};
use serde::ser::{self, Impossible, SerializeStruct, Serializer};
use serde::Serialize;

use crate::model;

/// The files the board's model is in.
const FILES: [(&str, &str); 2] = [("peer/src/model.rs", include_str!("../peer/src/model.rs")), ("src/model.rs", include_str!("model.rs"))];

/// Where the names go for the TypeScript compiler to hold types.ts to, from this crate's folder.
const FIXTURE: &str = "../src/testing/modelFields.ts";

/// A struct and the interface that mirrors it.
struct Pair {
    rust: &'static str,
    ts: &'static str,
    /// The names it writes its fields by (`written`).
    names: BTreeSet<&'static str>,
}

/// A struct paired with the interface `ts`, its names read from a value made for the purpose.
/// One `sample` can't make (it says why) is paired with `pair_of` and a value made by hand.
fn pair<T: DeserializeOwned + Serialize>(ts: &'static str) -> Pair {
    let rust = std::any::type_name::<T>().rsplit("::").next().unwrap_or_default();
    let value = sample::<T>().unwrap_or_else(|why| panic!("{rust}: no value of it could be made to see what it writes ({why}); pair it with `pair_of` and one made by hand"));
    pair_of(ts, &value)
}

fn pair_of<T: Serialize>(ts: &'static str, value: &T) -> Pair {
    let rust = std::any::type_name::<T>().rsplit("::").next().unwrap_or_default();
    Pair { rust, ts, names: written(value).unwrap_or_else(|why| panic!("{rust}: {why}")) }
}

/// The model's structs, each with the interface of types.ts that mirrors it: most by the same
/// name.
fn pairs() -> Vec<Pair> {
    macro_rules! same {
        ($($t:ident),* $(,)?) => { vec![$(pair::<model::$t>(stringify!($t))),*] };
    }
    let mut pairs = same![
        Session, Emblem, Artifact, Helper, SessionMode, ContextUse, QueuedMessage, SessionLimit, ConnectionLost, Suggestions, SideAnswer, Raised, AgentSeen, RunningIn, SessionOrigin, ChromeTabs,
        SessionInsight, Checks, Label, PrLabel, Pr, StaleApproval, GitInfo, Target, Lane, Cleanup, Preview, PreviewServer, DeviceTag, Meter, Usage, ProjectSummary, AccountInfo, Notice, Board,
        TodoHint, PrItem, LastReview, AutoReview, ReviewPart, PrSession,
    ];
    pairs.extend(others());
    pairs
}

/// What the windows are sent besides the board, where it's made: each with the interface that
/// mirrors it.
fn others() -> Vec<Pair> {
    macro_rules! of {
        ($module:path: $($t:ident),* $(,)?) => {{
            use $module as from;
            vec![$(pair::<from::$t>(stringify!($t))),*]
        }};
    }
    [
        of!(crate::todos: Todo),
        // Its `when` is a tagged enum, which no plain value reads as: its default stands in.
        vec![pair_of("Schedule", &crate::schedules::Schedule::default())],
        of!(crate::ledger: JevReport, JevUsed, SessionSpent, Spent),
        of!(crate::teamfile: TeamCfg, TeamFile),
        of!(crate::listen: Listening),
        of!(crate::mcp: Mcp),
        of!(crate::improve: Improve),
        of!(crate::training: Training),
        of!(crate::peers: DeviceView, ProjectThere),
        of!(
            crate::config: AccountCfg, Config, DevicesCfg, EmblemsCfg, ExperimentalCfg, GroupMemberCfg, JevCfg, OwnReviewsCfg, Profile, ProjectCfg, PullRequestsCfg, RepoCfg, ReviewsCfg, ReviewsPatch,
            RoutingCfg, RoutingGroupCfg, SessionsCfg, SoundsCfg, VoiceCfg
        ),
        of!(
            codealong_peer::api: GroupMember, ModeSwitched, Pending, Question, ResumeIn, Route, RoutingGroup, Sent, Side, SideKept, SideTurn, StartAccount, StartOptions, Started, Unqueued
        ),
        of!(codealong_peer::pr: CommitText, MergeBox, Turn),
        // Not among codealong-peer's: `StartSpec`, which reads a field by two names, so no plain
        // value of it can be made (one made by hand would pair it); and `TerminalRun`, whose
        // namesake in types.ts is a run of a terminal's drawing.
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The model's structs no interface mirrors, each with why.
const UNMIRRORED: [(&str, &str); 2] = [
    ("RepoSummary", "typed in place, as `ProjectSummary.repos`"),
    ("AppNames", "typed in place, as `Board.apps`"),
];

/// What a struct and its interface name differently on purpose.
struct Apart {
    rust: &'static str,
    /// What the engine writes and no window's type names.
    engine: &'static [&'static str],
    /// What the windows' type names and the engine never writes.
    windows: &'static [&'static str],
    why: &'static str,
}

const APART: [Apart; 2] = [
    Apart {
        rust: "Target",
        engine: &[],
        windows: &["image"],
        why: "An editor's own icon, which the windows put on its target from the icons they ask for apart (src/editors.ts).",
    },
    Apart { rust: "StartAccount", engine: &["preferred"], windows: &[], why: "Kept for devices of older builds that read it here; no window does." },
];

const NOT_A_STRUCT: &str = "it isn't written as a struct with named fields (a flattened field writes it as a map)";

/// The names `value` writes its fields by: each it writes, and each it leaves out of this value
/// that another has (`skip_serializing_if`, which serde tells what it writes to of).
fn written<T: Serialize>(value: &T) -> Result<BTreeSet<&'static str>, String> {
    struct Names(BTreeSet<&'static str>);
    impl SerializeStruct for Names {
        type Ok = BTreeSet<&'static str>;
        type Error = Error;
        fn serialize_field<V: ?Sized + Serialize>(&mut self, key: &'static str, _: &V) -> Result<(), Error> {
            self.0.insert(key);
            Ok(())
        }
        fn skip_field(&mut self, key: &'static str) -> Result<(), Error> {
            self.0.insert(key);
            Ok(())
        }
        fn end(self) -> Result<Self::Ok, Error> {
            Ok(self.0)
        }
    }
    /// Takes a struct with named fields and nothing else.
    struct Top;
    macro_rules! only_a_struct {
        ($($method:ident($($arg:ty),*) -> $ret:ty;)*) => {
            $(fn $method(self, $(_: $arg),*) -> Result<$ret, Error> { Err(ser::Error::custom(NOT_A_STRUCT)) })*
        };
    }
    impl Serializer for Top {
        type Ok = BTreeSet<&'static str>;
        type Error = Error;
        type SerializeSeq = Impossible<Self::Ok, Error>;
        type SerializeTuple = Impossible<Self::Ok, Error>;
        type SerializeTupleStruct = Impossible<Self::Ok, Error>;
        type SerializeTupleVariant = Impossible<Self::Ok, Error>;
        type SerializeMap = Impossible<Self::Ok, Error>;
        type SerializeStruct = Names;
        type SerializeStructVariant = Impossible<Self::Ok, Error>;
        fn serialize_struct(self, _: &'static str, _: usize) -> Result<Names, Error> {
            Ok(Names(BTreeSet::new()))
        }
        fn serialize_some<V: ?Sized + Serialize>(self, _: &V) -> Result<Self::Ok, Error> {
            Err(ser::Error::custom(NOT_A_STRUCT))
        }
        fn serialize_newtype_struct<V: ?Sized + Serialize>(self, _: &'static str, _: &V) -> Result<Self::Ok, Error> {
            Err(ser::Error::custom(NOT_A_STRUCT))
        }
        fn serialize_newtype_variant<V: ?Sized + Serialize>(self, _: &'static str, _: u32, _: &'static str, _: &V) -> Result<Self::Ok, Error> {
            Err(ser::Error::custom(NOT_A_STRUCT))
        }
        only_a_struct! {
            serialize_bool(bool) -> Self::Ok; serialize_i8(i8) -> Self::Ok; serialize_i16(i16) -> Self::Ok; serialize_i32(i32) -> Self::Ok; serialize_i64(i64) -> Self::Ok;
            serialize_u8(u8) -> Self::Ok; serialize_u16(u16) -> Self::Ok; serialize_u32(u32) -> Self::Ok; serialize_u64(u64) -> Self::Ok;
            serialize_f32(f32) -> Self::Ok; serialize_f64(f64) -> Self::Ok; serialize_char(char) -> Self::Ok; serialize_str(&str) -> Self::Ok; serialize_bytes(&[u8]) -> Self::Ok;
            serialize_none() -> Self::Ok; serialize_unit() -> Self::Ok; serialize_unit_struct(&'static str) -> Self::Ok; serialize_unit_variant(&'static str, u32, &'static str) -> Self::Ok;
            serialize_seq(Option<usize>) -> Self::SerializeSeq; serialize_tuple(usize) -> Self::SerializeTuple; serialize_tuple_struct(&'static str, usize) -> Self::SerializeTupleStruct;
            serialize_tuple_variant(&'static str, u32, &'static str, usize) -> Self::SerializeTupleVariant; serialize_map(Option<usize>) -> Self::SerializeMap;
            serialize_struct_variant(&'static str, u32, &'static str, usize) -> Self::SerializeStructVariant;
        }
    }
    value.serialize(Top).map_err(|e| e.to_string())
}

/// A value of any type that reads, made of the plainest of everything: every field of a struct
/// given, numbers 0, text and lists empty, what may be missing missing, an enum its first
/// variant. Only for seeing what it writes (`written`). An error is serde's, for a type that
/// can't be read this way: a field with an alias (given by both names, it's given twice), one
/// that must be something particular, one that holds itself as it's made.
fn sample<T: DeserializeOwned>() -> Result<T, String> {
    /// How far in a value is made before it's given up as one that never ends.
    const DEEPEST: usize = 64;
    /// What reads as the plainest of anything, this far in.
    struct Plain(usize);
    impl Plain {
        /// What reads the next level in from `depth`; an error where a type holds itself as
        /// it's made, which would never end.
        fn within(depth: usize) -> Result<Plain, Error> {
            if depth >= DEEPEST {
                return Err(de::Error::custom("it holds itself without end as it's made: an enum whose first variant holds the enum, most likely"));
            }
            Ok(Plain(depth + 1))
        }
    }
    macro_rules! plain {
        ($($method:ident => $visit:ident($($value:expr),*);)*) => {
            $(fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> { visitor.$visit($($value),*) })*
        };
    }
    impl<'de> Deserializer<'de> for Plain {
        type Error = Error;
        plain! {
            deserialize_any => visit_unit(); deserialize_ignored_any => visit_unit(); deserialize_unit => visit_unit(); deserialize_option => visit_none();
            deserialize_bool => visit_bool(false); deserialize_char => visit_char('a');
            deserialize_i8 => visit_i64(0); deserialize_i16 => visit_i64(0); deserialize_i32 => visit_i64(0); deserialize_i64 => visit_i64(0); deserialize_i128 => visit_i64(0);
            deserialize_u8 => visit_u64(0); deserialize_u16 => visit_u64(0); deserialize_u32 => visit_u64(0); deserialize_u64 => visit_u64(0); deserialize_u128 => visit_u64(0);
            deserialize_f32 => visit_f64(0.0); deserialize_f64 => visit_f64(0.0);
            deserialize_str => visit_str(""); deserialize_string => visit_str(""); deserialize_identifier => visit_str("");
            deserialize_bytes => visit_bytes(&[]); deserialize_byte_buf => visit_bytes(&[]);
            deserialize_seq => visit_seq(Items { left: 0, depth: 0 }); deserialize_map => visit_map(Fields { names: [].iter(), depth: 0 });
        }
        fn deserialize_unit_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_unit()
        }
        fn deserialize_newtype_struct<V: Visitor<'de>>(self, _: &'static str, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_newtype_struct(Plain::within(self.0)?)
        }
        fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_seq(Items { left: len, depth: self.0 })
        }
        fn deserialize_tuple_struct<V: Visitor<'de>>(self, _: &'static str, len: usize, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_seq(Items { left: len, depth: self.0 })
        }
        fn deserialize_struct<V: Visitor<'de>>(self, _: &'static str, fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
            visitor.visit_map(Fields { names: fields.iter(), depth: self.0 })
        }
        fn deserialize_enum<V: Visitor<'de>>(self, name: &'static str, variants: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
            let variant = variants.first().copied().ok_or_else(|| <Error as de::Error>::custom(format!("{name} has no variant")))?;
            visitor.visit_enum(First { variant, depth: self.0 })
        }
    }
    /// As many plain items as a tuple wants; none for a list.
    struct Items {
        left: usize,
        depth: usize,
    }
    impl<'de> SeqAccess<'de> for Items {
        type Error = Error;
        fn next_element_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<Option<S::Value>, Error> {
            if self.left == 0 {
                return Ok(None);
            }
            self.left -= 1;
            seed.deserialize(Plain::within(self.depth)?).map(Some)
        }
    }
    /// Each of a struct's fields, by the names it says it reads, with a plain value.
    struct Fields {
        names: std::slice::Iter<'static, &'static str>,
        depth: usize,
    }
    impl<'de> MapAccess<'de> for Fields {
        type Error = Error;
        fn next_key_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<Option<S::Value>, Error> {
            self.names.next().map(|name| seed.deserialize(StrDeserializer::new(name))).transpose()
        }
        fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, Error> {
            seed.deserialize(Plain::within(self.depth)?)
        }
    }
    /// An enum's first variant, with whatever it holds made plain.
    struct First {
        variant: &'static str,
        depth: usize,
    }
    impl<'de> EnumAccess<'de> for First {
        type Error = Error;
        type Variant = Plain;
        fn variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<(S::Value, Plain), Error> {
            Ok((seed.deserialize(StrDeserializer::new(self.variant))?, Plain(self.depth)))
        }
    }
    impl<'de> VariantAccess<'de> for Plain {
        type Error = Error;
        fn unit_variant(self) -> Result<(), Error> {
            Ok(())
        }
        fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, Error> {
            seed.deserialize(Plain::within(self.0)?)
        }
        fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
            visitor.visit_seq(Items { left: len, depth: self.0 })
        }
        fn struct_variant<V: Visitor<'de>>(self, fields: &'static [&'static str], visitor: V) -> Result<V::Value, Error> {
            visitor.visit_map(Fields { names: fields.iter(), depth: self.0 })
        }
    }
    T::deserialize(Plain(0)).map_err(|e| e.to_string())
}

/// A name as TypeScript writes a string: JSON's way, which is one of its own, whatever the name
/// holds.
fn literal(name: &str) -> String {
    serde_json::to_string(name).unwrap_or_default()
}

/// `src/testing/modelFields.ts`: for each pair, a type that holds only while the interface's
/// own names are the ones the struct writes, less what `APART` lists, which has lines of its
/// own to hold it true.
fn fixture() -> String {
    let mut out = String::from(
        "// Written by src-tauri/src/wiretypes.rs, whose test keeps it current\n\
         // (`CODEALONG_WRITE_FIXTURES=1 cargo test wiretypes` writes it again): the names the engine\n\
         // writes each struct of its model by, held against the interface of types.ts that mirrors it.\n\
         // `npm run typecheck` fails on the line of one that names other fields than the engine sends,\n\
         // saying which only one side has: name the field on the side that lacks it, or say why the two\n\
         // are apart in wiretypes.rs `APART`.\n\
         import type * as Types from '../types';\n\
         \n\
         type Same<Engine, Windows> = [Exclude<Engine, Windows>, Exclude<Windows, Engine>] extends [never, never]\n\
         \x20 ? true\n\
         \x20 : { onlyTheEngineNames: Exclude<Engine, Windows>; onlyTheWindowsName: Exclude<Windows, Engine> };\n\
         type Holds<T extends true> = T;\n\
         /** A type's names as JSON has them: a number's as text, and those of every member of a union. */\n\
         type Names<T> = T extends unknown ? Text<keyof T> : never;\n\
         type Text<Key> = Key extends string | number ? `${Key}` : Key;\n\
         \n\
         // The types above, held to what they're for: a union names what any member of it does, a\n\
         // number is its text, and a name only one side has doesn't hold.\n\
         export type Helpers = [\n\
         \x20 Holds<Same<\"id\" | \"more\", Names<{ id: string } | { id: string; more: string }>>>,\n\
         \x20 Holds<Same<\"0\", Names<{ 0: string }>>>,\n\
         \x20 // @ts-expect-error: `more` is only the windows'.\n\
         \x20 Holds<Same<\"id\", Names<{ id: string; more?: string }>>>,\n\
         ];\n",
    );
    // What holds `APART` true, where it has something to hold: an unused type fails the typecheck.
    if APART.iter().any(|a| !a.windows.is_empty()) {
        out.push_str("type Has<Theirs, Name> = Name extends Theirs ? true : { theWindowsNoLongerName: Name };\n");
    }
    if APART.iter().any(|a| !a.engine.is_empty()) {
        out.push_str("type Lacks<Theirs, Name> = Name extends Theirs ? { theWindowsNowName: Name } : true;\n");
    }
    for p in pairs() {
        let apart = APART.iter().find(|a| a.rust == p.rust);
        let names: Vec<String> = p.names.iter().filter(|n| !apart.is_some_and(|a| a.engine.contains(n))).map(|n| format!("  | {}", literal(n))).collect();
        let engine = if names.is_empty() { "  never".to_string() } else { names.join("\n") };
        let theirs = format!("Names<Types.{}>", p.ts);
        let windows = match apart.map(|a| a.windows).filter(|w| !w.is_empty()) {
            Some(only) => format!("Exclude<{theirs}, {}>", only.iter().map(|n| literal(n)).collect::<Vec<_>>().join(" | ")),
            None => theirs.clone(),
        };
        out.push_str(&format!("\nexport type {} = Holds<Same<\n{engine},\n  {windows}\n>>;\n", p.rust));
        if let Some(a) = apart {
            let held = a.windows.iter().map(|n| format!("Holds<Has<{theirs}, {}>>", literal(n))).chain(a.engine.iter().map(|n| format!("Holds<Lacks<{theirs}, {}>>", literal(n))));
            out.push_str(&format!("/** {} */\nexport type {}Apart = [{}];\n", a.why, p.rust, held.collect::<Vec<_>>().join(", ")));
        }
    }
    out
}

/// Every `pub struct` a Rust file declares.
fn structs(rust: &str) -> Vec<&str> {
    rust.lines().filter_map(|l| l.strip_prefix("pub struct ")).map(|l| l.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or_default()).collect()
}

/// What the TypeScript compiler holds types.ts to is what the model writes now.
#[test]
fn the_names_the_typecheck_holds_types_ts_to_are_current() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    let made = fixture();
    if std::env::var_os("CODEALONG_WRITE_FIXTURES").is_some() {
        std::fs::write(&path, &made).unwrap();
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        kept == made,
        "src/testing/modelFields.ts is stale: the model's fields changed. Write it again with `CODEALONG_WRITE_FIXTURES=1 cargo test wiretypes`, then name the same fields in src/types.ts, which `npm run typecheck` holds to it."
    );
}

#[test]
fn every_struct_of_the_model_is_paired_or_said_to_have_no_interface() {
    let pairs = pairs();
    let paired: BTreeSet<&str> = pairs.iter().map(|p| p.rust).collect();
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
    // What's apart on purpose is true of the engine's side; the typecheck holds the windows'.
    for a in &APART {
        let p = pairs.iter().find(|p| p.rust == a.rust).unwrap_or_else(|| panic!("APART lists {}, which isn't paired", a.rust));
        assert!(!a.why.is_empty() && !(a.engine.is_empty() && a.windows.is_empty()), "APART says what of {} is apart, and why", a.rust);
        assert!(a.engine.iter().all(|n| p.names.contains(n)), "APART says the engine writes {:?} of {}, and it doesn't", a.engine, a.rust);
        assert!(!a.windows.iter().any(|n| p.names.contains(n)), "APART says only the windows name {:?} of {}, and the engine writes it", a.windows, a.rust);
    }
}

/// The names are the ones written, however a field is read.
#[test]
fn a_structs_names_are_the_ones_it_writes() {
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct Inner {
        deep_field: u8,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Kind {
        Holding(Inner),
        Plain,
    }
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)]
    struct Sample {
        plain: u8,
        two_words: String,
        #[serde(rename = "other")]
        renamed: (u8, bool),
        #[serde(skip)]
        skipped: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left_out_when_none: Option<Inner>,
        #[serde(skip_serializing)]
        only_read: u8,
        #[serde(skip_deserializing)]
        only_written: u8,
        #[serde(rename(serialize = "sent", deserialize = "read"))]
        named_each_way: Vec<Inner>,
        kind: Kind,
        nested: Inner,
        anything: serde_json::Value,
    }
    let names = |names: &[&'static str]| Ok(names.iter().copied().collect::<BTreeSet<_>>());
    let value = sample::<Sample>().unwrap();
    assert_eq!(written(&value), names(&["plain", "twoWords", "other", "leftOutWhenNone", "onlyWritten", "sent", "kind", "nested", "anything"]));

    // A tag is a field of what's written.
    #[derive(Serialize, Deserialize)]
    #[serde(tag = "kind")]
    struct Tagged {
        id: u8,
    }
    assert_eq!(written(&Tagged { id: 0 }), names(&["kind", "id"]));

    // What can't be read this way says so: an alias, given by both names, is given twice.
    #[derive(Serialize, Deserialize)]
    struct Aliased {
        #[serde(alias = "old")]
        current: u8,
    }
    assert!(sample::<Aliased>().is_err_and(|why| why.contains("duplicate field")));
    assert_eq!(written(&Aliased { current: 0 }), names(&["current"]));

    // And what isn't written as a struct with named fields: a flattened field makes it a map.
    #[derive(Serialize, Deserialize, Default)]
    struct Flattened {
        id: u8,
        #[serde(flatten)]
        rest: Inner,
    }
    assert_eq!(written(&Flattened::default()), Err(NOT_A_STRUCT.to_string()));
    assert_eq!(written(&"text"), Err(NOT_A_STRUCT.to_string()));

    // A type that holds itself as it's made is given up, where making it would never end.
    #[derive(Serialize, Deserialize)]
    #[allow(dead_code)]
    enum Tree {
        Branch(Box<Tree>),
        Leaf,
    }
    assert!(sample::<Tree>().is_err_and(|why| why.contains("without end")));

    // A name goes into the fixture as a string of TypeScript's, whatever it holds.
    assert_eq!(literal("plain"), "\"plain\"");
    assert_eq!(literal("it's a \\n \"name\""), r#""it's a \\n \"name\"""#);

    assert_eq!(structs("pub struct One {\n    pub a: u8,\n}\npub struct Two;\nstruct Private;\n"), ["One", "Two"]);
}
