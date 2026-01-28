use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{debug, info};
use sha2::{Sha256, Digest};

use crate::ast::{ModuleBlock, SproutManifest};
use crate::lockfile::SproutLock;
use crate::manifest::load_manifest;

use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Compute hash of package definition for change detection
/// Compute hash of fetch block only
pub fn compute_fetch_hash(package: &ModuleBlock) -> Option<String> {
    package.fetch.as_ref().map(|fetch| {
        let mut hasher = DefaultHasher::new();
        fetch.spec.hash(&mut hasher);
        let hash_value = hasher.finish();
        
        let mut sha_hasher = Sha256::new();
        sha_hasher.update(hash_value.to_le_bytes());
        format!("{:x}", sha_hasher.finalize())
    })
}

/// Compute hash of build block only
pub fn compute_build_hash(package: &ModuleBlock) -> Option<String> {
    package.build.as_ref().map(|build| {
        let mut hasher = DefaultHasher::new();
        build.hash(&mut hasher);
        let hash_value = hasher.finish();
        
        let mut sha_hasher = Sha256::new();
        sha_hasher.update(hash_value.to_le_bytes());
        format!("{:x}", sha_hasher.finalize())
    })
}

/// Resolve dependency order using topological sort
pub fn resolve_dependency_order(manifest: &SproutManifest) -> Result<Vec<&ModuleBlock>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut modules: HashMap<String, &ModuleBlock> = HashMap::new();

    // Build the graph and initialize in-degrees
    for package in &manifest.modules {
        let module_id = package.id();
        modules.insert(module_id.clone(), package);
        graph.insert(module_id.clone(), Vec::new());
        in_degree.insert(module_id, 0);
    }

    // Add edges for dependencies
    for package in &manifest.modules {
        let module_id = package.id();
        for dep in &package.depends_on {
            // Find the dependency by name or full ID
            let dep_id = manifest.modules
                .iter()
                .find(|p| p.name == *dep || p.id() == *dep)
                .map(|p| p.id())
                .ok_or_else(|| anyhow!("Dependency not found: {}", dep))?;

            graph.get_mut(&dep_id).unwrap().push(module_id.clone());
            *in_degree.get_mut(&module_id).unwrap() += 1;
        }
    }

    // Topological sort using Kahn's algorithm
    let mut queue: Vec<String> = in_degree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| id.clone())
        .collect();

    let mut result = Vec::new();

    while let Some(current) = queue.pop() {
        result.push(modules[&current]);

        for neighbor in &graph[&current] {
            let degree = in_degree.get_mut(neighbor).unwrap();
            *degree -= 1;
            if *degree == 0 {
                queue.push(neighbor.clone());
            }
        }
    }

    if result.len() != manifest.modules.len() {
        return Err(anyhow!("Circular dependency detected"));
    }

    Ok(result)
}

/// Check which modules need to be rebuilt
pub fn fetch_package(sprout_path: &str, package: &ModuleBlock, dry_run: bool) -> Result<()> {
    let Some(fetch) = &package.fetch else {
        return Err(anyhow!(
            "Package {} has no fetch configuration",
            package.id()
        ));
    };

    if dry_run {
        println!("Would fetch: {}", package.id());
        return Ok(());
    }

    info!("Fetching package: {}", package.id());

    match &fetch.spec {
        crate::ast::FetchSpec::Git(git_spec) => {
            fetch_git(sprout_path, package, git_spec)?;
        }
        crate::ast::FetchSpec::Http(archive_spec) => {
            fetch_archive(sprout_path, package, archive_spec)?;
        }
        _ => {
            return Err(anyhow!("Unsupported fetch type for package {}", package.id()));
        }
    }

    // Reload package from manifest in case it was updated (e.g., SHA256 added)
    let manifest = load_manifest(sprout_path)?;
    let updated_package = manifest.modules.iter()
        .find(|m| m.id() == package.id())
        .ok_or_else(|| anyhow!("Package {} not found after fetch", package.id()))?;

    // Update lockfile with current fetch hash
    let mut lock = SproutLock::load(sprout_path)?;
    let fetch_hash = compute_fetch_hash(updated_package);
    let mut state = lock.get_module_state(&package.id())
        .cloned()
        .unwrap_or(crate::lockfile::PackageState {
            fetch_hash: None,
            build_hash: None,
        });
    state.fetch_hash = fetch_hash;
    lock.set_module_state(package.id(), state);
    lock.save(sprout_path)?;

    info!("Successfully fetched: {}", package.id());
    Ok(())
}

