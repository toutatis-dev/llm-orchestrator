use anyhow::Result;
use git2::Repository;
use std::path::Path;

pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    pub fn open(path: &Path) -> Result<Self> {
        let repo = Repository::open(path)?;
        Ok(Self { repo })
    }
}
