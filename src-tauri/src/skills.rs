//! The skills a session can be asked to run by name, for the quick panel's and the reply box's
//! `/` lists. A skill is a folder with a `SKILL.md` (the Agent Skills layout) or, for Claude
//! Code, a command file; each harness reads its own folders of a worktree and of an account
//! (`Adapter::worktree_skills`, `account_skills`) and has its own way to name one in a prompt
//! (`Adapter::skill_mention`: `/name` for Claude Code, `$name` for Codex). What's listed here is
//! each skill once by name, with how each harness that reads it is told to run it: a harness
//! that doesn't read a skill's folder, or can't be told by name, isn't in its `written`, and
//! the windows offer the skill only on the others' accounts.
//!
//! Nothing is kept between asks but reads macOS is holding (held.rs): the files are small, and a
//! skill added a moment ago should be there.

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
}

impl Root {
    pub fn skills(dir: PathBuf, depth: usize) -> Root {
        Root { dir, kind: Kind::Skills { depth }, prefix: None }
    }
    pub fn commands(dir: PathBuf) -> Root {
        Root { dir, kind: Kind::Commands, prefix: None }
    }
    pub fn of_plugin(mut self, plugin: &str) -> Root {
        self.prefix = Some(plugin.to_string());
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

/// A worktree's skills are read through `Held`: a worktree in Documents waits on a prompt.
static WORKTREES: LazyLock<Held<Vec<(Harness, File)>>> = LazyLock::new(|| Held::new("skills", read_worktree, || {}));

/// The skills of `folders` and `accounts` (by name, as config.toml has them).
pub fn list(folders: &[String], accounts: &[String], cfg: &Config) -> Listed {
    let uses = uses(cfg);
    let paths: Vec<PathBuf> = folders.iter().map(|f| crate::util::expand(f)).collect();
    let mut read = WORKTREES.read(paths.iter().cloned(), HELD_WAIT);
    let folders = folders.iter().zip(&paths).map(|(folder, path)| FolderSkills { folder: folder.clone(), skills: merged(read.remove(path).unwrap_or_default(), &uses) }).collect();
    let accounts = accounts
        .iter()
        .filter_map(|name| cfg.accounts.iter().find(|a| &a.name == name))
        .map(|a| AccountSkills { account: a.name.clone(), skills: merged(read_account(a), &uses) })
        .collect();
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
    harness::all().flat_map(|a| dirs.iter().flat_map(|dir| a.worktree_skills(dir)).flat_map(|root| read_root(&root)).map(|f| (a.harness(), f)).collect::<Vec<_>>()).collect()
}

fn read_account(account: &AccountCfg) -> Vec<(Harness, File)> {
    let Some(adapter) = harness::of(account) else { return Vec::new() };
    adapter.account_skills(account).iter().flat_map(read_root).map(|f| (adapter.harness(), f)).collect()
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
    let mut seen = HashSet::new();
    match root.kind {
        Kind::Skills { depth } => walk_skills(&root.dir, depth, root, &mut seen, &mut out),
        Kind::Commands => walk_commands(&root.dir, &root.dir, 3, root, &mut out),
    }
    out
}

/// A folder's entries, by name, the hidden ones left out: links are followed, as the harnesses
/// follow them (a repo's `.agents/skills` is often a link to its `.claude/skills`).
fn entries(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| !n.starts_with('.'))).take(MAX_ENTRIES).collect();
    paths.sort();
    paths
}

fn walk_skills(dir: &Path, depth: usize, root: &Root, seen: &mut HashSet<PathBuf>, out: &mut Vec<File>) {
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
                if seen.insert(std::fs::canonicalize(&file).unwrap_or(file)) {
                    let folder = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    out.extend(skill_file(&text, folder, root.prefix.as_deref()));
                }
            }
            Err(_) => walk_skills(&path, depth - 1, root, seen, out),
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
    typeable(&name).then(|| File {
        name,
        description: one_line(front.get("description").map(String::as_str).filter(|d| !d.is_empty()).unwrap_or(first_line(body))),
        hint: front.get("argument-hint").filter(|h| !h.is_empty()).cloned(),
        typed: front.get("user-invocable").map(String::as_str) != Some("false"),
        by_model: front.get("disable-model-invocation").map(String::as_str) != Some("true"),
    })
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
        // one in a folder above the one asked about.
        write(&dir.join(".claude/skills/ship/SKILL.md"), "---\nname: ship\ndescription: Ships\nargument-hint: \"[branch]\"\n---\n");
        write(&dir.join(".claude/skills/conventions/SKILL.md"), "---\nname: conventions\ndescription: House rules\nuser-invocable: false\n---\n");
        write(&dir.join(".claude/commands/git/review.md"), "Review the diff\n");
        write(&dir.join(".codex/skills/probe/SKILL.md"), "---\nname: probe\ndescription: Probes\n---\n");
        std::fs::create_dir_all(dir.join(".agents")).unwrap();
        std::os::unix::fs::symlink("../.claude/skills", dir.join(".agents/skills")).unwrap();
        write(&dir.join("app/.claude/skills/local/SKILL.md"), "---\ndescription: Only here\n---\n");

        let cfg = Config::default();
        let app = dir.join("app").to_string_lossy().into_owned();
        let listed = list(&[app.clone()], &[], &cfg);
        assert_eq!(listed.folders[0].folder, app);
        let said: Vec<(String, Vec<(String, String)>)> = listed.folders[0].skills.iter().map(|s| (s.name.clone(), s.written.clone().into_iter().collect())).collect();
        let pair = |h: &str, w: &str| (h.to_string(), w.to_string());
        assert_eq!(
            said,
            vec![
                ("git:review".to_string(), vec![pair("claude", "/git:review")]),
                ("local".to_string(), vec![pair("claude", "/local")]),
                ("probe".to_string(), vec![pair("codex", "$probe")]),
                ("ship".to_string(), vec![pair("claude", "/ship"), pair("codex", "$ship")]),
                // Kept off the menu by its file: last.
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
