use anyhow::{anyhow, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::lockfile::SproutLock;

const SYMLINKS_DIR: &str = "symlinks";
const LOCAL_PREFIX: &str = "/local/";

enum SymlinkStatus {
    UpToDate,
    Modified { reason: ModificationReason },
    Deleted,
    #[allow(dead_code)] // May be used in the future
    Untracked,
}

enum ModificationReason {
    DifferentHash,
    RegularFile,
    ContentModified,
}

fn hash_symlink_target(path: &Path, tracking_path: &str) -> Result<String> {
    let target = fs::read_link(path)?;

    // Extract the relative path within sprout/symlinks for the hash
    let target_str = target.to_string_lossy();
    let search_pattern = format!("/{}/", SYMLINKS_DIR);
    let relative_target = target_str.find(&search_pattern)
        .map(|pos| &target_str[pos + search_pattern.len()..])
        .context("Symlink target is not within a sprout/symlinks directory")?;

    // Get the relative path from tracking directory for the symlink location
    let normalized_home = normalize_path(tracking_path);
    let home_path = path.to_string_lossy();

    // Normalize paths by optionally removing /local prefix
    let normalized_path = normalize_path(&home_path);

    let relative_home_path = normalized_path.strip_prefix(normalized_home).map(|s| s.trim_start_matches('/'))
        .context("Symlink path is not within tracking directory")?;

    // Hash only the relative mapping: relative_home_path -> relative_symlinks_path
    let mapping = format!("{}:{}", relative_home_path, relative_target);

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(mapping.as_bytes());
    Ok(hex::encode(hasher.finalize()))
}

// On some systems (e.g., NFS-mounted home directories), /home/<user> and /local/home/<user>
// refer to the same physical path. The /local prefix is often used for local disk access
// to avoid network latency. This normalization ensures consistent hashing and symlink checking
// by treating both paths as equivalent.
//
// Note: We use string manipulation instead of fs::canonicalize because:
// - Works on non-existent paths (canonicalize requires the path to exist)
// - Doesn't follow symlinks we're managing (we need the symlink path, not its target)
// - Faster (no filesystem I/O)
// - More predictable (doesn't depend on current filesystem state)
fn normalize_path(path: &str) -> &str {
    if path.starts_with(LOCAL_PREFIX) {
        &path[LOCAL_PREFIX.len() - 1..] // Remove "/local" prefix
    } else {
        path
    }
}

/// Adds a local file or directory to be managed by Sprout.
pub fn add_file(sprout_path: &str, path: PathBuf, recursive: bool, dry_run: bool, tracking_path: &str) -> Result<()> {
    debug!("Original path: {:?}", path);
    
    // Normalize the path to handle /local prefix
    let path_str = path.to_string_lossy();
    let normalized_path = normalize_path(&path_str);
    let path = PathBuf::from(&normalized_path);
    debug!("Using path: {:?}", path);

    let home = PathBuf::from(tracking_path);
    debug!("Tracking directory: {}", home.display());

    // Convert to absolute path if relative
    let target = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };

    // Normalize the target path for comparison
    let target_str = target.to_string_lossy();
    let normalized_target = normalize_path(&target_str);
    debug!("Target as string: {}", target_str);
    debug!("Normalized target: {}", normalized_target);

    let normalized_home = normalize_path(tracking_path);
    debug!("Normalized tracking path: {}", normalized_home);

    // Check if the target is within the tracking directory
    debug!("Checking if '{}' starts with '{}'", normalized_target, normalized_home);
    if !normalized_target.starts_with(normalized_home) {
        return Err(anyhow!("Path must be within your tracking directory"));
    }

    // Get relative path from tracking directory
    let relative_home_path = normalized_target.strip_prefix(normalized_home).unwrap().trim_start_matches('/');
    debug!("Relative tracking path: {}", relative_home_path);

    // Check if this path or any parent is already managed
    let mut index = SproutLock::load(sprout_path)?;
    debug!("Checking if path '{}' or its parents are already managed by sprout", relative_home_path);

    // Check if any parent directory is already managed
    let path_parts: Vec<&str> = relative_home_path.split('/').collect();
    for i in 1..path_parts.len() {
        let parent_path = path_parts[..i].join("/");
        debug!("Checking parent directory: '{}'", parent_path);
        if index.symlinks.contains_key(&parent_path) {
            return Err(anyhow!(
                "Cannot add '{}' because parent directory '{}' is already managed by sprout",
                relative_home_path, parent_path
            ));
        }
    }

    // Check if any child paths are already managed
    for existing_path in index.symlinks.keys() {
        if existing_path.starts_with(&format!("{}/", relative_home_path)) {
            return Err(anyhow!(
                "Cannot add '{}' because child path '{}' is already managed by sprout",
                relative_home_path, existing_path
            ));
        }
    }

    debug!("Path '{}' is not already managed and can be added", relative_home_path);

    if dry_run {
        println!("Would add: {}", target.display());
        return Ok(());
    }

    // Create the symlinks directory structure
    let sprout_target = Path::new(sprout_path).join(SYMLINKS_DIR).join(relative_home_path);
    if let Some(parent) = sprout_target.parent() {
        fs::create_dir_all(parent)
            .context(format!("Failed to create directory structure for {}", sprout_target.display()))?;
    }

    // Copy the file/directory to sprout
    if target.is_dir() {
        if !recursive {
            return Err(anyhow!("Path {} is a directory. Use --recursive to add directories", target.display()));
        }
        info!("Copying directory {} to {}", target.display(), sprout_target.display());
        copy_dir_all(&target, &sprout_target)?;
    } else if target.is_file() {
        info!("Copying file {} to {}", target.display(), sprout_target.display());
        fs::copy(&target, &sprout_target)
            .context(format!("Failed to copy file {} to {}", target.display(), sprout_target.display()))?;
    } else {
        return Err(anyhow!("Path {} is neither a file nor directory", target.display()));
    }

    // Remove the original file/directory
    info!("Removing existing entry at {}", target.display());
    if target.is_dir() {
        fs::remove_dir_all(&target)
            .context(format!("Failed to remove directory {}", target.display()))?;
    } else {
        fs::remove_file(&target)
            .context(format!("Failed to remove file {}", target.display()))?;
    }

    // Create absolute symlink path
    let absolute_sprout_path = fs::canonicalize(sprout_path)?;
    let absolute_sprout_target = absolute_sprout_path.join(SYMLINKS_DIR).join(relative_home_path);

    // Create symlink
    info!("Creating symlink {} -> {}", target.display(), absolute_sprout_target.display());
    #[cfg(unix)]
    std::os::unix::fs::symlink(&absolute_sprout_target, &target)
        .context(format!("Failed to create symlink {} -> {}", target.display(), absolute_sprout_target.display()))?;

    // Calculate hash and update index
    let hash = hash_symlink_target(&target, tracking_path)?;
    index.symlinks.insert(relative_home_path.to_string(), hash);
    index.save(sprout_path)?;

    info!("Successfully added and symlinked {}", normalized_target);
    Ok(())
}