/// Build a package
pub fn build_package(
    sprout_path: &str,
    package: &ModuleBlock,
    dry_run: bool,
    rebuild: bool,
    verbose: bool,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Duration;

    let module_id = package.id();

    if dry_run {
        println!("Would build: {}", module_id);
        return Ok(());
    }

    let pb = if !verbose && atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("  {spinner} {msg}")?);
        pb.set_message(format!("Building {}", module_id));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        info!("Building package: {}", module_id);
        None
    };

    debug!("Package build block: {:?}", package.build);

    let source_path = get_source_path(sprout_path, package);
    let dist_path = get_dist_path(sprout_path, package);
    let lock = SproutLock::load(sprout_path)?;

    // Check all dependencies are built
    if !package.depends_on.is_empty() {
        let manifest = load_manifest(sprout_path)?;
        let all_deps = manifest.get_all_dependencies(&module_id);

        // Skip the last one (it's the package itself)
        for dep in all_deps.iter().take(all_deps.len().saturating_sub(1)) {
            let dep_pkg = manifest.modules.iter().find(|p| p.id() == *dep);

            if let Some(dep_pkg) = dep_pkg {
                let dep_dist = get_dist_path(sprout_path, dep_pkg);

                if !dep_dist.exists() {
                    return Err(anyhow!(
                        "Dependency '{}' is not built yet. Build it first.",
                        dep
                    ));
                }

                // Check if dependency is up to date
                if let Some(dep_state) = lock.get_module_state(dep) {
                    let current_hash = compute_build_hash(dep_pkg);
                    if current_hash != dep_state.build_hash {
                        return Err(anyhow!(
                            "Dependency '{}' has changed and needs rebuilding. Rebuild it first.",
                            dep
                        ));
                    }
                }
            }
        }
    }

    // Check if package is already up-to-date
    if !rebuild && dist_path.exists()
        && let Some(state) = lock.get_module_state(&module_id) {
            let current_hash = compute_build_hash(package);
            if current_hash == state.build_hash {
                info!("Package {} is already up-to-date, skipping build", module_id);
                return Ok(());
            }
        }

    // Only check source path if package has fetch configuration
    if package.fetch.is_some() && !source_path.exists() {
        return Err(anyhow!(
            "Source directory does not exist for {}: {}",
            module_id,
            source_path.display()
        ));
    }

    // Create source path if it doesn't exist (for modules without fetch)
    if !source_path.exists() {
        fs::create_dir_all(&source_path)?;
    }

    // Clean dist directory only if rebuild flag is set
    if rebuild && dist_path.exists() {
        info!("Cleaning existing dist directory");
        fs::remove_dir_all(&dist_path)?;
    }

    // Create dist directory
    fs::create_dir_all(&dist_path)?;

    // Execute build commands if any
    if let Some(build) = &package.build {
        debug!("Build env block: {:?}", build.env);

        // Build single shell script with all commands
        let mut script = String::from("set -e\n");

        // Export base env variables
        let sprout_dist = Path::new(sprout_path).join("dist");
        script.push_str(&format!("export SPROUT_DIST='{}'\n", sprout_dist.display()));
        script.push_str(&format!("export DIST_PATH='{}'\n", dist_path.display()));
        script.push_str(&format!("export SOURCE_PATH='{}'\n", source_path.display()));

        // Export env block variables in order (bash will expand them with double quotes)
        for (key, value) in &build.env {
            script.push_str(&format!("export {}=\"{}\"\n", key, value));
        }

        for cmd in &build.commands {
            script.push_str(cmd);
            script.push('\n');
        }

        info!("Executing build script");
        debug!("Generated script:\n{}", script);
        let work_dir = if source_path.exists() {
            &source_path
        } else {
            Path::new(sprout_path)
        };

        // Create logs directory
        let logs_dir = Path::new(sprout_path).join("logs");
        fs::create_dir_all(&logs_dir)?;

        // Generate timestamped log filename
        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let log_filename = format!("{}-build-{}.log", module_id, timestamp);
        let log_path = logs_dir.join(&log_filename);

        info!("Build log: {}", log_path.display());

        // Execute with output captured to both console and log file
        let mut child = Command::new("bash")
            .arg("-c")
            .arg(&script)
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // Create log file
        let mut log_file = fs::File::create(&log_path)?;

        // Write script to log file first
        writeln!(log_file, "=== Build Script ===")?;
        writeln!(log_file, "{}", script)?;
        writeln!(log_file, "=== Build Output ===")?;

        // Stream output to both console and log file
        let stdout_handle = std::thread::spawn({
            let log_path = log_path.clone();
            move || -> Result<()> {
                let mut log_file = fs::OpenOptions::new().append(true).open(&log_path)?;
                let mut reader = std::io::BufReader::new(stdout);
                let mut buffer = [0; 1024];

                loop {
                    match reader.read(&mut buffer)? {
                        0 => break,
                        n => {
                            let output = &buffer[..n];
                            // Write to console only if verbose
                            if verbose {
                                std::io::stdout().write_all(output)?;
                                std::io::stdout().flush()?;
                            }
                            // Always write to log file
                            log_file.write_all(output)?;
                            log_file.flush()?;
                        }
                    }
                }
                Ok(())
            }
        });

        let stderr_handle = std::thread::spawn({
            let log_path = log_path.clone();
            move || -> Result<()> {
                let mut log_file = fs::OpenOptions::new().append(true).open(&log_path)?;
                let mut reader = std::io::BufReader::new(stderr);
                let mut buffer = [0; 1024];

                loop {
                    match reader.read(&mut buffer)? {
                        0 => break,
                        n => {
                            let output = &buffer[..n];
                            // Write to console stderr only if verbose
                            if verbose {
                                std::io::stderr().write_all(output)?;
                                std::io::stderr().flush()?;
                            }
                            // Always write to log file
                            log_file.write_all(output)?;
                            log_file.flush()?;
                        }
                    }
                }
                Ok(())
            }
        });

        // Wait for process and threads to complete
        let status = child.wait()?;
        stdout_handle.join().map_err(|_| anyhow!("stdout thread panicked"))??;
        stderr_handle.join().map_err(|_| anyhow!("stderr thread panicked"))??;

        if let Some(pb) = &pb {
            pb.finish_and_clear();
        }

        if !status.success() {
            return Err(anyhow!(
                "Build failed for {} with exit code: {:?}\nLog saved to: {}",
                module_id,
                status.code(),
                log_path.display()
            ));
        }

        info!("Build completed successfully. Log saved to: {}", log_path.display());
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
        println!("  ✓ Built {}", module_id);
    }

    // Update lockfile
    let mut lock = lock;
    let build_hash = compute_build_hash(package);
    let mut state = lock.get_module_state(&module_id)
        .cloned()
        .unwrap_or(crate::lockfile::PackageState {
            fetch_hash: None,
            build_hash: None,
        });
    state.build_hash = build_hash;
    lock.set_module_state(module_id.clone(), state);
    lock.save(sprout_path)?;

    info!("Successfully built: {}", module_id);
    Ok(())
}

