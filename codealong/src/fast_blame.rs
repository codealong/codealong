use git2::{Oid, Repository};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

use crate::error::Error;

use regex::Regex;

// libgit2 has an extremely slow blame implementation:
// https://github.com/libgit2/libgit2/issues/3027
// so we instead defer to a git binary on the current path
pub struct FastBlame {
    child: Child,
    reader: RefCell<BufReader<ChildStdout>>,
    line_map: RefCell<HashMap<usize, Oid>>,
}

impl FastBlame {
    pub fn new(
        repo: &Repository,
        parent: &Oid,
        old_path: &Path,
        churn_cutoff: u64,
    ) -> Result<FastBlame, Error> {
        let mut child = Command::new("git")
            .current_dir(repo.path())
            .arg("blame")
            .arg(parent.to_string())
            .arg("-s")
            .arg("-l")
            .arg("-p")
            .arg("--incremental")
            .arg(format!("--since={}.days", churn_cutoff))
            .arg("--")
            .arg(old_path)
            .stdout(Stdio::piped())
            .spawn()
            .expect("failed to execute git blame");

        Ok(FastBlame {
            reader: RefCell::new(BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or_else(|| "Could not capture standard output.")?,
            )),
            child: child,
            line_map: RefCell::new(HashMap::new()),
        })
    }

    pub fn get_line(&self, lineno: usize) -> Option<Oid> {
        if let Some(l) = self.line_map.borrow().get(&lineno) {
            return Some(l.clone());
        }

        self.scan_for_line(lineno)
    }

    // see https://git-scm.com/docs/git-blame#_the_porcelain_format
    fn scan_for_line(&self, lineno: usize) -> Option<Oid> {
        let mut line = String::new();
        let mut reader = self.reader.borrow_mut();
        let mut line_map = self.line_map.borrow_mut();
        while let Ok(num_bytes) = reader.read_line(&mut line) {
            if num_bytes == 0 {
                break;
            }
            if let Some(blame_line) = BlameLine::new(&line) {
                line_map.insert(blame_line.original_lineno, blame_line.oid);
                if blame_line.original_lineno == lineno {
                    return Some(blame_line.oid.clone());
                }
            }
            line.clear();
        }
        None
    }
}

impl Drop for FastBlame {
    fn drop(&mut self) {
        // need this to prevent zombie "Z+" processes from occuring
        self.child.kill().expect("unable to kill process");
        self.child.wait().expect("unable to wait for process");
    }
}

struct BlameLine {
    oid: Oid,
    original_lineno: usize,
}

impl BlameLine {
    pub fn new(line: &str) -> Option<BlameLine> {
        lazy_static! {
            static ref BLAME_LINE_REGEX: Regex =
                Regex::new(r"^([0-9a-f]{40}) (\d+) \d+ \d+\n$").unwrap();
        }
        if let Some(captures) = BLAME_LINE_REGEX.captures(line) {
            Some(BlameLine {
                oid: Oid::from_str(&captures[1]).unwrap(),
                original_lineno: captures[2].parse().unwrap(),
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn it_works() {
        let repo = Repository::open(Path::new("./fixtures/repos/simple")).unwrap();
        let blame = FastBlame::new(
            &repo,
            &Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap(),
            Path::new("README.md"),
            14,
        ).unwrap();
        assert!(
            Some(Oid::from_str("86d242301830075e93ff039a4d1e88673a4a3020").unwrap())
                == blame.get_line(1)
        );
    }
}