/// Restores symlinks from the index, repairing broken or missing ones.
pub fn restore_symlinks(sprout_path: &str, dry_run: bool, _tracking_path: &str) -> Result<()> {
    let index = SproutLock::load(sprout_path)?;
    let home = dirs::home_dir().context("Could not find home directory")?;

    if index.symlinks.is_empty() {
        info!("No symlinks found in index. Nothing to restore.");
        return Ok(());
    }

    let mut restore_count = 0;

    for home_path_str in index.symlinks.keys() {
        // All paths in index are now relative - convert to absolute
        let home_path = home.join(home_path_str);
        let expected_target = fs::canonicalize(Path::new(sprout_path))?.join(SYMLINKS_DIR).join(home_path_str);

        let should_restore = if !home_path.exists() {
            true
        } else if let Ok(actual_target) = fs::read_link(&home_path) {
            actual_target != expected_target
        } else {
            true
        };

        if should_restore {
            restore_count += 1;
            if dry_run {
                println!("Would restore symlink: {} -> {}", home_path.display(), expected_target.display());
                if home_path.exists() {
                    println!("  (Would remove existing: {})", home_path.display());
                }
                continue;
            }

            // Force remove anything that exists at the target location
            if home_path.exists() || home_path.is_symlink() {
                debug!("Removing existing entry at {}", home_path.display());
                if home_path.is_dir() && !home_path.is_symlink() {
                    fs::remove_dir_all(&home_path)
                        .context(format!("Failed to remove directory {}", home_path.display()))?;
                } else {
                    // This handles both regular files and symlinks (including broken symlinks)
                    fs::remove_file(&home_path)
                        .context(format!("Failed to remove file/symlink {}", home_path.display()))?;
                }
            }

            // Ensure parent directory exists
            if let Some(parent) = home_path.parent() {
                fs::create_dir_all(parent)
                    .context(format!("Failed to create parent directory for {}", home_path.display()))?;
            }

            debug!("Creating symlink {} -> {}", home_path.display(), expected_target.display());
            #[cfg(unix)]
            std::os::unix::fs::symlink(&expected_target, &home_path)
                .context(format!("Failed to create symlink {} -> {}", home_path.display(), expected_target.display()))?;

            info!(
                "Restored symlink: {} -> {}",
                home_path.display(),
                expected_target.display()
            );
        }
    }

    if dry_run {
        println!("Would restore {} symlink(s).", restore_count);
    } else {
        info!("Symlink restoration complete.");
    }
    Ok(())
}

