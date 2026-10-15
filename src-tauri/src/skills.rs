//! The skills a session can be asked to run by name, for the quick panel's and the reply box's
//! `/` lists. A skill is a folder with a `SKILL.md` (the Agent Skills layout) or, for Claude
//! Code, a command file; each harness reads its own folders of a worktree and of an account
//! (`Adapter::worktree_skills`, `account_skills`) and has its own way to name one in a prompt
//! (`Adapter::skill_mention`: `/name` for Claude Code, `$name` for Codex). A harness can also
//! hold skills and commands in its program, which no folder shows (`Adapter::builtin_skills`:
//! Claude Code's `/design`, `/loop`, `/compact`); an account's list has those too. What's listed
//! here is each skill once by name, with how each harness that reads it is told to run it: a
//! harness that doesn't read a skill's folder, or can't be told by name, isn't in its
//! `written`, and the windows offer the skill only on the others' accounts.
//!
//! Nothing is kept between asks but reads macOS is holding (held.rs) and what a harness's
//! program lists, which is slow to ask: the files are small, and a skill added a moment ago
//! should be there.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use serde::Serialize;

use crate::config::{AccountCfg, Config};
use crate::harness;
use crate::held::Held;
use crate::model::Harness;

/// What a folder of skills holds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    /// A folder a skill, each with a `SKILL.md`, at most `depth` folders down.
    Skills { depth: usize },
    /// Claude Code's commands: a Markdown file each, named by its path (`a/b.md` is `a:b`).
    Commands,
}

/// A folder a harness reads skills from.
#[derive(Clone, PartialEq, Debug)]
pub struct Root {
    pub dir: PathBuf,
    pub kind: Kind,
    /// What its skills' names start with: a plugin's name, for `plugin:skill`.
    pub prefix: Option<String>,
    /// Its harness makes a command of each skill in it (`command_skills`); None for one that
    /// takes a skill as its file says.
    pub as_commands: Option<AsCommands>,
}

/// How a harness that makes a command of each skill in a folder reads it (Cursor's command
/// line): by the folder each is in, whatever its file's `name` says, and only the ones meant to
/// be typed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AsCommands {
    /// The harness's program, which a skill's `metadata.surfaces` can leave out (Cursor's say
    /// `cli`, `ide` or `cloud`): one that lists only others isn't read.
    pub surface: &'static str,
    /// Only skills really in the folder: one a link takes out of it isn't read.
    pub contained: bool,
}

impl Root {
    pub fn skills(dir: PathBuf, depth: usize) -> Root {
        Root { dir, kind: Kind::Skills { depth }, prefix: None, as_commands: None }
    }
    pub fn commands(dir: PathBuf) -> Root {
        Root { dir, kind: Kind::Commands, prefix: None, as_commands: None }
    }
    pub fn of_plugin(mut self, plugin: &str) -> Root {
        self.prefix = Some(plugin.to_string());
        self
    }
    pub fn as_commands(mut self, how: AsCommands) -> Root {
        self.as_commands = Some(how);
        self
    }
}

/// A skill, as the windows list it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    /// As it's typed after the slash.
    pub name: String,
    pub description: String,
    /// What may follow its name (`argument-hint`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Meant to be run by name; false for one its file keeps off the menu (`user-invocable`).
    pub typed: bool,
    /// The agent may load it by itself; false for one only you run (`disable-model-invocation`).
    pub by_model: bool,
    /// What goes in a prompt in place of `/name`, for each harness that reads it and can be
    /// told to run it by name, by the harness's name in config.toml.
    pub written: BTreeMap<String, String>,
    /// How often it's been run, and when last, where a harness keeps count.
    pub uses: u32,
    pub last_used: i64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct FolderSkills {
    pub folder: String,
    pub skills: Vec<Skill>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct AccountSkills {
    pub account: String,
    pub skills: Vec<Skill>,
}

/// What `list` answers: each folder's skills (its own and those of the folders above it in its
/// worktree), and each account's own, plugins' included.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Listed {
    pub folders: Vec<FolderSkills>,
    pub accounts: Vec<AccountSkills>,
}

/// A skill or command a harness holds in its program, as it lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct Builtin {
    pub name: String,
    pub description: String,
    pub hint: Option<String>,
}

/// A skill's file, as read.
#[derive(Clone, Debug, PartialEq)]
struct File {
    name: String,
    description: String,
    hint: Option<String>,
    typed: bool,
    by_model: bool,
}

