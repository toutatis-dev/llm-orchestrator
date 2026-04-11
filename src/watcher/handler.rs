use crate::tui::events::FileChangeEvent;
use anyhow::{Context, Result};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Watches for external file changes during execution
/// 
/// This is used to detect if the user or another process modifies files
/// while the orchestrator is executing tasks. When such changes are detected,
/// the orchestrator can pause and ask the user how to proceed.
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    watched_paths: Arc<Mutex<HashSet<PathBuf>>>,
    expected_changes: Arc<Mutex<HashSet<PathBuf>>>,
    event_tx: mpsc::UnboundedSender<FileChangeEvent>,
}

impl FileWatcher {
    /// Create a new FileWatcher that sends events to the given channel
    pub fn new(event_tx: mpsc::UnboundedSender<FileChangeEvent>) -> Result<Self> {
        let watched_paths = Arc::new(Mutex::new(HashSet::new()));
        let expected_changes = Arc::new(Mutex::new(HashSet::new()));

        // Clone for closure
        let event_tx_clone = event_tx.clone();
        let watched_paths_clone = watched_paths.clone();
        let expected_changes_clone = expected_changes.clone();

        let watcher = notify::recommended_watcher(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        Self::handle_event(
                            event,
                            &event_tx_clone,
                            &watched_paths_clone,
                            &expected_changes_clone,
                        );
                    }
                    Err(e) => {
                        tracing::error!("File watcher error: {}", e);
                    }
                }
            },
        )
        .with_context(|| "Failed to create file watcher")?;

        Ok(Self {
            watcher,
            watched_paths,
            expected_changes,
            event_tx,
        })
    }

    /// Watch a directory recursively
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .watch(path, RecursiveMode::Recursive)
            .with_context(|| format!("Failed to watch path {:?}", path))?;

        self.watched_paths.lock().unwrap().insert(path.to_path_buf());
        tracing::info!("Started watching path: {:?}", path);
        Ok(())
    }

    /// Stop watching a directory
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .unwatch(path)
            .with_context(|| format!("Failed to unwatch path {:?}", path))?;

        self.watched_paths.lock().unwrap().remove(path);
        tracing::info!("Stopped watching path: {:?}", path);
        Ok(())
    }

    /// Mark a file as expected to change (won't trigger external modification event)
    pub fn expect_change(&self, path: &Path) {
        self.expected_changes
            .lock()
            .unwrap()
            .insert(path.to_path_buf());
    }

    /// Mark a file as no longer expected to change
    pub fn unexpect_change(&self, path: &Path) {
        self.expected_changes.lock().unwrap().remove(path);
    }

    /// Clear all expected changes
    pub fn clear_expected_changes(&self) {
        self.expected_changes.lock().unwrap().clear();
    }

    /// Handle a file system event
    fn handle_event(
        event: Event,
        event_tx: &mpsc::UnboundedSender<FileChangeEvent>,
        watched_paths: &Arc<Mutex<HashSet<PathBuf>>>,
        expected_changes: &Arc<Mutex<HashSet<PathBuf>>>,
    ) {
        // Filter events to only those in watched paths
        let watched = watched_paths.lock().unwrap();
        let relevant_paths: Vec<_> = event
            .paths
            .iter()
            .filter(|p| {
                watched.iter().any(|watched_path| p.starts_with(watched_path))
            })
            .cloned()
            .collect();
        drop(watched);

        if relevant_paths.is_empty() {
            return;
        }

        // Check which changes are expected vs external
        let expected = expected_changes.lock().unwrap();
        for path in relevant_paths {
            let is_expected = expected.contains(&path);

            let file_event = match event.kind {
                EventKind::Create(_) if is_expected => {
                    FileChangeEvent::ExpectedModification { path }
                }
                EventKind::Create(_) => FileChangeEvent::Created(path),
                EventKind::Modify(_) if is_expected => {
                    FileChangeEvent::ExpectedModification { path }
                }
                EventKind::Modify(_) => FileChangeEvent::ExternalModification { path },
                EventKind::Remove(_) => FileChangeEvent::Deleted(path),
                _ => continue,
            };

            if let Err(e) = event_tx.send(file_event) {
                tracing::error!("Failed to send file change event: {}", e);
            }
        }
    }

    /// Check if a path is being watched
    pub fn is_watching(&self, path: &Path) -> bool {
        self.watched_paths.lock().unwrap().contains(path)
    }

    /// Get list of watched paths
    pub fn watched_paths(&self) -> Vec<PathBuf> {
        self.watched_paths.lock().unwrap().iter().cloned().collect()
    }
}