/// Shows the status of tracked dotfiles.
pub fn check_symlinks(sprout_path: &str, show_all: bool, tracking_path: &str) -> Result<()> {
    use colored::Colorize;
    use std::process::Command;

    let home = dirs::home_dir().context("Could not find home directory")?;
    let index = SproutLock::load(sprout_path)?;

    debug!("Home directory: {}", home.display());
    debug!("Loaded index with {} tracked symlinks", index.symlinks.len());

    // Get git status for symlinks directory
    let git_output = Command::new("git")
        .args(&["-C", sprout_path, "status", "--porcelain", SYMLINKS_DIR])
        .output()
        .ok();
    
    let mut git_modified = std::collections::HashSet::new();
    if let Some(output) = git_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() > 3 {
                    let status = &line[..2];
                    let file = line[3..].trim();
                    if status.contains('M') || status.contains('A') || status.contains('D') {
                        if let Some(rel_path) = file.strip_prefix(&format!("{}/", SYMLINKS_DIR)) {
                            git_modified.insert(rel_path.to_string());
                        }
                    }
                }
            }
        }
    }

    let mut statuses: Vec<(String, String, SymlinkStatus, Option<String>)> = vec![];

    debug!("Checking tracked symlinks for modifications...");
    for (tracked_path, hash) in &index.symlinks {
        let absolute_path = home.join(tracked_path);

        let (status, current_hash) = if absolute_path.exists() {
            if absolute_path.is_symlink() {
                let hash_now = hash_symlink_target(&absolute_path, tracking_path)?;
                debug!("Checking tracked file: {} (indexed hash: {}, current hash: {})",
                       tracked_path, hash, hash_now);
                if hash_now != *hash {
                    debug!("Hash mismatch detected for: {}", tracked_path);
                    (SymlinkStatus::Modified { reason: ModificationReason::DifferentHash }, Some(hash_now))
                } else if git_modified.contains(tracked_path) {
                    debug!("Content modified detected by git for: {}", tracked_path);
                    (SymlinkStatus::Modified { reason: ModificationReason::ContentModified }, Some(hash_now))
                } else {
                    (SymlinkStatus::UpToDate, Some(hash_now))
                }
            } else {
                debug!("Tracked symlink is now a regular file: {}", tracked_path);
                (SymlinkStatus::Modified { reason: ModificationReason::RegularFile }, None)
            }
        } else {
            debug!("Tracked symlink no longer exists: {}", tracked_path);
            (SymlinkStatus::Deleted, None)
        };

        statuses.push((tracked_path.clone(), hash.clone(), status, current_hash));
    }

    let modified: Vec<_> = statuses.iter().filter_map(|(p, h, s, ch)| match s {
        SymlinkStatus::Modified { reason } => Some((p, h, reason, ch)),
        _ => None,
    }).collect();
    let deleted: Vec<_> = statuses.iter().filter_map(|(p, h, s, _)| match s {
        SymlinkStatus::Deleted => Some((p, h)),
        _ => None,
    }).collect();
    let up_to_date: Vec<_> = statuses.iter().filter_map(|(p, h, s, _)| match s {
        SymlinkStatus::UpToDate => Some((p, h)),
        _ => None,
    }).collect();

    debug!("Status summary - Modified: {}, Deleted: {}", modified.len(), deleted.len());

    let symlinks_dir = Path::new(sprout_path).join(SYMLINKS_DIR);

    if modified.is_empty() && deleted.is_empty() {
        if show_all && !up_to_date.is_empty() {
            for (file, hash) in &up_to_date {
                let target = symlinks_dir.join(file);
                println!("{} {} [{}] {}", "✓".green(), file.green(), &hash[..8].green(), 
                    format!("→ {}", target.display()).bright_black());
            }
            let symlinks_dir = Path::new(sprout_path).join(SYMLINKS_DIR);
            println!("\nYour symlinks are up to date with '{}'.", symlinks_dir.display());
        } else {
            let symlinks_dir = Path::new(sprout_path).join(SYMLINKS_DIR);
            println!("Your symlinks are up to date with '{}'.", symlinks_dir.display());
        }
        return Ok(());
    }

    if show_all && !up_to_date.is_empty() {
        for (file, hash) in &up_to_date {
            let target = symlinks_dir.join(file);
            println!("{} {} [{}] {}", "✓".green(), file.green(), &hash[..8].green(),
                format!("→ {}", target.display()).bright_black());
        }
    }
    if !modified.is_empty() {
        for (file, expected_hash, reason, current_hash) in &modified {
            let target = symlinks_dir.join(file);
            match reason {
                ModificationReason::DifferentHash => {
                    let current = current_hash.as_ref().map(|h| &h[..8]).unwrap_or("none");
                    println!("{} {} [expected: {} current: {}] {}", 
                        "M".red(), file.red(), &expected_hash[..8].green(), current.red(),
                        format!("→ {}", target.display()).bright_black());
                }
                ModificationReason::RegularFile => {
                    println!("{} {} [expected: {} current: {}] {}", 
                        "M".red(), file.red(), &expected_hash[..8].green(), "regular file".red(),
                        format!("→ {}", target.display()).bright_black());
                }
                ModificationReason::ContentModified => {
                    println!("{} {} [{}] {} {}", 
                        "M".yellow(), file.yellow(), &expected_hash[..8].green(),
                        format!("→ {}", target.display()).bright_black(),
                        "(content modified)".yellow());
                }
            }
        }
    }
    if !deleted.is_empty() {
        for (file, hash) in &deleted {
            let target = symlinks_dir.join(file);
            println!("{} {} [expected: {}] {}", "D".red(), file.red(), &hash[..8].green(),
                format!("→ {}", target.display()).bright_black());
        }
    }

    println!("\n{}:", "Legend".bold());
    if show_all {
        println!("  {} = Up-to-date (symlink is correct).", "✓".green());
    }
    println!("  {} = Modified (hash mismatch or regular file).", "M".red());
    println!("  {} = Modified (content changed in git).", "M".yellow());
    println!("  {} = Deleted (symlink missing).", "D".red());
    Ok(())
}

