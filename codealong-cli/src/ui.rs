use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::io;
use std::sync::{Arc, Mutex};

/// Encapsulates a MultiProgress with a progres bar to track an overall count
pub struct ProgressPool {
    m: Arc<MultiProgress>,
    overall_pb: Arc<ProgressBar>,
    remaining: Arc<Mutex<u64>>,
}

impl ProgressPool {
    pub fn new(count: u64, visible: bool) -> ProgressPool {
        let m = MultiProgress::new();
        if !visible {
            m.set_draw_target(ProgressDrawTarget::hidden());
        }
        let overall_pb = m.add(ProgressBar::new(count));
        overall_pb.set_style(ProgressStyle::default_bar().template("{pos:>7}/{len:7} {msg}"));
        let remaining = count;
        ProgressPool {
            m: Arc::new(m),
            overall_pb: Arc::new(overall_pb),
            remaining: Arc::new(Mutex::new(remaining)),
        }
    }

    pub fn add(&self) -> NamedProgressBar {
        NamedProgressBar::new(self.m.add(ProgressBar::new(0)))
    }

    pub fn inc(&self, delta: u64) {
        self.overall_pb.inc(delta);
        let mut remaining = self.remaining.lock().unwrap();
        *remaining -= 1;
        if *remaining <= 0 {
            self.overall_pb.finish();
        }
    }

    pub fn set_message(&self, msg: &str) {
        self.overall_pb.set_message(msg);
    }

    pub fn join_and_clear(&self) -> io::Result<()> {
        self.m.join_and_clear()
    }
}

pub struct NamedProgressBar {
    pb: ProgressBar,
    name: Option<String>,
}

impl NamedProgressBar {
    pub fn new(pb: ProgressBar) -> NamedProgressBar {
        let b = NamedProgressBar { pb, name: None };
        b.reset_style();
        b
    }

    pub fn reset(&mut self, name: String) {
        self.name.replace(name);
        self.reset_style();
        self.pb.set_position(0);
    }

    pub fn set_length(&self, pos: u64) {
        self.pb.set_length(pos)
    }

    pub fn set_position(&self, pos: u64) {
        self.pb.set_position(pos);
    }

    pub fn set_message(&self, msg: &str) {
        self.pb.set_message(msg)
    }

    pub fn inc(&self, delta: u64) {
        self.pb.inc(delta)
    }

    pub fn finish(&self) {
        self.pb.finish_with_message("done")
    }

    fn reset_style(&self) {
        self.pb.set_style(self.pb_style());
    }

    fn pb_style(&self) -> ProgressStyle {
        if let Some(ref name) = self.name {
            ProgressStyle::default_bar()
                .template(&format!(
                    "[{{elapsed_precise}}] {{bar:40.green/cyan}} {{pos:>7}}/{{len:7}} {} - {{msg}}",
                    name
                ))
                .progress_chars("##-")
        } else {
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.green/cyan} {pos:>7}/{len:7} {msg}")
                .progress_chars("##-")
        }
    }
}