pub fn get_source_path(sprout_path: &str, package: &ModuleBlock) -> PathBuf {
    let subdir = if let Some(fetch) = &package.fetch {
        match &fetch.spec {
            crate::ast::FetchSpec::Git(_) => "git",
            crate::ast::FetchSpec::Http(_) => "http",
            _ => "archive",
        }
    } else {
        "archive"
    };
    
    let fetch_hash = compute_fetch_hash(package)
        .map(|h| h[..8].to_string())
        .unwrap_or_else(|| "no-fetch".to_string());
    
    let dir_name = format!("{}-{}", package.id(), fetch_hash);
    Path::new(sprout_path).join("sources").join(subdir).join(dir_name)
}

pub fn get_dist_path(sprout_path: &str, package: &ModuleBlock) -> PathBuf {
    Path::new(sprout_path).join("dist").join(package.id())
}

fn fetch_git(sprout_path: &str, package: &ModuleBlock, git: &crate::ast::GitSpec) -> Result<()> {
    use std::process::Command;
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Duration;

    let source_path = get_source_path(sprout_path, package);

    // Clean existing source directory
    if source_path.exists() {
        info!("Cleaning existing source directory: {}", source_path.display());
        fs::remove_dir_all(&source_path)?;
    }
    fs::create_dir_all(&source_path)?;

    // Create logs directory and log file for git operations
    let logs_dir = Path::new(sprout_path).join("logs");
    fs::create_dir_all(&logs_dir)?;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let log_filename = format!("{}-fetch-{}.log", package.id(), timestamp);
    let log_path = logs_dir.join(&log_filename);

    let pb = if atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("  {spinner} {msg}")?);
        pb.set_message(format!("Cloning {}", package.id()));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        info!("Cloning git repository: {} -> {}", git.url, source_path.display());
        None
    };

    info!("Fetch log: {}", log_path.display());

    // Log git clone command
    let mut log_file = fs::File::create(&log_path)?;
    writeln!(log_file, "=== Git Fetch Log ===")?;
    writeln!(log_file, "Repository: {}", git.url)?;
    writeln!(log_file, "Target: {}", source_path.display())?;
    if let Some(ref_) = &git.ref_ {
        writeln!(log_file, "Ref: {}", ref_)?;
    }
    writeln!(log_file, "=== Git Clone Output ===")?;
    drop(log_file);

    // Execute git clone with depth 1 and optional recursive
    let mut cmd = Command::new("git");
    cmd.arg("clone")
       .arg("--depth")
       .arg("1");

    if git.recursive {
        cmd.arg("--recursive");
    }

    if let Some(ref_) = &git.ref_ {
        cmd.arg("--branch").arg(ref_);
    }

    cmd.arg(&git.url).arg(&source_path);

    let status = cmd
        .stdout(fs::OpenOptions::new().append(true).open(&log_path)?)
        .stderr(fs::OpenOptions::new().append(true).open(&log_path)?)
        .status()?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
        if status.success() {
            println!("  ✓ Cloned {}", package.id());
        }
    }

    if !status.success() {
        return Err(anyhow!(
            "git clone failed with exit code: {:?}\nLog saved to: {}",
            status.code(),
            log_path.display()
        ));
    }

    info!("Git fetch completed successfully. Log saved to: {}", log_path.display());
    Ok(())
}

