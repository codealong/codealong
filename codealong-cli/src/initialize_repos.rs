use std::collections::VecDeque;
use std::iter::FromIterator;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::error::Result;
use crate::repo::Repo;
use crate::ui::ProgressPool;

/// Clone and/or fetch all repos
pub fn initialize_repos(matches: &clap::ArgMatches, repos: Vec<Repo>) -> Result<()> {
    let num_threads = matches
        .value_of("concurrency")
        .unwrap_or_else(|| "6")
        .parse::<i32>()?;
    let m = Arc::new(ProgressPool::new(repos.len() as u64));
    m.set_message("Repos initialized");
    let repos = Arc::new(Mutex::new(VecDeque::from_iter(repos)));
    for _ in 0..num_threads {
        let repos = repos.clone();
        let m = m.clone();
        let mut pb = m.add();
        thread::spawn(move || loop {
            let repo = {
                let mut repos = repos.lock().unwrap();
                repos.pop_front()
            };
            if let Some(repo) = repo {
                pb.reset(repo.display_name().to_owned());
                pb.set_message("fetching");
                let cb = |cur: usize, total: usize| {
                    pb.set_length(total as u64);
                    pb.set_position(cur as u64);
                };
                match repo.init(Some(Box::new(cb))) {
                    Ok(_) => pb.set_message("finished"),
                    Err(e) => {
                        pb.set_message(&format!("error: {}", e));
                        thread::sleep(std::time::Duration::from_secs(2));
                    }
                };
                m.inc(1);
            } else {
                pb.finish();
                break;
            }
        });
    }
    m.join_and_clear()?;
    Ok(())
}