/// Undoes a symlink by copying the file back to its original location and removing it from tracking.
pub fn undo_symlink(sprout_path: &str, path: PathBuf, dry_run: bool, _tracking_path: &str) -> Result<()> {
    debug!("Starting undo_symlink for path: {}", path.display());
    debug!("Sprout path: {}", sprout_path);

    let mut index = SproutLock::load(sprout_path)?;
    let home = dirs::home_dir().context("Could not find home directory")?;

    debug!("Home directory: {}", home.display());
    debug!("Index contains {} tracked symlinks", index.symlinks.len());

    // Resolve the path to absolute, but don't follow symlinks
    let home_target = if path.is_absolute() {
        debug!("Path is absolute: {}", path.display());
        path
    } else {
        // Convert relative path to absolute without following symlinks
        let current_dir = env::current_dir().context("Could not get current directory")?;
        let mut absolute_path = current_dir.join(&path);

        // Manually normalize the path to remove . and .. components without following symlinks
        let mut components = Vec::new();
        for component in absolute_path.components() {
            match component {
                std::path::Component::CurDir => {}, // Skip "."
                std::path::Component::ParentDir => {
                    components.pop(); // Handle ".."
                },
                _ => components.push(component),
            }
        }
        absolute_path = components.iter().collect();

        debug!("Normalized absolute path: {}", absolute_path.display());
        absolute_path
    };

    debug!("Resolved home target: {}", home_target.display());

    // Convert to relative path for index lookup using normalize_path to handle /local prefix
    let home_target_str = home_target.to_string_lossy();
    let normalized_target = normalize_path(&home_target_str);
    debug!("Normalized target path: {}", normalized_target);

    let home_dir = env::var("HOME").context("HOME environment variable not set")?;
    let normalized_home = normalize_path(&home_dir);
    debug!("Normalized home directory: {}", normalized_home);

    let relative_home_path = normalized_target.strip_prefix(normalized_home).map(|s| s.trim_start_matches('/'))
        .context("Target path is not within HOME directory")?;

    debug!("Relative home path for index lookup: {}", relative_home_path);

    // Find the entry in the index
    debug!("Looking up entry in index...");
    let entry_hash = index.symlinks.get(relative_home_path)
        .context(format!("Path '{}' is not tracked by sprout", relative_home_path))?;

    debug!("Found index entry - hash: {}", entry_hash);

    // Construct the source path in sprout (assuming it's in symlinks directory)
    let sprout_source = Path::new(sprout_path).join(SYMLINKS_DIR).join(relative_home_path);
    debug!("Sprout source path: {}", sprout_source.display());

    if !sprout_source.exists() {
        debug!("Sprout source does not exist: {}", sprout_source.display());
        return Err(anyhow!("Source file {} no longer exists in sprout directory", sprout_source.display()));
    }

    if dry_run {
        println!("Would undo symlink: {}", home_target.display());
        println!("  Source in sprout: {}", sprout_source.display());
        if home_target.exists() || home_target.is_symlink() {
            println!("  Would remove existing: {}", home_target.display());
        }
        if sprout_source.is_dir() {
            println!("  Would move directory back to original location.");
        } else {
            println!("  Would move file back to original location.");
        }
        println!("  Would remove from tracking.");
        return Ok(());
    }

    debug!("Sprout source exists and is accessible");
    debug!("Sprout source is_file: {}, is_dir: {}", sprout_source.is_file(), sprout_source.is_dir());

    // Remove the existing symlink if it exists
    debug!("Checking home target status...");
    debug!("Home target exists: {}, is_symlink: {}, is_dir: {}",
           home_target.exists(), home_target.is_symlink(), home_target.is_dir());

    if home_target.exists() || home_target.is_symlink() {
        if home_target.is_dir() && !home_target.is_symlink() {
            debug!("Target is a directory but not a symlink - cannot undo");
            return Err(anyhow!("Target {} is a directory, not a symlink. Cannot undo.", home_target.display()));
        } else {
            info!("Removing symlink at {}", home_target.display());
            debug!("Attempting to remove file/symlink: {}", home_target.display());
            fs::remove_file(&home_target)
                .context(format!("Failed to remove symlink {}", home_target.display()))?;
            debug!("Successfully removed symlink");
        }
    } else {
        debug!("Home target does not exist, nothing to remove");
    }

    // Ensure parent directory exists
    if let Some(parent) = home_target.parent() {
        debug!("Ensuring parent directory exists: {}", parent.display());
        fs::create_dir_all(parent)
            .context(format!("Failed to create parent directory for {}", home_target.display()))?;
        debug!("Parent directory ready");
    } else {
        debug!("No parent directory needed");
    }

    // Move the file/directory back from sprout to home
    if sprout_source.is_dir() {
        info!("Moving directory {} back to {}", sprout_source.display(), home_target.display());
        debug!("Starting directory move operation");
        fs::rename(&sprout_source, &home_target)
            .context(format!("Failed to move directory {} to {}", sprout_source.display(), home_target.display()))?;
        debug!("Directory move completed successfully");
    } else if sprout_source.is_file() {
        info!("Moving file {} back to {}", sprout_source.display(), home_target.display());
        debug!("Starting file move operation");
        fs::rename(&sprout_source, &home_target)
            .context(format!("Failed to move file {} to {}", sprout_source.display(), home_target.display()))?;
        debug!("File move completed successfully");
    } else {
        debug!("Source is neither file nor directory: {}", sprout_source.display());
        return Err(anyhow!("Source {} is neither a file nor directory", sprout_source.display()));
    }

    // Remove from index
    debug!("Removing entry from index: {}", relative_home_path);
    let removed_entry = index.symlinks.remove(relative_home_path);
    debug!("Index removal result: {:?}", removed_entry.is_some());

    debug!("Writing updated index to disk");
    index.save(sprout_path)?;
    debug!("Index successfully written");

    info!("Successfully undid symlink: {} is no longer tracked and has been restored", relative_home_path);
    debug!("undo_symlink completed successfully");
    Ok(())
}