fn fetch_archive(sprout_path: &str, package: &ModuleBlock, archive: &crate::ast::HttpSpec) -> Result<()> {
    let fetch_hash = compute_fetch_hash(package)
        .map(|h| h[..8].to_string())
        .unwrap_or_else(|| "no-fetch".to_string());
    
    let cache_dir_name = format!("{}-{}", package.id(), fetch_hash);
    let cache_dir = Path::new(sprout_path).join("cache/http").join(&cache_dir_name);
    std::fs::create_dir_all(&cache_dir)?;

    let original_filename = archive.url.split('/').next_back().unwrap_or("archive");
    let cache_path = cache_dir.join(original_filename);

    if !cache_path.exists() {
        download_file(&archive.url, &cache_path, original_filename)?;
    } else {
        info!("Using cached {}", original_filename);
    }

    // Compute SHA256 if not present in manifest
    let computed_hash = if archive.sha256.is_none() {
        info!("Computing SHA256 for {}", original_filename);
        let hash = compute_file_sha256(&cache_path)?;
        info!("SHA256: {}", hash);
        Some(hash)
    } else {
        None
    };

    if let Some(expected_hash) = &archive.sha256 {
        verify_sha256(&cache_path, expected_hash, original_filename)?;
    }

    // Update manifest with computed SHA256
    if let Some(hash) = computed_hash {
        let package_id = package.id();
        let mut manifest = load_manifest(sprout_path)?;
        if let Some(module) = manifest.modules.iter_mut().find(|m| m.id() == package_id) {
            if let Some(fetch) = &mut module.fetch {
                if let crate::ast::FetchSpec::Http(http_spec) = &mut fetch.spec {
                    http_spec.sha256 = Some(hash);
                    info!("Updated manifest with SHA256 for {}", package_id);
                    crate::manifest::save_manifest(sprout_path, &manifest)?;
                }
            }
        }
    }

    let source_path = get_source_path(sprout_path, package);
    if source_path.exists() {
        info!("Cleaning existing source directory: {}", source_path.display());
        fs::remove_dir_all(&source_path)?;
    }
    fs::create_dir_all(&source_path)?;

    // Use custom output filename if specified
    let output_filename = package.fetch.as_ref()
        .and_then(|f| f.output.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(original_filename);

    // If output is specified, skip extraction (just copy the file)
    let skip_extract = package.fetch.as_ref()
        .and_then(|f| f.output.as_ref())
        .is_some();

    if skip_extract {
        info!("Copying {} -> {}", original_filename, source_path.display());
        copy_file_with_progress(&cache_path, &source_path, original_filename, output_filename)?;
    } else {
        info!("Extracting {} -> {}", original_filename, source_path.display());
        extract_archive_with_output(&cache_path, &source_path, original_filename, output_filename)?;
    }
    Ok(())
}

fn download_file(url: &str, dest: &Path, filename: &str) -> Result<()> {
    use std::io::Write;
    use indicatif::{ProgressBar, ProgressStyle};

    let mut response = reqwest::blocking::get(url)?;
    let total_size = response.content_length().unwrap_or(0);

    let pb = if atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new(total_size);
        pb.set_style(ProgressStyle::default_bar()
            .template("  {msg} [{bar:40}] {bytes}/{total_bytes} ({eta})")?
            .progress_chars("=>-"));
        pb.set_message(format!("Downloading {}", filename));
        Some(pb)
    } else {
        info!("Downloading {}", filename);
        None
    };

    let mut file = std::fs::File::create(dest)?;
    let mut downloaded = 0u64;
    let mut buffer = [0; 8192];

    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 { break; }
        file.write_all(&buffer[..n])?;
        downloaded += n as u64;
        if let Some(ref pb) = pb {
            pb.set_position(downloaded);
        }
    }

    if let Some(pb) = pb {
        pb.finish_with_message(format!("✓ Downloaded {}", filename));
    } else {
        info!("Downloaded {}", filename);
    }
    Ok(())
}

