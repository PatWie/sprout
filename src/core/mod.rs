pub mod deps;
pub mod symlinks;

// Re-export commonly used functions
pub use deps::*;
pub use symlinks::*;

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tracing::info;
use aws_config::Region;
use aws_sdk_bedrockruntime::{
    Client,
    config::BehaviorVersion,
    types::{ContentBlock, ConversationRole, Message},
};

const AI_MODEL_ID: &str = "global.anthropic.claude-haiku-4-5-20251001-v1:0";
const AI_AWS_PROFILE: &str = "my-aws-bedrock";
const AI_AWS_REGION: &str = "us-east-1";

/// Generate commit message using AWS Bedrock
async fn generate_commit_message<P: AsRef<Path>>(sprout_path: P) -> Result<String> {
    let sprout_path = sprout_path.as_ref();
    
    // Get git diff
    let diff_output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["diff", "--cached"])
        .output()
        .context("Failed to get git diff")?;
    
    let diff = String::from_utf8_lossy(&diff_output.stdout);
    
    if diff.trim().is_empty() {
        return Err(anyhow::anyhow!("No staged changes to commit"));
    }
    
    // Set up AWS Bedrock client
    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(AI_AWS_REGION.to_owned()))
        .profile_name(AI_AWS_PROFILE.to_owned())
        .load()
        .await;
    let client = Client::new(&sdk_config);
    
    // Create prompt
    let prompt = format!(
        "Generate a concise git commit message for the following changes. \
        Return ONLY the commit message, no explanations or quotes.\n\n{}",
        diff
    );
    
    let user_message = Message::builder()
        .role(ConversationRole::User)
        .content(ContentBlock::Text(prompt))
        .build()?;
    
    // Call Bedrock
    let response = client
        .converse()
        .model_id(AI_MODEL_ID)
        .messages(user_message)
        .send()
        .await?;
    
    // Extract message
    let message = response
        .output
        .and_then(|o| o.as_message().ok().cloned())
        .context("No response from model")?;
    
    let text = message
        .content()
        .iter()
        .find_map(|c| match c {
            ContentBlock::Text(t) => Some(t.clone()),
            _ => None,
        })
        .context("No text in response")?;
    
    Ok(text.trim().to_string())
}

/// Create a git commit with the given message
pub fn git_commit<P: AsRef<Path>>(sprout_path: P, message: &str) -> Result<()> {
    let sprout_path = sprout_path.as_ref();

    // Check if git repo exists
    if !sprout_path.join(".git").exists() {
        return Ok(()); // Skip if not a git repo
    }

    let output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["add", "."])
        .output()
        .context("Failed to execute git add")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["commit", "-m", message])
        .output()
        .context("Failed to execute git commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("nothing to commit") {
            info!("No changes to commit");
            return Ok(());
        }
        return Err(anyhow::anyhow!("git commit failed: {}", stderr));
    }

    info!("Created git commit: {}", message);
    Ok(())
}

/// Create a git commit with AI-generated message
pub async fn git_commit_ai<P: AsRef<Path>>(sprout_path: P) -> Result<()> {
    let sprout_path = sprout_path.as_ref();
    
    // Check if git repo exists
    if !sprout_path.join(".git").exists() {
        return Err(anyhow::anyhow!("Not a git repository"));
    }
    
    // Stage all changes first
    let output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["add", "."])
        .output()
        .context("Failed to execute git add")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    // Generate commit message
    info!("Generating commit message with AI...");
    let message = generate_commit_message(sprout_path).await?;
    info!("Generated message: {}", message);
    
    // Commit with generated message (without staging again)
    let output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["commit", "-m", &message])
        .output()
        .context("Failed to execute git commit")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("nothing to commit") {
            info!("No changes to commit");
            return Ok(());
        }
        return Err(anyhow::anyhow!("git commit failed: {}", stderr));
    }

    info!("Created git commit: {}", message);
    Ok(())
}