/// How long a list waits on a folder macOS is holding behind a privacy prompt.
const HELD_WAIT: Duration = Duration::from_secs(2);
/// How long a list waits for a harness to list what it holds, the first time it's asked: it's
/// asked when Codealong starts (`warm`), so usually it's there already.
const BUILTIN_WAIT: Duration = Duration::from_secs(5);

/// A worktree's skills are read through `Held`: a worktree in Documents waits on a prompt.
static WORKTREES: LazyLock<Held<Vec<(Harness, File)>>> = LazyLock::new(|| Held::new("skills", read_worktree, || {}));

/// The skills of `folders` and `accounts` (by name, as config.toml has them).
pub fn list(folders: &[String], accounts: &[String], cfg: &Config) -> Listed {
    let uses = uses(cfg);
    let paths: Vec<PathBuf> = folders.iter().map(|f| crate::util::expand(f)).collect();
    let mut read = WORKTREES.read(paths.iter().cloned(), HELD_WAIT);
    let folders = folders.iter().zip(&paths).map(|(folder, path)| FolderSkills { folder: folder.clone(), skills: merged(read.remove(path).unwrap_or_default(), &uses) }).collect();
    let accounts: Vec<&AccountCfg> = accounts.iter().filter_map(|name| cfg.accounts.iter().find(|a| &a.name == name)).collect();
    // Every account's harness asked at once, so one waited for doesn't hold up another's ask.
    warm(accounts.iter().copied());
    let accounts = accounts.into_iter().map(|a| AccountSkills { account: a.name.clone(), skills: merged(read_account(a), &uses) }).collect();
    Listed { folders, accounts }
}

/// How often each skill was run, by name, over every account that keeps count.
fn uses(cfg: &Config) -> HashMap<String, (u32, i64)> {
    let mut out: HashMap<String, (u32, i64)> = HashMap::new();
    for a in &cfg.accounts {
        let Some(adapter) = harness::of(a) else { continue };
        for (name, (count, last)) in adapter.skill_uses(a) {
            let e = out.entry(name).or_default();
            *e = (e.0 + count, e.1.max(last));
        }
    }
    out
}

/// The folders from `folder` up to its worktree's root (the one with a `.git`), nearest first;
/// `folder` alone where there's none.
fn up_to_root(folder: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in folder.ancestors() {
        out.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            return out;
        }
    }
    vec![folder.to_path_buf()]
}

/// Every harness's skills in a folder of a worktree, the nearest folder's first.
fn read_worktree(folder: &Path) -> Vec<(Harness, File)> {
    let dirs = up_to_root(folder);
    harness::all().flat_map(|a| a.worktree_skills(&dirs).iter().flat_map(read_root).map(|f| (a.harness(), f)).collect::<Vec<_>>()).collect()
}

/// An account's skills: its folders' first, which describe a name both have, then what its
/// harness holds.
fn read_account(account: &AccountCfg) -> Vec<(Harness, File)> {
    let Some(adapter) = harness::of(account) else { return Vec::new() };
    let files = adapter.account_skills(account).iter().flat_map(read_root).collect::<Vec<_>>();
    let held = adapter.builtin_skills(account, BUILTIN_WAIT).into_iter().filter_map(builtin_file);
    files.into_iter().chain(held).map(|f| (adapter.harness(), f)).collect()
}

/// Has each account's harness list what it holds, in the background where that's due: as
/// Codealong starts, so the first `/` finds it.
pub fn warm<'a>(accounts: impl IntoIterator<Item = &'a AccountCfg>) {
    for a in accounts {
        if let Some(adapter) = harness::of(a) {
            adapter.builtin_skills(a, Duration::ZERO);
        }
    }
}

/// What a harness holds, as a skill: one you run by name. Whether the agent may run it itself
/// isn't said. None for a name nobody could type.
fn builtin_file(b: Builtin) -> Option<File> {
    typeable(&b.name).then(|| File { name: b.name, description: one_line(&b.description), hint: b.hint.filter(|h| !h.is_empty()), typed: true, by_model: false })
}