fn verify_sha256(path: &Path, expected: &str, filename: &str) -> Result<()> {
    let computed = compute_file_sha256(path)?;

    if computed != expected {
        return Err(anyhow!(
            "SHA256 mismatch for {}: expected {}, got {}",
            filename, expected, computed
        ));
    }
    Ok(())
}

fn compute_file_sha256(path: &Path) -> Result<String> {
    use sha2::{Sha256, Digest};

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_file_with_progress(cache_path: &Path, dest_dir: &Path, filename: &str, output_name: &str) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Duration;

    let pb = if atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("  {spinner} {msg}")?);
        pb.set_message(format!("Copying {}", filename));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        info!("Copying {}", filename);
        None
    };

    let output_path = dest_dir.join(output_name);
    std::fs::copy(cache_path, &output_path)?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
        println!("  ✓ Copied {}", filename);
    } else {
        info!("Copied {}", filename);
    }
    Ok(())
}

fn extract_archive_with_output(cache_path: &Path, dest: &Path, filename: &str, output_name: &str) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::time::Duration;

    let is_archive = filename.ends_with(".tar.gz") || filename.ends_with(".tgz") 
        || filename.ends_with(".tar.xz") || filename.ends_with(".tar.lz") || filename.ends_with(".zip")
        || filename.ends_with(".gz") || filename.ends_with(".xz");

    let action = if is_archive { "Extracting" } else { "Copying" };
    let action_past = if is_archive { "Extracted" } else { "Copied" };

    let pb = if atty::is(atty::Stream::Stderr) {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner()
            .template("  {spinner} {msg}")?);
        pb.set_message(format!("{} {}", action, filename));
        pb.enable_steady_tick(Duration::from_millis(100));
        Some(pb)
    } else {
        info!("{} {}", action, filename);
        None
    };

    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        let tar_gz = std::fs::File::open(cache_path)?;
        let tar = flate2::read::GzDecoder::new(tar_gz);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(dest)?;
    } else if filename.ends_with(".tar.xz") {
        let tar_xz = std::fs::File::open(cache_path)?;
        let tar = xz::read::XzDecoder::new(tar_xz);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(dest)?;
    } else if filename.ends_with(".tar.lz") {
        let tar_lz = std::fs::File::open(cache_path)?;
        let mut decompressed = Vec::new();
        lzma_rs::lzma_decompress(&mut std::io::BufReader::new(tar_lz), &mut decompressed)?;
        let mut archive = tar::Archive::new(std::io::Cursor::new(decompressed));
        archive.unpack(dest)?;
    } else if filename.ends_with(".zip") {
        let file = std::fs::File::open(cache_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        archive.extract(dest)?;
    } else if filename.ends_with(".gz") {
        let gz_file = std::fs::File::open(cache_path)?;
        let mut decoder = flate2::read::GzDecoder::new(gz_file);
        let output_path = dest.join(output_name);
        let mut output_file = std::fs::File::create(output_path)?;
        std::io::copy(&mut decoder, &mut output_file)?;
    } else if filename.ends_with(".xz") {
        let xz_file = std::fs::File::open(cache_path)?;
        let mut decoder = xz::read::XzDecoder::new(xz_file);
        let output_path = dest.join(output_name);
        let mut output_file = std::fs::File::create(output_path)?;
        std::io::copy(&mut decoder, &mut output_file)?;
    } else {
        // Raw file - just copy it with the specified output name
        let output_path = dest.join(output_name);
        std::fs::copy(cache_path, &output_path)?;
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
        println!("  ✓ {} {}", action_past, filename);
    } else {
        info!("{} {}", action_past, filename);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use std::collections::HashMap;

    #[test]
    fn test_fetch_hash_consistency() {
        // Create identical fetch blocks
        let fetch1 = FetchBlock {
            spec: FetchSpec::Git(GitSpec {
                url: "https://github.com/test/repo.git".to_string(),
                ref_: Some("v1.0".to_string()),
                recursive: false,
            }),
            output: None,
        };

        let fetch2 = FetchBlock {
            spec: FetchSpec::Git(GitSpec {
                url: "https://github.com/test/repo.git".to_string(),
                ref_: Some("v1.0".to_string()),
                recursive: false,
            }),
            output: None,
        };

        let module1 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: Some(fetch1),
            build: None,
            update: None,
        };

        let module2 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: Some(fetch2),
            build: None,
            update: None,
        };

        let hash1 = compute_fetch_hash(&module1);
        let hash2 = compute_fetch_hash(&module2);

        assert_eq!(hash1, hash2, "Identical fetch blocks should produce identical hashes");
    }

    #[test]
    fn test_build_hash_consistency_with_env_vars() {
        // Create build blocks with same env vars in same order
        let build1 = ScriptBlock {
            env: vec![
                ("CC".to_string(), "gcc".to_string()),
                ("CFLAGS".to_string(), "-O2".to_string()),
            ],
            commands: vec!["make".to_string()],
        };

        let build2 = ScriptBlock {
            env: vec![
                ("CC".to_string(), "gcc".to_string()),
                ("CFLAGS".to_string(), "-O2".to_string()),
            ],
            commands: vec!["make".to_string()],
        };

        let module1 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: None,
            build: Some(build1),
            update: None,
        };

        let module2 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: None,
            build: Some(build2),
            update: None,
        };

        let hash1 = compute_build_hash(&module1);
        let hash2 = compute_build_hash(&module2);

        assert_eq!(hash1, hash2, "Identical build blocks should produce identical hashes");
    }

    #[test]
    fn test_build_hash_different_for_different_content() {
        let build1 = ScriptBlock {
            env: vec![("CC".to_string(), "gcc".to_string())],
            commands: vec!["make".to_string()],
        };

        let build2 = ScriptBlock {
            env: vec![("CC".to_string(), "clang".to_string())],
            commands: vec!["make".to_string()],
        };

        let module1 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: None,
            build: Some(build1),
            update: None,
        };

        let module2 = ModuleBlock {
            name: "test".to_string(),
            depends_on: vec![],
            exports: vec![],
            fetch: None,
            build: Some(build2),
            update: None,
        };

        let hash1 = compute_build_hash(&module1);
        let hash2 = compute_build_hash(&module2);

        assert_ne!(hash1, hash2, "Different build blocks should produce different hashes");
    }

    #[test]
    fn test_serialize_script_for_hash_deterministic() {
        let script = ScriptBlock {
            env: vec![
                ("Z_VAR".to_string(), "last".to_string()),
                ("A_VAR".to_string(), "first".to_string()),
                ("M_VAR".to_string(), "middle".to_string()),
            ],
            commands: vec!["cmd1".to_string(), "cmd2".to_string()],
        };

        let serialized = script.to_string();
        assert_eq!(serialized, "ScriptBlock{env:[Z_VAR=last,A_VAR=first,M_VAR=middle],commands:[cmd1,cmd2]}");
    }
}

