use llm_orchestrator::executor::worktree::WorktreeManager;
use llm_orchestrator::git::branch::BranchManager;
use llm_orchestrator::git::cleanup::BranchCleanup;
use std::path::Path;
use tempfile::TempDir;

fn setup_test_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let repo = git2::Repository::init(&temp_dir).unwrap();

    // Configure git user for commits
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();

    // Create initial file
    let readme_path = temp_dir.path().join("README.md");
    std::fs::write(&readme_path, "# Initial README\n").unwrap();

    // Create initial commit
    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    index.write().unwrap();

    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap();

    // Rename master/main branch to "main" if needed
    let head = repo.head().unwrap();
    if head.shorthand() == Some("master") {
        // Create main branch and checkout
        let commit = head.peel_to_commit().unwrap();
        repo.branch("main", &commit, false).unwrap();
        repo.set_head("refs/heads/main").unwrap();
    }

    temp_dir
}

#[test]
fn test_worktree_lifecycle_create_write_commit_cleanup() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    // Create managers
    let worktree_manager = WorktreeManager::new(repo_path).unwrap();
    let branch_manager = BranchManager::new(repo_path).unwrap();
    let cleanup = BranchCleanup::new(repo_path).unwrap();

    let session_id = "test-session-001";
    let batch_id = 1;
    let task_id = "task-hello-world";

    // Step 1: Create worktree
    let worktree = worktree_manager
        .create_worktree(session_id, batch_id, &task_id.to_string(), "main")
        .expect("Failed to create worktree");

    assert!(worktree.path.exists(), "Worktree directory should exist");
    assert!(
        worktree.branch.contains(&task_id),
        "Branch name should contain task ID"
    );
    println!("✓ Created worktree at {:?}", worktree.path);

    // Step 2: Write files to worktree
    let src_dir = worktree.path.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();

    let main_rs = src_dir.join("main.rs");
    std::fs::write(&main_rs, "fn main() { println!(\"Hello, world!\"); }\n").unwrap();

    let cargo_toml = worktree.path.join("Cargo.toml");
    std::fs::write(
        &cargo_toml,
        r#"[package]
name = "hello-world"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    println!("✓ Written files to worktree");

    // Step 3: Commit changes in worktree
    let worktree_repo = git2::Repository::open(&worktree.path).unwrap();
    let mut index = worktree_repo.index().unwrap();
    index
        .add_all(&["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();

    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = worktree_repo.find_tree(tree_id).unwrap();

    let parent_commit = worktree_repo.head().unwrap().peel_to_commit().unwrap();
    let commit_id = worktree_repo
        .commit(
            Some("HEAD"),
            &sig,
            &sig,
            "[orchestrator] Add hello world program",
            &tree,
            &[&parent_commit],
        )
        .expect("Failed to commit");

    println!("✓ Created commit: {}", commit_id);

    // Step 4: Verify branch exists in main repo
    assert!(
        branch_manager.branch_exists(&worktree.branch),
        "Branch should exist in main repo"
    );

    // Step 5: Create merge branch and merge worktree
    let merge_branch = format!("orchestrator/{}/batch-{}-merged", session_id, batch_id);
    branch_manager
        .create_branch(&merge_branch, "main")
        .expect("Failed to create merge branch");

    branch_manager
        .merge_branch(
            &worktree.branch,
            &format!("Merge task {} into batch", task_id),
        )
        .expect("Failed to merge task branch");

    println!("✓ Merged task into batch branch");

    // Step 6: Remove worktree (keep branch for now)
    let worktree_path = worktree.path.clone();
    worktree_manager
        .remove_worktree(worktree)
        .expect("Failed to remove worktree");

    assert!(
        !worktree_path.exists(),
        "Worktree directory should be removed"
    );
    println!("✓ Removed worktree");

    // Step 7: Cleanup session
    // Note: remove_worktree() already deleted the task branch
    // cleanup_success() will delete the merged branch
    let report = cleanup
        .cleanup_success(session_id)
        .expect("Failed to cleanup session");

    assert_eq!(
        report.branches_deleted, 1,
        "Should delete merged branch (task branch already deleted by remove_worktree)"
    );
    assert!(
        !branch_manager.branch_exists(&merge_branch),
        "Merge branch should be deleted"
    );

    println!(
        "✓ Cleanup complete: {} branches deleted",
        report.branches_deleted
    );
    println!("  Disk space reclaimed: {}", report.format_disk_space());
}

#[test]
fn test_parallel_worktree_creation() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let worktree_manager = WorktreeManager::new(repo_path).unwrap();

    let session_id = "parallel-test";
    let batch_id = 1;

    // Create multiple worktrees in "parallel" (sequentially for test)
    let task_ids = vec!["task-1", "task-2", "task-3"];
    let mut worktrees = Vec::new();

    for task_id in &task_ids {
        let worktree = worktree_manager
            .create_worktree(session_id, batch_id, &task_id.to_string(), "main")
            .expect(&format!("Failed to create worktree for {}", task_id));
        worktrees.push(worktree);
    }

    // Verify all worktrees exist
    for (i, worktree) in worktrees.iter().enumerate() {
        assert!(
            worktree.path.exists(),
            "Worktree {} should exist",
            task_ids[i]
        );
    }

    println!("✓ Created {} parallel worktrees", worktrees.len());

    // Cleanup all worktrees
    for worktree in worktrees {
        worktree_manager
            .remove_worktree(worktree)
            .expect("Failed to remove worktree");
    }

    println!("✓ All worktrees cleaned up");
}

#[test]
fn test_failed_session_preserves_branches() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();

    let worktree_manager = WorktreeManager::new(repo_path).unwrap();
    let branch_manager = BranchManager::new(repo_path).unwrap();
    let cleanup = BranchCleanup::new(repo_path).unwrap();

    let session_id = "failed-session";

    // Create worktree
    let worktree = worktree_manager
        .create_worktree(session_id, 1, &"task-fail".to_string(), "main")
        .unwrap();

    let branch_name = worktree.branch.clone();

    // For a failed session, we only remove the worktree directory
    // but KEEP the branch for forensics
    std::fs::remove_dir_all(&worktree.path).unwrap();

    // Cleanup as "failed" session - should preserve branches
    let report = cleanup.cleanup_failed_session(session_id).unwrap();

    assert_eq!(
        report.branches_deleted, 0,
        "Failed session should preserve branches"
    );
    assert!(
        branch_manager.branch_exists(&branch_name),
        "Branch should still exist for forensics"
    );

    println!("✓ Failed session preserved branch: {}", branch_name);

    // Now cleanup successfully
    let success_report = cleanup.cleanup_success(session_id).unwrap();
    assert_eq!(success_report.branches_deleted, 1);
    assert!(!branch_manager.branch_exists(&branch_name));

    println!("✓ Manual cleanup succeeded");
}

#[test]
fn test_disk_usage_tracking() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let cleanup = BranchCleanup::new(repo_path).unwrap();

    // Initially should be empty
    let (count, size) = cleanup.disk_usage_summary().unwrap();
    assert_eq!(count, 0, "Should start with 0 worktrees");
    println!("✓ Initial disk usage: {} bytes", size);
}