/// Each skill once by name, with what each harness that read it writes for it. The first file
/// of a name describes it. Ones meant to be typed go first, then the most run, then by name.
fn merged(files: Vec<(Harness, File)>, uses: &HashMap<String, (u32, i64)>) -> Vec<Skill> {
    let mut out: Vec<Skill> = Vec::new();
    for (h, f) in files {
        let Some(mention) = harness::adapter(h).skill_mention(&f.name) else { continue };
        let at = match out.iter().position(|s| s.name == f.name) {
            Some(i) => i,
            None => {
                let (uses, last_used) = uses.get(&f.name).copied().unwrap_or_default();
                out.push(Skill { name: f.name.clone(), description: f.description, hint: f.hint, typed: f.typed, by_model: f.by_model, written: BTreeMap::new(), uses, last_used });
                out.len() - 1
            }
        };
        out[at].written.entry(h.config().to_string()).or_insert(mention);
    }
    out.sort_by(|a, b| b.typed.cmp(&a.typed).then(b.uses.cmp(&a.uses)).then(a.name.cmp(&b.name)));
    out
}

/// The most entries of one folder that are looked at: a folder of skills is tens of them.
const MAX_ENTRIES: usize = 2000;

fn read_root(root: &Root) -> Vec<File> {
    let mut out = Vec::new();
    match root.kind {
        Kind::Skills { depth } => {
            let mut found = Vec::new();
            walk_skills(&root.dir, depth, false, &mut HashSet::new(), &mut found);
            out = match root.as_commands {
                Some(how) => command_skills(found, root, how),
                None => found.iter().filter_map(|f| skill_file(&f.text, folder_name(&f.folder), root.prefix.as_deref())).collect(),
            };
        }
        Kind::Commands => walk_commands(&root.dir, &root.dir, 3, root, &mut out),
    }
    out
}

/// A `SKILL.md` found in a folder of skills: the folder it's in, where it really is, what it
/// says, and whether a link between the root and its folder led to it.
struct Found {
    folder: PathBuf,
    real: PathBuf,
    text: String,
    below_link: bool,
}

fn folder_name(folder: &Path) -> &str {
    folder.file_name().and_then(|n| n.to_str()).unwrap_or_default()
}

/// A folder's entries, by name, the hidden ones left out: links are followed, as the harnesses
/// follow them (a repo's `.agents/skills` is often a link to its `.claude/skills`).
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| !n.starts_with('.'))).take(MAX_ENTRIES).collect();
    paths.sort();
    paths
}

fn walk_skills(dir: &Path, depth: usize, below_link: bool, seen: &mut HashSet<PathBuf>, out: &mut Vec<Found>) {
    if depth == 0 {
        return;
    }
    for path in entries(dir) {
        if !path.is_dir() {
            continue;
        }
        let file = path.join("SKILL.md");
        match std::fs::read_to_string(&file) {
            // A skill reached twice (through a link) is one skill.
            Ok(text) => {
                let real = std::fs::canonicalize(&file).unwrap_or(file);
                if seen.insert(real.clone()) {
                    out.push(Found { folder: path, real, text, below_link });
                }
            }
            Err(_) => {
                let link = path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
                walk_skills(&path, depth - 1, below_link || link, seen, out)
            }
        }
    }
}

