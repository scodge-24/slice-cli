use std::process::Command;

use rustc_hash::FxHashSet;

use crate::context::Context;

pub trait GitBackend {
    fn changed_files(&self, ctx: &Context, files: &[String], verified_at: &str) -> GitChanges;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessGitBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitChanges {
    Changed(Vec<String>),
    BadRevision,
}

impl GitBackend for ProcessGitBackend {
    fn changed_files(&self, ctx: &Context, files: &[String], verified_at: &str) -> GitChanges {
        let mut changed = FxHashSet::default();
        if !verified_at.is_empty() {
            let mut command = Command::new("git");
            command
                .args(["diff", "--name-only", &format!("{verified_at}..HEAD"), "--"])
                .args(files)
                .current_dir(ctx.repo_root());
            match command.output() {
                Ok(output) if output.status.success() => {
                    changed.extend(lines(&output.stdout));
                }
                Ok(_) => return GitChanges::BadRevision,
                Err(_) => return GitChanges::Changed(Vec::new()),
            }
        }

        let mut command = Command::new("git");
        command
            .args(["diff", "--name-only", "HEAD", "--"])
            .args(files)
            .current_dir(ctx.repo_root());
        if let Ok(output) = command.output()
            && output.status.success()
        {
            changed.extend(lines(&output.stdout));
        }

        let mut ordered = changed.into_iter().collect::<Vec<_>>();
        ordered.sort();
        GitChanges::Changed(ordered)
    }
}

fn lines(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
        .into_iter()
}
