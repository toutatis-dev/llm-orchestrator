use std::path::Path;

pub struct SimpleContext;

impl SimpleContext {
    pub fn new() -> Self {
        Self
    }

    pub fn gather(&self, _root: &Path) -> anyhow::Result<String> {
        // TODO: Implement context gathering
        Ok(String::new())
    }
}