pub fn rehash_symlinks(sprout_path: &str, tracking_path: &str, discover: bool, dry_run: bool) -> Result<()> {
    let mut index = SproutLock::load(sprout_path)?;
    let home = dirs::home_dir().context("Could not find home directory")?;

    if discover {
        info!("Discovering managed symlinks (dry_run: {})...", dry_run);
        let symlinks_dir = Path::new(sprout_path).join("symlinks");
        
        if !symlinks_dir.exists() {
            info!("No symlinks directory found.");
            return Ok(());
        }

        let mut discovered_count = 0;
        discover_symlinks_recursive(&symlinks_dir, &symlinks_dir, &home, tracking_path, &mut index, &mut discovered_count, dry_run)?;
        
        if !dry_run {
            index.save(sprout_path)?;
            info!("Discovery complete: {} symlinks added to lockfile", discovered_count);
        } else {
            info!("Would add {} symlinks to lockfile", discovered_count);
        }
        return Ok(());
    }

    if index.symlinks.is_empty() {
        info!("No symlinks found in index. Nothing to rehash.");
        return Ok(());
    }

    let mut updated_count = 0;
    let mut error_count = 0;

    info!("Rehashing {} tracked symlinks (dry_run: {})...", index.symlinks.len(), dry_run);

    let symlink_paths: Vec<String> = index.symlinks.keys().cloned().collect();

    for relative_path in symlink_paths {
        let absolute_path = home.join(&relative_path);

        if absolute_path.exists() && absolute_path.is_symlink() {
            match hash_symlink_target(&absolute_path, tracking_path) {
                Ok(new_hash) => {
                    let old_hash = index.symlinks.get(&relative_path).cloned();
                    if old_hash.as_ref() != Some(&new_hash) {
                        info!("Updated hash for {}: {:?} -> {}", relative_path, old_hash, new_hash);
                        if !dry_run {
                            index.symlinks.insert(relative_path, new_hash);
                        }
                        updated_count += 1;
                    } else {
                        debug!("Hash unchanged for {}", relative_path);
                    }
                }
                Err(e) => {
                    warn!("Failed to rehash {}: {}", relative_path, e);
                    error_count += 1;
                }
            }
        } else {
            warn!("Symlink {} no longer exists or is not a symlink", relative_path);
            error_count += 1;
        }
    }

    if !dry_run {
        index.save(sprout_path)?;
        info!("Rehashing complete: {} updated, {} errors", updated_count, error_count);
    } else {
        info!("Would update {} symlinks, {} errors", updated_count, error_count);
    }
    Ok(())
}