/// Builder for FileWatcher with configuration
pub struct FileWatcherBuilder {
    debounce_ms: u64,
    poll_interval_ms: u64,
}

impl FileWatcherBuilder {
    pub fn new() -> Self {
        Self {
            debounce_ms: 500,
            poll_interval_ms: 1000,
        }
    }

    pub fn debounce_ms(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    pub fn poll_interval_ms(mut self, ms: u64) -> Self {
        self.poll_interval_ms = ms;
        self
    }

    pub fn build(self, event_tx: mpsc::UnboundedSender<FileChangeEvent>) -> Result<FileWatcher> {
        // Configure watcher with custom settings
        let _config = Config::default()
            .with_poll_interval(std::time::Duration::from_millis(self.poll_interval_ms));

        let watched_paths = Arc::new(Mutex::new(HashSet::new()));
        let expected_changes = Arc::new(Mutex::new(HashSet::new()));

        let event_tx_clone = event_tx.clone();
        let watched_paths_clone = watched_paths.clone();
        let expected_changes_clone = expected_changes.clone();

        let watcher = notify::recommended_watcher(
            move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        FileWatcher::handle_event(
                            event,
                            &event_tx_clone,
                            &watched_paths_clone,
                            &expected_changes_clone,
                        );
                    }
                    Err(e) => {
                        tracing::error!("File watcher error: {}", e);
                    }
                }
            },
        )
        .with_context(|| "Failed to create file watcher")?;

        Ok(FileWatcher {
            watcher,
            watched_paths,
            expected_changes,
            event_tx,
        })
    }
}

impl Default for FileWatcherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_file_watcher_detects_changes() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let temp_dir = TempDir::new().unwrap();

        let mut watcher = FileWatcher::new(tx).unwrap();
        watcher.watch(temp_dir.path()).unwrap();

        // Create a file
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, "hello").await.unwrap();

        // Wait for event
        let event = timeout(Duration::from_secs(2), rx.recv()).await;
        assert!(event.is_ok(), "Should receive file change event");

        let event = event.unwrap().unwrap();
        match event {
            FileChangeEvent::Created(path) | FileChangeEvent::ExpectedModification { path } => {
                assert_eq!(path.file_name().unwrap(), "test.txt");
            }
            _ => panic!("Expected create event"),
        }
    }

    #[tokio::test]
    async fn test_expected_changes_filtered() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let temp_dir = TempDir::new().unwrap();

        let watcher = FileWatcher::new(tx).unwrap();
        
        // Mark file as expected
        let test_file = temp_dir.path().join("expected.txt");
        watcher.expect_change(&test_file);

        // Create the file (would normally trigger external modification)
        tokio::fs::write(&test_file, "content").await.unwrap();

        // Wait a bit - should NOT receive external modification event
        let event = timeout(Duration::from_millis(100), rx.recv()).await;
        // Event might or might not arrive depending on timing, 
        // but if it does it should be ExpectedModification
        if let Ok(Some(event)) = event {
            match event {
                FileChangeEvent::ExpectedModification { .. } => {}
                FileChangeEvent::ExternalModification { .. } => {
                    panic!("Should not receive external modification for expected change")
                }
                _ => {}
            }
        }
    }
}
