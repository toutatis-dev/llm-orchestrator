use anyhow::Result;
use notify::{Event, RecommendedWatcher, Watcher};
use std::path::Path;

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn new() -> Result<Self> {
        let watcher = notify::recommended_watcher(|res: Result<Event, _>| {
            if let Ok(event) = res {
                println!("File event: {:?}", event);
            }
        })?;

        Ok(Self { _watcher: watcher })
    }

    pub fn watch(&mut self, _path: &Path) -> Result<()> {
        // TODO: Implement watching
        Ok(())
    }
}