fn discover_symlinks_recursive(
    symlinks_root: &Path,
    current_sprout_dir: &Path,
    home: &Path,
    tracking_path: &str,
    index: &mut SproutLock,
    discovered_count: &mut usize,
    dry_run: bool,
) -> Result<()> {
    debug!("Scanning directory: {}", current_sprout_dir.display());
    
    for entry in fs::read_dir(current_sprout_dir)? {
        let entry = entry?;
        let sprout_path = entry.path();
        let relative_path = sprout_path.strip_prefix(symlinks_root)
            .context("Failed to get relative path")?;
        let home_path = home.join(relative_path);

        if sprout_path.is_dir() {
            // Check if home path is a symlink to this directory
            if home_path.is_symlink() {
                let target = fs::read_link(&home_path)?;
                debug!("Found symlink: {} -> {}", home_path.display(), target.display());
                if target == sprout_path {
                    // It's a directory symlink
                    let relative_str = relative_path.to_string_lossy().to_string();
                    if !index.symlinks.contains_key(&relative_str) {
                        match hash_symlink_target(&home_path, tracking_path) {
                            Ok(hash) => {
                                if !dry_run {
                                    index.symlinks.insert(relative_str.clone(), hash);
                                }
                                info!("Discovered directory symlink: {}", relative_str);
                                *discovered_count += 1;
                            }
                            Err(e) => warn!("Failed to hash {}: {}", relative_str, e),
                        }
                    } else {
                        debug!("Already tracked: {}", relative_str);
                    }
                }
            } else if home_path.is_dir() {
                // Real directory, descend into it
                debug!("Descending into directory: {}", relative_path.display());
                discover_symlinks_recursive(symlinks_root, &sprout_path, home, tracking_path, index, discovered_count, dry_run)?;
            } else {
                debug!("Home path doesn't exist or is not a directory: {}", home_path.display());
            }
        } else if sprout_path.is_file() {
            // Check if home path is a symlink to this file
            if home_path.is_symlink() {
                let target = fs::read_link(&home_path)?;
                debug!("Found symlink: {} -> {}", home_path.display(), target.display());
                if target == sprout_path {
                    let relative_str = relative_path.to_string_lossy().to_string();
                    if !index.symlinks.contains_key(&relative_str) {
                        match hash_symlink_target(&home_path, tracking_path) {
                            Ok(hash) => {
                                if !dry_run {
                                    index.symlinks.insert(relative_str.clone(), hash);
                                }
                                info!("Discovered file symlink: {}", relative_str);
                                *discovered_count += 1;
                            }
                            Err(e) => warn!("Failed to hash {}: {}", relative_str, e),
                        }
                    } else {
                        debug!("Already tracked: {}", relative_str);
                    }
                }
            } else {
                debug!("Home path is not a symlink: {}", home_path.display());
            }
        }
    }
    Ok(())
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
