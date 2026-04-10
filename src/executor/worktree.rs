use anyhow::Result;
use git2::Repository;
use std::path::Path;

pub struct WorktreeManager {
    repo: Repository,
}

impl WorktreeManager {
    pub fn new(repo_path: &Path) -> Result<Self> {
        let repo = Repository::open(repo_path)?;
        Ok(Self { repo })
    }
}
