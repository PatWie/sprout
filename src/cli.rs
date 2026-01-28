use clap::{Parser, Subcommand, ValueEnum};
use std::path::{PathBuf, Path};
use std::collections::{HashMap, HashSet};
use anyhow::{Result, Context};
use tracing::{info, warn};

use crate::core::*;
use crate::manifest::{load_manifest, save_manifest};
use crate::lockfile::{SproutLock, PackageState};
use crate::ast::PrettyPrint;

const DEFAULT_SPROUT_PATH: &str = "/sprout";

#[derive(Debug, Clone, ValueEnum)]
pub enum FetchMethod {
    /// Automatically detect the fetch method based on URL
    Auto,
    /// Git repository
    Git,
    /// HTTP download (tar, zip, etc.)
    Http,
    /// Local path
    Local,
}

#[derive(Parser, Debug)]
#[command(
    name = "sprout",
    about = "                          __
  ___ ___  _______  __ __/ /_
 (_-</ _ \\/ __/ _ \\/ // / __/
/___/ .__/_/  \\___/\\_,_/\\__/
   /_/ Package and environment manager
",
    version,
    disable_version_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Verbose mode (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Quiet mode (suppresses all output except errors)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Path to sprout directory (overrides SPROUT_PATH env var)
    #[arg(long, global = true)]
    pub sprout_path: Option<PathBuf>,

    /// Path to track files from (overrides HOME env var for symlink operations)
    #[arg(long, global = true)]
    pub tracking_path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize a new sprout directory with required structure
    ///
    /// Creates symlinks/, sources/, cache/, dist/ directories and initializes
    /// a git repository with .gitignore and manifest.sprout with example modules
    #[command(visible_alias = "i")]
    Init {
        /// Directory to initialize (defaults to /sprout)
        #[arg(default_value = "/sprout")]
        path: PathBuf,

        /// Create empty manifest instead of using template with examples
        #[arg(long)]
        empty: bool,
    },

    /// Manage dependencies (fetch, build, install modules)
    ///
    /// Modules are defined in manifest.sprout and can be fetched from git
    /// repositories or HTTP archives, then built with custom scripts
    #[command(visible_alias = "m")]
    Modules {
        #[command(subcommand)]
        command: ModulesCommand,
    },

    /// Manage symlinks for dotfiles and configuration
    ///
    /// Track files/directories by moving them to /sprout/symlinks and
    /// creating symlinks back to their original locations
    #[command(visible_alias = "s")]
    Symlinks {
        #[command(subcommand)]
        command: SymlinksCommand,
    },

    /// Manage environment sets and generate shell exports
    ///
    /// Group modules into named environments and generate export statements
    /// for PATH, LD_LIBRARY_PATH, etc.
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },

    /// Show complete status (modules, symlinks, and git)
    Status {
        /// Show all symlinks including up-to-date ones
        #[arg(long)]
        all: bool,
        /// Expand module dependency tree
        #[arg(long)]
        expand: bool,
    },

    /// Commit changes in sprout directory
    ///
    /// Commits all changes (manifest, lockfile, symlinks) to git.
    /// Opens editor for commit message if not provided
    Commit {
        /// Commit message (opens editor if not provided)
        #[arg(short, long)]
        message: Option<String>,
        /// Generate commit message using AI
        #[arg(long)]
        ai: bool,
    },

    /// Pull changes from remote git repository
    Pull {
        /// Remote name (default: origin)
        #[arg(short, long)]
        remote: Option<String>,
        /// Branch name (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,
    },

    /// Push changes to remote git repository
    Push {
        /// Remote name (default: origin)
        #[arg(short, long)]
        remote: Option<String>,
        /// Branch name (default: current branch)
        #[arg(short, long)]
        branch: Option<String>,
    },

    /// Edit manifest.sprout with $EDITOR
    ///
    /// Opens manifest in your editor and validates syntax after saving
    #[command(visible_alias = "e")]
    Edit {
        /// Sprout directory path (defaults to /sprout)
        #[arg(default_value = "/sprout")]
        path: PathBuf,
    },

    /// Verify and reformat manifest.sprout
    ///
    /// Reformats manifest, computes missing SHA256 hashes for HTTP archives,
    /// and updates lockfile. Use -i to write changes in-place
    Format {
        /// Sprout directory path (defaults to /sprout)
        #[arg(default_value = "/sprout")]
        path: PathBuf,
        /// Write changes in-place, otherwise print to stdout
        #[arg(short)]
        i: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModulesCommand {
    /// Fetch dependencies from git or HTTP sources
    ///
    /// Downloads source code to sources/ and caches HTTP archives.
    /// Automatically computes and adds SHA256 hashes for HTTP archives
    #[command(visible_alias = "f")]
    Fetch {
        /// Fetch all dependencies in manifest
        #[arg(long)]
        all: bool,
        /// Specific packages to fetch (e.g., 'ripgrep cmake')
        packages: Vec<String>,
        /// Show what would be fetched without fetching
        #[arg(long)]
        dry_run: bool,
    },

    /// Build dependencies using their build scripts
    ///
    /// Executes build commands from manifest.sprout and installs to dist/.
    /// Checks dependencies are built first and skips if already up-to-date
    #[command(visible_alias = "b")]
    Build {
        /// Build all dependencies in manifest
        #[arg(long)]
        all: bool,
        /// Specific packages to build (e.g., 'ripgrep cmake')
        packages: Vec<String>,
        /// Force rebuild even if up-to-date
        #[arg(long)]
        rebuild: bool,
        /// Show what would be built without building
        #[arg(long)]
        dry_run: bool,
    },

    /// Install dependencies (fetch + build in one step)
    ///
    /// Convenience command that fetches and builds one or more modules
    #[command(visible_alias = "i")]
    Install {
        /// Install all dependencies in manifest
        #[arg(long)]
        all: bool,
        /// Specific packages to install (e.g., 'ripgrep cmake gcc')
        packages: Vec<String>,
        /// Also install dependencies of specified packages
        #[arg(long)]
        with_deps: bool,
        /// Force rebuild even if up-to-date
        #[arg(long)]
        rebuild: bool,
        /// Show what would be done without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Show module status with build information
    ///
    /// Displays modules with their fetch/build status and dependencies.
    /// Use --expand to show full dependency tree, --all to show up-to-date modules
    #[command(visible_alias = "s")]
    Status {
        /// Expand tree to show all dependencies recursively
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        expand: bool,
        /// Show all modules including up-to-date ones
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        all: bool,
    },

    /// Compute and display/update module hashes
    ///
    /// Calculates hashes for fetch and build configurations.
    /// Use -i to update lockfile with computed hashes
    Hash {
        /// Write hashes to lockfile instead of stdout
        #[arg(short)]
        i: bool,
        /// Compute fetch hashes (default if neither specified)
        #[arg(long)]
        fetch: bool,
        /// Compute build hashes (default if neither specified)
        #[arg(long)]
        build: bool,
    },

    /// Remove unused cache/source directories
    ///
    /// Cleans up old source and cache directories that don't match
    /// current manifest hashes. Frees disk space from old versions
    #[command(visible_alias = "c")]
    Clean {
        /// Show what would be removed without removing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum SymlinksCommand {
    /// Add file or directory to symlink management
    ///
    /// Moves the file/directory to /sprout/symlinks and creates a symlink
    /// back to the original location. Tracks it in the lockfile
    #[command(visible_alias = "a")]
    Add {
        /// Path to file or directory to add (e.g., ~/.bashrc)
        path: PathBuf,
        /// Add directory recursively (required for directories)
        #[arg(short, long)]
        recursive: bool,
        /// Show what would be done without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Show symlink status (modified, deleted, up-to-date)
    ///
    /// Shows which tracked symlinks have changed, been deleted, or are
    /// pointing to the wrong target
    #[command(visible_alias = "s")]
    Status {
        /// Show all files including up-to-date ones
        #[arg(long)]
        all: bool,
    },

    /// Restore broken or missing symlinks
    ///
    /// Recreates symlinks based on lockfile. Use after fresh clone or
    /// when symlinks are broken/deleted
    Restore {
        /// Show what would be restored without restoring
        #[arg(long)]
        dry_run: bool,
    },

    /// Rehash symlinks or discover managed symlinks
    ///
    /// Without --discover: Updates hashes for tracked symlinks
    /// With --discover: Finds symlinks pointing to /sprout/symlinks and
    /// adds them to lockfile (useful after migration or lost lockfile)
    Rehash {
        /// Discover and add managed symlinks not in lockfile
        #[arg(long)]
        discover: bool,
        /// Show what would be done without doing it
        #[arg(long)]
        dry_run: bool,
    },

    /// Undo symlink management for a path
    ///
    /// Moves file/directory back from /sprout/symlinks to original location,
    /// removes symlink, and stops tracking it
    Undo {
        /// Path to undo (e.g., ~/.bashrc)
        path: PathBuf,
        /// Show what would be done without doing it
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnvCommand {
    /// Interactively edit environment (toggle modules)
    ///
    /// Opens interactive menu to select which built modules should be
    /// included in the environment. Modules define exports for PATH,
    /// LD_LIBRARY_PATH, etc.
    Edit {
        /// Environment name (default: "default")
        #[arg(default_value = "default")]
        environment: String,
    },

    /// List environments and their modules
    ///
    /// Shows all defined environments or details of a specific one
    List {
        /// Specific environment to show (shows all if not specified)
        environment: Option<String>,
    },

    /// Generate environment export statements
    ///
    /// Outputs shell export statements for PATH, LD_LIBRARY_PATH, etc.
    /// Use: eval "$(sprout env generate)" to load environment
    Generate {
        /// Environment name (default: "default")
        environment: Option<String>,
        /// Generate for all built dependencies (ignores environment sets)
        #[arg(long)]
        all: bool,
    },
}

pub async fn run_cli(cli: Cli) -> Result<()> {
    let sprout_path = cli.sprout_path
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| std::env::var("SPROUT_PATH").ok())
        .unwrap_or_else(|| DEFAULT_SPROUT_PATH.to_string());

    let verbose = cli.verbose > 0;

    match cli.command {
        Commands::Init { path, empty } => {
            // Use global sprout-path if provided, otherwise use the init-specific path
            let init_path = if sprout_path != "/sprout" {
                &sprout_path
            } else {
                path.to_str().unwrap()
            };
            init_sprout(init_path, empty)?;
        }
        Commands::Modules { command } => {
            handle_modules_command(&sprout_path, command, verbose)?;
        }
        Commands::Symlinks { command } => {
            let tracking_path = cli.tracking_path
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
                .context("Could not determine tracking path (HOME directory)")?;
            handle_symlinks_command(&sprout_path, command, &tracking_path)?;
        }
        Commands::Env { command } => {
            handle_env_command(&sprout_path, command)?;
        }
        Commands::Status { all, expand } => {
            use colored::Colorize;

            let tracking_path = cli.tracking_path
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
                .context("Could not determine tracking path (HOME directory)")?;

            println!("{}", "=== Modules ===".bold());
            show_status_tree(&sprout_path, expand, all)?;

            println!("\n{}", "=== Symlinks ===".bold());
            check_symlinks(&sprout_path, all, &tracking_path)?;

            println!("\n{}", "=== Git Status ===".bold());
            crate::core::git_status(&sprout_path)?;
        }
        Commands::Commit { message, ai } => {
            if ai {
                crate::core::git_commit_ai(&sprout_path).await?;
            } else if let Some(msg) = message {
                crate::core::git_commit(&sprout_path, &msg)?;
            } else {
                crate::core::git_commit_interactive(&sprout_path)?;
            }
        }
        Commands::Pull { remote, branch } => {
            crate::core::git_pull(&sprout_path, remote, branch)?;
        }
        Commands::Push { remote, branch } => {
            crate::core::git_push(&sprout_path, remote, branch)?;
        }
        Commands::Edit { path } => {
            let edit_path = path.to_string_lossy();
            edit_manifest(&edit_path)?;
        }
        Commands::Format { path, i } => {
            let format_path = path.to_string_lossy();
            format_manifest(&format_path, i)?;
        }
    }

    Ok(())
}

fn handle_modules_command(sprout_path: &str, command: ModulesCommand, verbose: bool) -> Result<()> {
    match command {

        ModulesCommand::Fetch { all, packages, dry_run } => {
            let manifest = load_manifest(sprout_path)?;

            if all {
                info!("Fetching all dependencies");
                for package in &manifest.modules {
                    if let Err(e) = fetch_package(sprout_path, package, dry_run) {
                        info!("Skipping {}: {}", package.id(), e);
                    }
                }
            } else if !packages.is_empty() {
                for module_id in packages {
                    let package = manifest.modules.iter()
                        .find(|p| p.id() == module_id || p.name == module_id)
                        .ok_or_else(|| anyhow::anyhow!("Package not found: {}", module_id))?;

                    fetch_package(sprout_path, package, dry_run)?;
                }
            } else {
                return Err(anyhow::anyhow!("Specify --all or one or more package names"));
            }
        }
        ModulesCommand::Build { all, packages, rebuild, dry_run } => {
            let manifest = load_manifest(sprout_path)?;

            if all {
                info!("Building all dependencies");
                let ordered_modules = resolve_dependency_order(&manifest)?;

                for package in ordered_modules {
                    if let Err(e) = build_package(sprout_path, package, dry_run, rebuild, verbose) {
                        warn!("Failed to build {}: {}", package.id(), e);
                    }
                }
            } else if !packages.is_empty() {
                for module_id in packages {
                    let package = manifest.modules.iter()
                        .find(|p| p.id() == module_id || p.name == module_id)
                        .ok_or_else(|| anyhow::anyhow!("Package not found: {}", module_id))?;

                    build_package(sprout_path, package, dry_run, rebuild, verbose)?;
                }
            } else {
                return Err(anyhow::anyhow!("Specify --all or one or more package names"));
            }
        }
        ModulesCommand::Install { all, packages, with_deps, rebuild, dry_run } => {
            let manifest = load_manifest(sprout_path)?;

            if all {
                info!("Installing all dependencies");
                let ordered_modules = resolve_dependency_order(&manifest)?;

                for package in ordered_modules {
                    if package.fetch.is_some() && let Err(e) = fetch_package(sprout_path, package, dry_run) {
                        warn!("Failed to fetch {}: {}", package.id(), e);
                        continue;
                    }
                    if let Err(e) = build_package(sprout_path, package, dry_run, rebuild, verbose) {
                        warn!("Failed to build {}: {}", package.id(), e);
                    }
                }
            } else if !packages.is_empty() {
                if with_deps {
                    // Collect all packages and their dependencies
                    let mut all_packages = std::collections::HashSet::new();
                    for module_id in &packages {
                        let deps = manifest.get_all_dependencies(module_id);
                        for dep in deps {
                            all_packages.insert(dep);
                        }
                    }

                    // Resolve dependency order for all collected packages
                    let ordered_modules = resolve_dependency_order(&manifest)?;
                    let packages_to_install: Vec<_> = ordered_modules.into_iter()
                        .filter(|p| all_packages.contains(&p.id()))
                        .collect();

                    for package in packages_to_install {
                        if package.fetch.is_some() {
                            if let Err(e) = fetch_package(sprout_path, package, dry_run) {
                                warn!("Failed to fetch {}: {}", package.id(), e);
                                continue;
                            }
                        }
                        if let Err(e) = build_package(sprout_path, package, dry_run, rebuild, verbose) {
                            warn!("Failed to build {}: {}", package.id(), e);
                        }
                    }
                } else {
                    // Install only specified packages without dependencies
                    for module_id in packages {
                        let package = manifest.modules.iter()
                            .find(|p| p.id() == module_id || p.name == module_id)
                            .ok_or_else(|| anyhow::anyhow!("Package not found: {}", module_id))?;

                        if package.fetch.is_some() {
                            fetch_package(sprout_path, package, dry_run)?;
                        }
                        build_package(sprout_path, package, dry_run, rebuild, verbose)?;
                    }
                }
            } else {
                return Err(anyhow::anyhow!("Specify --all or one or more package names"));
            }
        }
        ModulesCommand::Status { expand, all } => {
            show_status_tree(sprout_path, expand, all)?;
        }
        ModulesCommand::Hash { i, fetch, build } => {
            use crate::core::deps::{compute_fetch_hash, compute_build_hash};

            let manifest = load_manifest(sprout_path)?;
            let mut lock = SproutLock::load(sprout_path)?;

            let compute_fetch = fetch || !build;
            let compute_build = build || !fetch;

            for module in &manifest.modules {
                let module_id = module.id();

                if compute_fetch
                    && let Some(hash) = compute_fetch_hash(module) {
                        if i {
                            let mut state = lock.get_module_state(&module_id).cloned()
                                .unwrap_or(PackageState { fetch_hash: None, build_hash: None });
                            state.fetch_hash = Some(hash);
                            lock.set_module_state(module_id.clone(), state);
                        } else {
                            println!("{} fetch_hash: {}", module_id, hash);
                        }
                    }

                if compute_build
                    && let Some(hash) = compute_build_hash(module) {
                        if i {
                            let mut state = lock.get_module_state(&module_id).cloned()
                                .unwrap_or(PackageState { fetch_hash: None, build_hash: None });
                            state.build_hash = Some(hash);
                            lock.set_module_state(module_id.clone(), state);
                        } else {
                            println!("{} build_hash: {}", module_id, hash);
                        }
                    }
            }

            if i {
                lock.save(sprout_path)?;
                println!("Updated lockfile.");
            }
        }
        ModulesCommand::Clean { dry_run } => {
            clean_unused_directories(sprout_path, dry_run)?;
        }
    }

    Ok(())
}

fn handle_symlinks_command(sprout_path: &str, command: SymlinksCommand, tracking_path: &str) -> Result<()> {
    match command {
        SymlinksCommand::Add { path, recursive, dry_run } => {
            info!("Adding symlink: {} (recursive: {}, dry_run: {})", path.display(), recursive, dry_run);
            add_file(sprout_path, path, recursive, dry_run, tracking_path)?;
        }
        SymlinksCommand::Status { all } => {
            info!("Checking symlinks (show_all: {})", all);
            check_symlinks(sprout_path, all, tracking_path)?;
        }
        SymlinksCommand::Restore { dry_run } => {
            info!("Restoring symlinks (dry_run: {})", dry_run);
            restore_symlinks(sprout_path, dry_run, tracking_path)?;
        }
        SymlinksCommand::Rehash { discover, dry_run } => {
            info!("Rehashing symlinks (discover: {}, dry_run: {})", discover, dry_run);
            rehash_symlinks(sprout_path, tracking_path, discover, dry_run)?;
        }
        SymlinksCommand::Undo { path, dry_run } => {
            info!("Undoing symlink: {} (dry_run: {})", path.display(), dry_run);
            undo_symlink(sprout_path, path, dry_run, tracking_path)?;
        }
    }

    Ok(())
}

fn handle_env_command(sprout_path: &str, command: EnvCommand) -> Result<()> {
    match command {
        EnvCommand::Edit { environment } => {
            env_edit_interactive(sprout_path, &environment)?;
        }
        EnvCommand::List { environment } => {
            let manifest = load_manifest(sprout_path)?;

            if let Some(environments) = &manifest.environments {
                if let Some(env_name) = environment {
                    if let Some(modules) = environments.environments.get(&env_name) {
                        println!("Environment '{}':", env_name);
                        for package in modules {
                            println!("  {}", package);
                        }
                    } else {
                        println!("Environment '{}' not found.", env_name);
                    }
                } else {
                    println!("Environments:");
                    for (name, modules) in &environments.environments {
                        println!("  {}:", name);
                        for package in modules {
                            println!("    {}", package);
                        }
                    }
                }
            } else {
                println!("No environments defined.");
            }
        }
        EnvCommand::Generate { environment, all } => {
            let manifest = load_manifest(sprout_path)?;
            let env_name = environment.as_deref().unwrap_or("default");

            if all {
                // Generate environment for all built modules
                info!("Generating environment for all built modules");
                // TODO: Implement environment generation for all modules
                warn!("env generate --all not yet implemented");
            } else if let Some(environments) = &manifest.environments {
                if let Some(modules) = environments.environments.get(env_name) {
                    println!("# Environment: {}", env_name);
                    
                    // Guard to prevent loading environment multiple times in nested shells.
                    // Without this, each time the shell config is sourced (e.g., exec zsh),
                    // the exports would append to existing values, causing duplicates and
                    // trailing colons that break tools like glibc's configure script.
                    // This is especially problematic for variables that didn't exist before
                    // (like custom vars), where repeated sourcing creates: "value:value:value"
                    println!("# Guard to prevent loading multiple times");
                    println!("if [ -n \"$SPROUT_ENV_LOADED\" ]; then");
                    println!("  return 0 2>/dev/null || :");
                    println!("fi");
                    println!("export SPROUT_ENV_LOADED=1");
                    println!();

                    // Collect all exports by variable name
                    use std::collections::HashMap;
                    let mut exports: HashMap<String, Vec<String>> = HashMap::new();

                    for module_id in modules {
                        if let Some(package) = manifest.modules.iter().find(|p| p.id() == *module_id) {
                            let dist_path = std::path::Path::new(sprout_path).join("dist").join(package.id());

                            for (var, path) in &package.exports {
                                let full_path = dist_path.join(path.trim_start_matches('/'));
                                exports.entry(var.clone())
                                    .or_insert_with(Vec::new)
                                    .push(full_path.display().to_string());
                            }
                        }
                    }

                    // Generate consolidated export statements
                    let mut sorted_vars: Vec<_> = exports.keys().collect();
                    sorted_vars.sort();

                    for var in sorted_vars {
                        let paths = &exports[var];
                        let joined_paths = paths.join(":");
                        println!("export {}=\"{}${{{}:+:${{{}}}}}\"", var, joined_paths, var, var);
                    }
                } else {
                    return Err(anyhow::anyhow!("Environment '{}' not found", env_name));
                }
            } else {
                return Err(anyhow::anyhow!("No environments defined"));
            }
        }
    }

    Ok(())
}

fn edit_manifest(sprout_path: &str) -> Result<()> {
    use std::process::Command;

    let manifest_path = Path::new(sprout_path).join("manifest.sprout");

    if !manifest_path.exists() {
        return Err(anyhow::anyhow!("Manifest not found: {}", manifest_path.display()));
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor)
        .arg(&manifest_path)
        .status()
        .with_context(|| format!("Failed to launch editor: {}", editor))?;

    if !status.success() {
        return Err(anyhow::anyhow!("Editor exited with error"));
    }

    // Validate the manifest after editing
    match load_manifest(sprout_path) {
        Ok(_) => {
            println!("Manifest edited successfully.");
            Ok(())
        }
        Err(e) => {
            eprintln!("Warning: Manifest has syntax errors: {}", e);
            eprintln!("Please fix the errors and try again.");
            Err(e)
        }
    }
}

fn format_manifest(sprout_path: &str, in_place: bool) -> Result<()> {
    let mut manifest = load_manifest(sprout_path)?;
    let mut updated_modules = Vec::new();

    // Compute and add missing SHA256 hashes for HTTP archives
    for module in &mut manifest.modules {
        if let Some(fetch) = &module.fetch {
            if let crate::ast::FetchSpec::Http(http_spec) = &fetch.spec {
                if http_spec.sha256.is_none() {
                    // Compute old hash before adding SHA256
                    let old_fetch_hash = crate::core::deps::compute_fetch_hash(module)
                        .map(|h| h[..8].to_string())
                        .unwrap_or_else(|| "no-fetch".to_string());

                    let module_id = module.id();
                    let old_dir_name = format!("{}-{}", module_id, old_fetch_hash);
                    let cache_dir = std::path::Path::new(sprout_path).join("cache/http").join(&old_dir_name);
                    let original_filename = http_spec.url.split('/').next_back().unwrap_or("archive");
                    let cache_path = cache_dir.join(original_filename);

                    if cache_path.exists() {
                        use sha2::{Sha256, Digest};
                        let mut file = std::fs::File::open(&cache_path)?;
                        let mut hasher = Sha256::new();
                        std::io::copy(&mut file, &mut hasher)?;
                        let hash = format!("{:x}", hasher.finalize());

                        println!("Adding SHA256 for {}: {}.", module_id, hash);

                        // Update the module
                        if let Some(fetch_mut) = &mut module.fetch {
                            if let crate::ast::FetchSpec::Http(http_spec_mut) = &mut fetch_mut.spec {
                                http_spec_mut.sha256 = Some(hash);

                                // Compute new hash after adding SHA256
                                let new_fetch_hash = crate::core::deps::compute_fetch_hash(module)
                                    .map(|h| h[..8].to_string())
                                    .unwrap_or_else(|| "no-fetch".to_string());
                                let new_dir_name = format!("{}-{}", module_id, new_fetch_hash);

                                // Rename directories only in in-place mode
                                if in_place {
                                    // Rename cache directory
                                    let old_cache_dir = std::path::Path::new(sprout_path).join("cache/http").join(&old_dir_name);
                                    let new_cache_dir = std::path::Path::new(sprout_path).join("cache/http").join(&new_dir_name);
                                    if old_cache_dir.exists() && !new_cache_dir.exists() {
                                        std::fs::rename(&old_cache_dir, &new_cache_dir)?;
                                        println!("  Renamed cache: {} -> {}.", old_dir_name, new_dir_name);
                                    }

                                    // Rename source directory
                                    let old_source_dir = std::path::Path::new(sprout_path).join("sources/http").join(&old_dir_name);
                                    let new_source_dir = std::path::Path::new(sprout_path).join("sources/http").join(&new_dir_name);
                                    if old_source_dir.exists() && !new_source_dir.exists() {
                                        std::fs::rename(&old_source_dir, &new_source_dir)?;
                                        println!("  Renamed source: {} -> {}.", old_dir_name, new_dir_name);
                                    }
                                }

                                updated_modules.push(module_id);
                            }
                        }
                    }
                }
            }
        }
    }

    let updated = !updated_modules.is_empty();

    if in_place {
        save_manifest(sprout_path, &manifest)?;

        // Update lockfile with new fetch hashes for updated modules
        if updated {
            let mut lock = SproutLock::load(sprout_path)?;
            for module_id in &updated_modules {
                if let Some(module) = manifest.modules.iter().find(|m| m.id() == *module_id) {
                    let new_fetch_hash = crate::core::deps::compute_fetch_hash(module);
                    let mut state = lock.get_module_state(module_id)
                        .cloned()
                        .unwrap_or(PackageState {
                            fetch_hash: None,
                            build_hash: None,
                        });
                    state.fetch_hash = new_fetch_hash;
                    lock.set_module_state(module_id.clone(), state);
                }
            }
            lock.save(sprout_path)?;
            println!("Formatted manifest.sprout and added missing SHA256 hashes.");
        } else {
            println!("Formatted manifest.sprout.");
        }
    } else {
        // Print to stdout
        print!("{}", manifest.pretty_print());
    }
    Ok(())
}



fn show_status_tree(sprout_path: &str, expand: bool, show_all: bool) -> Result<()> {
    use colored::Colorize;

    let manifest = load_manifest(sprout_path)?;
    let lock = SproutLock::load(sprout_path)?;

    let mut module_map: HashMap<String, &crate::ast::ModuleBlock> = HashMap::new();
    for module in &manifest.modules {
        module_map.insert(module.id(), module);
    }

    let mut roots: Vec<String> = manifest.modules.iter().map(|m| m.id()).collect();
    roots.sort();

    let mut processed = HashSet::new();
    let mut has_issues = false;

    for root in &roots {
        let node_has_issues = print_tree_node(root, &module_map, &lock, sprout_path, "", true, &mut processed, expand, show_all)?;
        has_issues = has_issues || node_has_issues;
    }

    if !has_issues && !show_all {
        let manifest_path = Path::new(sprout_path).join("manifest.sprout");
        println!("Your modules are up to date with '{}'.", manifest_path.display());
        return Ok(());
    }

    println!("\n{}:", "Legend".bold());
    println!("  Name: green=up-to-date, red=needs rebuild.");
    println!("  Hashes: green=done, red=not done.");
    println!("  S=Source, C=Cache (- = not applicable).");

    Ok(())
}

fn print_tree_node(
    id: &str,
    module_map: &HashMap<String, &crate::ast::ModuleBlock>,
    lock: &SproutLock,
    sprout_path: &str,
    prefix: &str,
    is_last: bool,
    processed: &mut HashSet<String>,
    expand: bool,
    show_all: bool,
) -> Result<bool> {
    use colored::Colorize;

    let module = match module_map.get(id) {
        Some(m) => m,
        None => {
            println!("{}{}─ {} ({})", prefix, if is_last { "└" } else { "├" }, id, "not found".red());
            return Ok(true);
        }
    };

    // Calculate status flags
    let source_path = get_source_path(sprout_path, module);
    let dist_path = get_dist_path(sprout_path, module);

    let has_source = source_path.exists();
    let has_dist = dist_path.exists();

    let has_cache = if let Some(fetch) = &module.fetch {
        match &fetch.spec {
            crate::ast::FetchSpec::Http(_) => {
                let fetch_hash = compute_fetch_hash(module)
                    .map(|h| h[..8].to_string())
                    .unwrap_or_else(|| "no-fetch".to_string());
                let cache_dir = Path::new(sprout_path)
                    .join("cache/http")
                    .join(format!("{}-{}", id, fetch_hash));
                Some(cache_dir.exists())
            }
            _ => None
        }
    } else {
        None
    };

    let fetched = module.fetch.is_none() || has_source;
    let built = has_dist;
    let mut up_to_date = false;

    if has_dist && let Some(state) = lock.get_module_state(id) {
        let current_fetch_hash = compute_fetch_hash(module);
        let current_build_hash = compute_build_hash(module);

        let fetch_changed = current_fetch_hash != state.fetch_hash;
        let build_changed = current_build_hash != state.build_hash;

        up_to_date = !fetch_changed && !build_changed && state.build_hash.is_some();
    }

    let check = |b: bool| if b { "✓".green() } else { "✗".red() };
    let check_opt = |opt: Option<bool>| match opt {
        Some(true) => "✓".green(),
        Some(false) => "✗".red(),
        None => "-".bright_black(),
    };

    let fetch_hash_str = compute_fetch_hash(module)
        .map(|h| h[..8].to_string())
        .unwrap_or_else(|| "-".to_string());

    let build_hash_str = compute_build_hash(module)
        .map(|h| h[..8].to_string())
        .unwrap_or_else(|| "-".to_string());

    let fetch_hash_colored = if fetched {
        fetch_hash_str.green()
    } else {
        fetch_hash_str.red()
    };

    let build_hash_colored = if built {
        build_hash_str.green()
    } else {
        build_hash_str.red()
    };

    let status_line = format!("{}/{} S:{} C:{}",
        fetch_hash_colored, build_hash_colored,
        check(has_source), check_opt(has_cache));

    let colored_id = if up_to_date { id.green() } else { id.red() };

    let has_issues = !up_to_date;

    if show_all || has_issues {
        println!("{}{}─ {} [{}]", prefix, if is_last { "└" } else { "├" }, colored_id, status_line);
    }

    // Print dependencies (not dependents)
    let mut child_has_issues = false;
    if expand && !module.depends_on.is_empty() {
        let mut sorted_deps = module.depends_on.clone();
        sorted_deps.sort();

        let child_prefix = format!("{}{}  ", prefix, if is_last { " " } else { "│" });

        for (i, dep_id) in sorted_deps.iter().enumerate() {
            let is_last_child = i == sorted_deps.len() - 1;
            let dep_has_issues = print_tree_node(dep_id, module_map, lock, sprout_path, &child_prefix, is_last_child, processed, expand, show_all)?;
            child_has_issues = child_has_issues || dep_has_issues;
        }
    }

    Ok(has_issues || child_has_issues)
}


fn env_edit_interactive(sprout_path: &str, env_name: &str) -> Result<()> {
    use dialoguer::MultiSelect;

    let manifest = load_manifest(sprout_path)?;
    let lock = SproutLock::load(sprout_path)?;

    // Get all built modules
    let mut available_modules: Vec<String> = manifest.modules.iter()
        .filter(|m| {
            let module_id = m.id();
            lock.get_module_state(&module_id)
                .map(|s| s.build_hash.is_some())
                .unwrap_or(false)
        })
        .map(|m| m.id())
        .collect();

    if available_modules.is_empty() {
        return Err(anyhow::anyhow!("No built modules available"));
    }

    available_modules.sort();

    // Get current environment modules
    let current_modules: HashSet<String> = manifest.environments
        .as_ref()
        .and_then(|envs| envs.environments.get(env_name))
        .map(|v| v.iter().cloned().collect())
        .unwrap_or_default();

    // Create defaults (true if currently in environment)
    let defaults: Vec<bool> = available_modules.iter()
        .map(|m| current_modules.contains(m))
        .collect();

    // Interactive selection
    let selections = MultiSelect::new()
        .with_prompt(format!("Select modules for environment '{}' (space to toggle, enter to confirm)", env_name))
        .items(&available_modules)
        .defaults(&defaults)
        .interact()?;

    // Update manifest
    let mut manifest = load_manifest(sprout_path)?;
    let new_modules: Vec<String> = selections.iter()
        .map(|&i| available_modules[i].clone())
        .collect();

    if manifest.environments.is_none() {
        manifest.environments = Some(crate::ast::EnvironmentsBlock {
            environments: HashMap::new(),
        });
    }

    manifest.environments.as_mut().unwrap()
        .environments.insert(env_name.to_string(), new_modules);

    save_manifest(sprout_path, &manifest)?;
    println!("✓ Updated environment '{}'.", env_name);

    Ok(())
}

fn clean_unused_directories(sprout_path: &str, dry_run: bool) -> Result<()> {
    use crate::core::deps::compute_fetch_hash;
    use std::fs;

    let manifest = load_manifest(sprout_path)?;

    // Collect valid hashes from manifest
    let mut valid_hashes = HashSet::new();
    for module in &manifest.modules {
        if let Some(hash) = compute_fetch_hash(module) {
            let short_hash = &hash[..8];
            let dir_name = format!("{}-{}", module.id(), short_hash);
            valid_hashes.insert(dir_name);
        }
    }

    let mut removed_count = 0;
    let mut freed_bytes = 0u64;

    // Clean all directories
    let dirs_to_clean = [
        ("sources/git", Path::new(sprout_path).join("sources/git")),
        ("sources/http", Path::new(sprout_path).join("sources/http")),
        ("cache/http", Path::new(sprout_path).join("cache/http")),
    ];

    for (label, dir_path) in &dirs_to_clean {
        if dir_path.exists() {
            for entry in fs::read_dir(dir_path)? {
                let entry = entry?;
                let dir_name = entry.file_name().to_string_lossy().to_string();

                if !valid_hashes.contains(&dir_name) {
                    let size = dir_size(&entry.path())?;
                    freed_bytes += size;

                    if dry_run {
                        println!("Would remove: {}/{} ({} MB)", label, dir_name, size / 1_000_000);
                    } else {
                        println!("Removing: {}/{} ({} MB)", label, dir_name, size / 1_000_000);
                        fs::remove_dir_all(entry.path())?;
                    }
                    removed_count += 1;
                }
            }
        }
    }

    if removed_count == 0 {
        println!("No unused directories found.");
    } else if dry_run {
        println!("\nWould remove {} directory(ies), freeing {} MB.", removed_count, freed_bytes / 1_000_000);
    } else {
        println!("\nRemoved {} directory(ies), freed {} MB.", removed_count, freed_bytes / 1_000_000);
    }

    Ok(())
}

fn dir_size(path: &Path) -> Result<u64> {
    let mut size = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                size += dir_size(&entry.path())?;
            } else {
                size += metadata.len();
            }
        }
    } else {
        size = path.metadata()?.len();
    }
    Ok(size)
}