/// Initialize a new sprout directory
pub fn init_sprout<P: AsRef<Path>>(path: P, empty: bool) -> Result<()> {
    let sprout_path = path.as_ref();

    info!("Initializing sprout directory at: {}", sprout_path.display());

    // Create directory structure
    fs::create_dir_all(sprout_path)?;
    fs::create_dir_all(sprout_path.join("symlinks"))?;
    fs::create_dir_all(sprout_path.join("sources/git"))?;
    fs::create_dir_all(sprout_path.join("sources/http"))?;
    fs::create_dir_all(sprout_path.join("cache/archives"))?;
    fs::create_dir_all(sprout_path.join("dist"))?;

    // Create manifest.sprout
    let manifest_path = sprout_path.join("manifest.sprout");
    if !manifest_path.exists() {
        if empty {
            fs::write(&manifest_path, "")?;
        } else {
            let default_manifest = include_str!("../templates/default_manifest.sprout");
            fs::write(&manifest_path, default_manifest)?;
        }
    }

    // Create empty sprout.lock
    let lock_path = sprout_path.join("sprout.lock");
    if !lock_path.exists() {
        fs::write(&lock_path, "# Auto-generated by Sprout — do not edit\n\n[modules]\n\n[symlinks]\n")?;
    }

    // Create .gitignore
    let gitignore_path = sprout_path.join(".gitignore");
    if !gitignore_path.exists() {
        let gitignore_content = r#"# Sprout build artifacts
dist/
cache/
sources/
logs/

# Keep symlinks and manifest
!symlinks/
!manifest.sprout
!sprout.lock
"#;
        fs::write(&gitignore_path, gitignore_content)?;
    }

    // Initialize git repository
    if !sprout_path.join(".git").exists() {
        info!("Initializing git repository");
        let output = std::process::Command::new("git")
            .current_dir(sprout_path)
            .args(["init"])
            .output()
            .context("Failed to initialize git repository")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "git init failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Create initial commit
        git_commit(sprout_path, "Initialize sprout directory")?;
    }

    info!("Sprout directory initialized successfully");
    Ok(())
}

pub fn git_status<P: AsRef<Path>>(sprout_path: P) -> Result<()> {
    let sprout_path = sprout_path.as_ref();
    if !sprout_path.join(".git").exists() {
        return Err(anyhow::anyhow!("Not a git repository"));
    }
    std::process::Command::new("git")
        .current_dir(sprout_path)
        .arg("status")
        .status()
        .context("Failed to execute git status")?;
    Ok(())
}

pub fn git_commit_interactive<P: AsRef<Path>>(sprout_path: P) -> Result<()> {
    let sprout_path = sprout_path.as_ref();
    if !sprout_path.join(".git").exists() {
        return Err(anyhow::anyhow!("Not a git repository"));
    }
    std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["add", "."])
        .status()?;
    let status_output = std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["status", "--porcelain"])
        .output()?;
    let has_changes = String::from_utf8_lossy(&status_output.stdout)
        .lines()
        .any(|line| line.len() >= 2 && line.chars().next().unwrap_or(' ') != ' ' && line.chars().next().unwrap_or(' ') != '?');
    if !has_changes {
        println!("No changes to commit.");
        return Ok(());
    }
    std::process::Command::new("git")
        .current_dir(sprout_path)
        .arg("commit")
        .status()?;
    Ok(())
}

pub fn git_push<P: AsRef<Path>>(sprout_path: P, remote: Option<String>, branch: Option<String>) -> Result<()> {
    let sprout_path = sprout_path.as_ref();
    if !sprout_path.join(".git").exists() {
        return Err(anyhow::anyhow!("Not a git repository"));
    }
    let target_remote = remote.unwrap_or_else(|| "origin".to_string());
    let target_branch = if let Some(b) = branch {
        b
    } else {
        let output = std::process::Command::new("git")
            .current_dir(sprout_path)
            .args(["branch", "--show-current"])
            .output()?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    std::process::Command::new("git")
        .current_dir(sprout_path)
        .args(["push", &target_remote, &target_branch])
        .status()?;
    Ok(())
}