fn walk_commands(top: &Path, dir: &Path, depth: usize, root: &Root, out: &mut Vec<File>) {
    for path in entries(dir) {
        if path.is_dir() {
            if depth > 1 {
                walk_commands(top, &path, depth - 1, root, out);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Ok(rel) = path.with_extension("").strip_prefix(top).map(Path::to_path_buf) else { continue };
        let name = rel.components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join(":");
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        out.extend(command_file(&text, &name, root.prefix.as_deref()));
    }
}

fn named(name: &str, prefix: Option<&str>) -> String {
    match prefix {
        Some(p) => format!("{p}:{name}"),
        None => name.to_string(),
    }
}

/// A name that can be typed after a slash and found again in what was typed.
fn typeable(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
}

/// A `SKILL.md` as a skill: named by its `name`, else its folder, and described by its
/// `description`, else its first line. None for a name nobody could type.
fn skill_file(text: &str, folder: &str, prefix: Option<&str>) -> Option<File> {
    let (front, body) = front_matter(text);
    let name = named(front.get("name").map(String::as_str).filter(|n| !n.is_empty()).unwrap_or(folder), prefix);
    described(name, &front, body)
}

/// A skill named `name`, as its front matter and body describe it. None for a name nobody could
/// type.
fn described(name: String, front: &HashMap<String, String>, body: &str) -> Option<File> {
    typeable(&name).then(|| File {
        name,
        description: one_line(front.get("description").map(String::as_str).filter(|d| !d.is_empty()).unwrap_or(first_line(body))),
        hint: front.get("argument-hint").filter(|h| !h.is_empty()).cloned(),
        typed: front.get("user-invocable").map(String::as_str) != Some("false"),
        by_model: front.get("disable-model-invocation").map(String::as_str) != Some("true"),
    })
}

/// The skills of a folder a harness makes commands of, named as it names them, in the order of
/// their files' paths: by the folder each is in, its file's `name` being only a title, or, for a
/// folder name two of them share, by its path from the root (`api/review` is `api-review`), with
/// `-2`, `-3`… after a name an earlier one took. Not one kept off the menu, one whose
/// `metadata.surfaces` leaves out the harness's program, one a link takes out of a folder that
/// keeps to itself, or, where links are followed out, one below the folder a link leads to.
fn command_skills(mut found: Vec<Found>, root: &Root, how: AsCommands) -> Vec<File> {
    let real_root = std::fs::canonicalize(&root.dir).unwrap_or_else(|_| root.dir.clone());
    found.retain(|f| if how.contained { f.real.starts_with(&real_root) } else { !f.below_link });
    found.sort_by_key(|f| f.folder.join("SKILL.md").to_string_lossy().into_owned());
    let mut shared: HashMap<String, usize> = HashMap::new();
    for f in &found {
        *shared.entry(folder_name(&f.folder).to_string()).or_default() += 1;
    }
    // Every skill's name is taken before any is left out, as the harness takes them.
    let mut taken = HashSet::new();
    let named_skills: Vec<(String, &Found)> = found
        .iter()
        .filter_map(|f| {
            let folder = folder_name(&f.folder);
            let base = if shared[folder] == 1 {
                folder.to_string()
            } else {
                f.folder.strip_prefix(&root.dir).ok()?.components().map(|c| c.as_os_str().to_str()).collect::<Option<Vec<_>>>()?.join("-")
            };
            let id = std::iter::once(base.clone()).chain((2..).map(|n| format!("{base}-{n}"))).find(|id| !taken.contains(id))?;
            taken.insert(id.clone());
            Some((id, f))
        })
        .collect();
    named_skills
        .into_iter()
        .filter(|(_, f)| surfaces(&f.text).is_none_or(|listed| listed.iter().any(|s| s == how.surface)))
        .filter_map(|(id, f)| {
            let (front, body) = front_matter(&f.text);
            // Kept off the menu however the word is written, as it reads the value.
            if front.get("user-invocable").is_some_and(|v| v.trim().eq_ignore_ascii_case("false")) {
                return None;
            }
            described(named(&id, root.prefix.as_deref()), &front, body)
        })
        .collect()
}

/// The programs a skill's `metadata.surfaces` lists, a list or words split by commas; None when
/// it lists none. `front_matter` keeps `metadata`, a map, as its lines run together
/// (`surfaces: - ide - cloud`), so the list is the words after `surfaces:` up to the next key,
/// each without the quotes around it or around the words it was split from (`"cli, ide"`).
fn surfaces(text: &str) -> Option<Vec<String>> {
    let (front, _) = front_matter(text);
    let metadata = front.get("metadata")?;
    let after = metadata.split_once("surfaces:")?.1;
    let words = after.split(|c: char| c.is_whitespace() || matches!(c, ',' | '[' | ']')).filter(|w| !w.is_empty() && *w != "-");
    let listed: Vec<String> = words.take_while(|w| !w.ends_with(':')).map(|w| w.trim_matches(|c| c == '"' || c == '\'').to_string()).filter(|w| !w.is_empty()).collect();
    (!listed.is_empty()).then_some(listed)
}

/// A file's first words, its heading's marks left off: what describes one that doesn't say.
fn first_line(body: &str) -> &str {
    body.lines().map(|l| l.trim().trim_start_matches('#').trim()).find(|l| !l.is_empty()).unwrap_or_default()
}

/// A command file as a skill: described by its `description`, else its first line.
fn command_file(text: &str, name: &str, prefix: Option<&str>) -> Option<File> {
    let (front, body) = front_matter(text);
    let name = named(name, prefix);
    typeable(&name).then(|| File {
        name,
        description: one_line(front.get("description").map(String::as_str).filter(|d| !d.is_empty()).unwrap_or(first_line(body))),
        hint: front.get("argument-hint").filter(|h| !h.is_empty()).cloned(),
        typed: true,
        by_model: front.get("disable-model-invocation").map(String::as_str) != Some("true"),
    })
}

/// The longest a description is kept: a list shows a line or two of it.
const MAX_DESCRIPTION: usize = 400;

fn one_line(s: &str) -> String {
    let joined = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.char_indices().nth(MAX_DESCRIPTION) {
        Some((at, _)) => format!("{}…", &joined[..at]),
        None => joined,
    }
}

/// A file's front matter (the lines between its opening `---` and the next) as its top-level
/// keys with plain values, and the rest of the file. This reads what skills write there, not
/// all of YAML: `key: value`, a quoted value, a value that goes on over indented lines, and a
/// block (`>` or `|`); lists and maps are passed over.
fn front_matter(text: &str) -> (HashMap<String, String>, &str) {
    let mut out = HashMap::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(after) = text.strip_prefix("---").and_then(|t| t.strip_prefix("\r\n").or_else(|| t.strip_prefix('\n'))) else { return (out, text) };
    let mut body = "";
    let mut key: Option<String> = None;
    let mut at = 0;
    for line in after.split_inclusive('\n') {
        at += line.len();
        let plain = line.trim_end();
        if plain == "---" || plain == "..." {
            body = &after[at..];
            break;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if indented || plain.is_empty() {
            // More of the value above.
            if let (Some(k), false) = (&key, plain.trim().is_empty()) {
                let v: &mut String = out.entry(k.clone()).or_default();
                if !v.is_empty() {
                    v.push(' ');
                }
                v.push_str(plain.trim());
            }
            continue;
        }
        key = None;
        let Some((k, v)) = plain.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() || k.starts_with('#') || k.starts_with('-') {
            continue;
        }
        key = Some(k.to_string());
        let block = matches!(v.trim_end_matches(|c: char| c == '-' || c == '+' || c.is_ascii_digit()), ">" | "|");
        out.insert(k.to_string(), if block { String::new() } else { unquoted(v) });
    }
    (out, body)
}

fn unquoted(v: &str) -> String {
    let quoted = |q: char| v.len() >= 2 && v.starts_with(q) && v.ends_with(q);
    if quoted('"') {
        v[1..v.len() - 1].replace("\\\"", "\"").replace("\\n", " ")
    } else if quoted('\'') {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        // A comment after a plain value isn't part of it.
        v.split(" #").next().unwrap_or(v).trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::scratch;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn front_matter_reads_what_skills_write() {
        let text = "---\nname: ship\ndescription: \"Ship it: push, \\\"then\\\" watch\"\nargument-hint: '[branch]'\nallowed-tools: Bash, Read\nuser-invocable: false # hidden\nmetadata:\n  author: someone\nwhen: >-\n  a long\n  line\nplain: goes on\n  below\n---\n# Ship\nbody\n";
        let (front, body) = front_matter(text);
        assert_eq!(front["name"], "ship");
        assert_eq!(front["description"], "Ship it: push, \"then\" watch");
        assert_eq!(front["argument-hint"], "[branch]");
        assert_eq!(front["user-invocable"], "false");
        assert_eq!(front["when"], "a long line");
        assert_eq!(front["plain"], "goes on below");
        // A map's keys aren't the file's.
        assert_eq!(front["metadata"], "author: someone");
        assert!(!front.contains_key("author"));
        assert_eq!(body, "# Ship\nbody\n");
        // No front matter: the whole file is its body.
        assert_eq!(front_matter("# Just words\n"), (HashMap::new(), "# Just words\n"));
    }

    #[test]
    fn reads_the_programs_a_skill_is_for() {
        let with = |metadata: &str| surfaces(&format!("---\nname: x\nmetadata:\n{metadata}\ndescription: y\n---\n"));
        let list = |words: &[&str]| Some(words.iter().map(|w| w.to_string()).collect::<Vec<_>>());
        // As Cursor's own skills write it, and the other ways its command line takes.
        assert_eq!(with("  surfaces:\n    - ide\n    - cloud"), list(&["ide", "cloud"]));
        assert_eq!(with("  surfaces: cli, ide"), list(&["cli", "ide"]));
        assert_eq!(with("  surfaces: [\"cli\"]"), list(&["cli"]));
        assert_eq!(with("  surfaces: \"cli, ide\""), list(&["cli", "ide"]));
        // Up to the next key.
        assert_eq!(with("  surfaces:\n    - cli\n  author: someone"), list(&["cli"]));
        assert_eq!(with("  author: someone"), None);
        assert_eq!(surfaces("---\nname: x\n---\n"), None);
    }

    #[test]
    fn a_harness_that_makes_commands_of_skills_names_them_by_folder() {
        let dir = scratch("skills-as-commands");
        let elsewhere = scratch("skills-as-commands-elsewhere");
        // A title its file gives, two folders of one name and a folder named as one of them is
        // then, two kept off the menu, one only for another program, one for this one, one a
        // link takes out of the folder, and one below a folder a link leads to.
        write(&dir.join("release/SKILL.md"), "---\nname: Deploy release\ndescription: Releases\n---\n");
        write(&dir.join("api/review/SKILL.md"), "---\nname: review\n---\n");
        write(&dir.join("web/review/SKILL.md"), "---\nname: review\n---\n");
        write(&dir.join("api-review/SKILL.md"), "---\nname: api-review\ndescription: Top\n---\n");
        write(&dir.join("rules/SKILL.md"), "---\nname: rules\nuser-invocable: false\n---\n");
        write(&dir.join("quiet/SKILL.md"), "---\nname: quiet\nuser-invocable: \" False \"\n---\n");
        write(&dir.join("canvas/SKILL.md"), "---\nname: canvas\nmetadata:\n  surfaces:\n    - ide\n---\n");
        write(&dir.join("config/SKILL.md"), "---\nname: config\nmetadata:\n  surfaces: \"cli, ide\"\n---\n");
        write(&elsewhere.join("shared/SKILL.md"), "---\nname: shared\n---\n");
        std::os::unix::fs::symlink(elsewhere.join("shared"), dir.join("shared")).unwrap();
        write(&elsewhere.join("bundle/review/SKILL.md"), "---\nname: review\n---\n");
        std::os::unix::fs::symlink(elsewhere.join("bundle"), dir.join("bundle")).unwrap();

        let read = |contained: bool| -> Vec<(String, String)> {
            let root = Root::skills(dir.clone(), 3).as_commands(AsCommands { surface: "cli", contained });
            let mut files: Vec<(String, String)> = read_root(&root).into_iter().map(|f| (f.name, f.description)).collect();
            files.sort();
            files
        };
        let pair = |n: &str, d: &str| (n.to_string(), d.to_string());
        // `api-review/SKILL.md` comes before `api/review/SKILL.md`, and takes the name first.
        assert_eq!(read(true), [pair("api-review", "Top"), pair("api-review-2", ""), pair("config", ""), pair("release", "Releases"), pair("web-review", "")]);
        // Followed out of the folder where the harness follows links, but only to the folder a
        // link leads to.
        assert_eq!(read(false), [pair("api-review", "Top"), pair("api-review-2", ""), pair("config", ""), pair("release", "Releases"), pair("shared", ""), pair("web-review", "")]);
        // As its file says, for the others.
        let names: Vec<String> = read_root(&Root::skills(dir.clone(), 3)).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["review", "api-review", "review", "canvas", "config", "quiet", "rules", "shared", "review"]);
    }

    #[test]
    fn a_skill_is_named_by_its_file_then_its_folder() {
        let named = skill_file("---\nname: deploy\ndescription: Deploys\ndisable-model-invocation: true\n---\n", "folder", None).unwrap();
        assert_eq!((named.name.as_str(), named.typed, named.by_model), ("deploy", true, false));
        let bare = skill_file("# Folder things\nmore", "folder", Some("plug")).unwrap();
        assert_eq!((bare.name.as_str(), bare.description.as_str()), ("plug:folder", "Folder things"));
        // A name with a space can't be typed after a slash and found again.
        assert_eq!(skill_file("---\nname: two words\n---\n", "folder", None), None);
        let command = command_file("\n# Review the diff\nmore\n", "git:review", None).unwrap();
        assert_eq!((command.name.as_str(), command.description.as_str()), ("git:review", "Review the diff"));
    }

    #[test]
    fn a_worktrees_skills_are_listed_once_with_what_each_harness_writes() {
        let dir = scratch("skills-worktree");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        // Claude Code's own, one both read through a link, one only Codex reads, a command, and
        // one in a folder below the root. Cursor reads the skills folders of the folder asked
        // about alone, names a skill by its folder, and leaves out one only for its app and one
        // kept off the menu.
        write(&dir.join(".claude/skills/ship/SKILL.md"), "---\nname: ship\ndescription: Ships\nargument-hint: \"[branch]\"\n---\n");
        write(&dir.join(".claude/skills/conventions/SKILL.md"), "---\nname: conventions\ndescription: House rules\nuser-invocable: false\n---\n");
        write(&dir.join(".claude/skills/settings/SKILL.md"), "---\nname: settings\ndescription: Sets\nmetadata:\n  surfaces:\n    - ide\n    - cloud\n---\n");
        write(&dir.join(".claude/commands/git/review.md"), "Review the diff\n");
        write(&dir.join(".codex/skills/probe/SKILL.md"), "---\nname: probe\ndescription: Probes\n---\n");
        std::fs::create_dir_all(dir.join(".agents")).unwrap();
        std::os::unix::fs::symlink("../.claude/skills", dir.join(".agents/skills")).unwrap();
        write(&dir.join("app/.claude/skills/local/SKILL.md"), "---\ndescription: Only here\n---\n");
        write(&dir.join(".cursor/skills/tidy-up/SKILL.md"), "---\nname: tidy\ndescription: Tidies\n---\n");

        let cfg = Config::default();
        let app = dir.join("app").to_string_lossy().into_owned();
        let root = dir.to_string_lossy().into_owned();
        let listed = list(&[app.clone(), root], &[], &cfg);
        assert_eq!(listed.folders[0].folder, app);
        let said = |i: usize| -> Vec<(String, Vec<(String, String)>)> { listed.folders[i].skills.iter().map(|s| (s.name.clone(), s.written.clone().into_iter().collect())).collect() };
        let pair = |h: &str, w: &str| (h.to_string(), w.to_string());
        assert_eq!(
            said(0),
            vec![
                ("git:review".to_string(), vec![pair("claude", "/git:review")]),
                ("local".to_string(), vec![pair("claude", "/local"), pair("cursor", "/local")]),
                ("probe".to_string(), vec![pair("codex", "$probe")]),
                ("settings".to_string(), vec![pair("claude", "/settings"), pair("codex", "$settings")]),
                ("ship".to_string(), vec![pair("claude", "/ship"), pair("codex", "$ship")]),
                // Kept off the menu by its file: last.
                ("conventions".to_string(), vec![pair("claude", "/conventions"), pair("codex", "$conventions")]),
            ]
        );
        assert_eq!(
            said(1),
            vec![
                ("git:review".to_string(), vec![pair("claude", "/git:review")]),
                ("probe".to_string(), vec![pair("codex", "$probe"), pair("cursor", "/probe")]),
                ("settings".to_string(), vec![pair("claude", "/settings"), pair("codex", "$settings")]),
                ("ship".to_string(), vec![pair("claude", "/ship"), pair("codex", "$ship"), pair("cursor", "/ship")]),
                ("tidy-up".to_string(), vec![pair("cursor", "/tidy-up")]),
                // Kept off the menu: not a command of Cursor's.
                ("conventions".to_string(), vec![pair("claude", "/conventions"), pair("codex", "$conventions")]),
            ]
        );
        let ship = listed.folders[0].skills.iter().find(|s| s.name == "ship").unwrap();
        assert_eq!((ship.description.as_str(), ship.hint.as_deref()), ("Ships", Some("[branch]")));
        // A folder with none, and one that isn't there, have none.
        let bare = scratch("skills-none");
        assert_eq!(list(&[bare.to_string_lossy().into_owned(), bare.join("gone").to_string_lossy().into_owned()], &[], &cfg).folders.iter().map(|f| f.skills.len()).collect::<Vec<_>>(), [0, 0]);
    }

    #[test]
    fn the_most_run_skills_come_first() {
        let file = |name: &str, typed: bool| (Harness::Claude, File { name: name.into(), description: String::new(), hint: None, typed, by_model: true });
        let uses = HashMap::from([("db".to_string(), (5, 9)), ("rules".to_string(), (50, 9))]);
        let names: Vec<String> = merged(vec![file("alpha", true), file("rules", false), file("db", true)], &uses).into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["db", "alpha", "rules"]);
    }
}
