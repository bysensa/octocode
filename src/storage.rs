use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

/// Get the system-wide storage directory for Octocode
/// Following XDG Base Directory specification on Unix-like systems
/// and proper conventions on other systems
pub fn get_system_storage_dir() -> Result<PathBuf> {
    let base_dir = if cfg!(target_os = "macos") {
        // macOS: ~/.local/share/octocode
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
            .join(".local")
            .join("share")
            .join("octocode")
    } else if cfg!(target_os = "windows") {
        // Windows: %APPDATA%/octocode
        dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine data directory"))?
            .join("octocode")
    } else {
        // Linux and other Unix-like: ~/.local/share/octocode or $XDG_DATA_HOME/octocode
        if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(xdg_data_home).join("octocode")
        } else {
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
                .join(".local")
                .join("share")
                .join("octocode")
        }
    };

    // Create the directory if it doesn't exist
    if !base_dir.exists() {
        fs::create_dir_all(&base_dir)?;
    }

    Ok(base_dir)
}

/// Get the project identifier for a given directory
/// First tries to get Git remote URL, falls back to path hash
pub fn get_project_identifier(project_path: &Path) -> Result<String> {
    // Try to get git remote URL first
    if let Ok(git_remote) = get_git_remote_url(project_path) {
        // Create a hash from the git remote URL
        let mut hasher = Sha256::new();
        hasher.update(git_remote.as_bytes());
        let result = hasher.finalize();
        return Ok(format!("{:x}", result)[..16].to_string()); // Use first 16 chars
    }

    // Fallback to absolute path hash
    let absolute_path = project_path.canonicalize()
        .or_else(|_| {
            // If canonicalize fails, try to get absolute path manually
            if project_path.is_absolute() {
                Ok(project_path.to_path_buf())
            } else {
                std::env::current_dir().map(|cwd| cwd.join(project_path))
            }
        })?;

    let mut hasher = Sha256::new();
    hasher.update(absolute_path.to_string_lossy().as_bytes());
    let result = hasher.finalize();
    Ok(format!("{:x}", result)[..16].to_string()) // Use first 16 chars
}

/// Try to get the Git remote URL for a project
fn get_git_remote_url(project_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .output()?;

    if output.status.success() {
        let url = String::from_utf8(output.stdout)?
            .trim()
            .to_string();
        
        if !url.is_empty() {
            return Ok(normalize_git_url(&url));
        }
    }

    Err(anyhow::anyhow!("No git remote found"))
}

/// Normalize git URL to be consistent regardless of protocol
/// e.g., https://github.com/user/repo.git and git@github.com:user/repo.git
/// both become github.com/user/repo
fn normalize_git_url(url: &str) -> String {
    let url = url.trim();
    
    // Remove .git suffix if present
    let url = if let Some(stripped) = url.strip_suffix(".git") {
        stripped
    } else {
        url
    };

    // Handle SSH format: git@host:user/repo
    if url.contains("@") && url.contains(":") && !url.contains("://") {
        if let Some(at_pos) = url.find('@') {
            if let Some(colon_pos) = url[at_pos..].find(':') {
                let host = &url[at_pos + 1..at_pos + colon_pos];
                let path = &url[at_pos + colon_pos + 1..];
                return format!("{}/{}", host, path);
            }
        }
    }

    // Handle HTTPS format: https://host/user/repo
    if url.starts_with("http://") || url.starts_with("https://") {
        if let Some(scheme_end) = url.find("://") {
            return url[scheme_end + 3..].to_string();
        }
    }

    // Return as-is if we can't parse it
    url.to_string()
}

/// Get the storage path for a specific project
pub fn get_project_storage_path(project_path: &Path) -> Result<PathBuf> {
    let system_dir = get_system_storage_dir()?;
    let project_id = get_project_identifier(project_path)?;
    
    Ok(system_dir.join(project_id))
}

/// Get the database path for a specific project
pub fn get_project_database_path(project_path: &Path) -> Result<PathBuf> {
    let project_storage = get_project_storage_path(project_path)?;
    Ok(project_storage.join("storage"))
}

/// Get the config path for a specific project (local to project)
/// Config remains local to projects for project-specific settings
pub fn get_project_config_path(project_path: &Path) -> Result<PathBuf> {
    Ok(project_path.join(".octocode"))
}

/// Get the system-wide cache directory for shared resources like FastEmbed models
pub fn get_system_cache_dir() -> Result<PathBuf> {
    let cache_dir = if cfg!(target_os = "macos") {
        // macOS: ~/.cache/octocode
        dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
            .join(".cache")
            .join("octocode")
    } else if cfg!(target_os = "windows") {
        // Windows: %LOCALAPPDATA%/octocode/cache
        dirs::cache_dir()
            .ok_or_else(|| anyhow::anyhow!("Unable to determine cache directory"))?
            .join("octocode")
    } else {
        // Linux and other Unix-like: ~/.cache/octocode or $XDG_CACHE_HOME/octocode
        if let Ok(xdg_cache_home) = std::env::var("XDG_CACHE_HOME") {
            PathBuf::from(xdg_cache_home).join("octocode")
        } else {
            dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?
                .join(".cache")
                .join("octocode")
        }
    };

    // Create the directory if it doesn't exist
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }

    Ok(cache_dir)
}

/// Get the system-wide FastEmbed cache directory
pub fn get_fastembed_cache_dir() -> Result<PathBuf> {
    let cache_dir = get_system_cache_dir()?.join("fastembed");
    
    // Create the directory if it doesn't exist
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir)?;
    }
    
    Ok(cache_dir)
}

/// Ensure the project storage directory exists
pub fn ensure_project_storage_exists(project_path: &Path) -> Result<PathBuf> {
    let storage_path = get_project_storage_path(project_path)?;
    
    if !storage_path.exists() {
        fs::create_dir_all(&storage_path)?;
    }
    
    Ok(storage_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_normalize_git_url() {
        // HTTPS URLs
        assert_eq!(
            normalize_git_url("https://github.com/user/repo.git"),
            "github.com/user/repo"
        );
        assert_eq!(
            normalize_git_url("https://github.com/user/repo"),
            "github.com/user/repo"
        );

        // SSH URLs
        assert_eq!(
            normalize_git_url("git@github.com:user/repo.git"),
            "github.com/user/repo"
        );
        assert_eq!(
            normalize_git_url("git@github.com:user/repo"),
            "github.com/user/repo"
        );

        // Other formats should remain unchanged
        assert_eq!(
            normalize_git_url("local/path/to/repo"),
            "local/path/to/repo"
        );
    }

    #[test]
    fn test_project_identifier() {
        let temp_dir = env::temp_dir().join("test_octocode");
        let _ = fs::create_dir_all(&temp_dir);
        
        // Should not panic and should return a consistent hash
        let id1 = get_project_identifier(&temp_dir).unwrap();
        let id2 = get_project_identifier(&temp_dir).unwrap();
        
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16); // Should be 16 characters
        
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_system_storage_dir() {
        let storage_dir = get_system_storage_dir().unwrap();
        
        // Should contain "octocode" in the path
        assert!(storage_dir.to_string_lossy().contains("octocode"));
        
        // Should be an absolute path
        assert!(storage_dir.is_absolute());
    }

    #[test]
    fn test_system_cache_dir() {
        let cache_dir = get_system_cache_dir().unwrap();
        
        // Should contain "octocode" in the path
        assert!(cache_dir.to_string_lossy().contains("octocode"));
        
        // Should be an absolute path
        assert!(cache_dir.is_absolute());
        
        // Should be different from storage directory
        let storage_dir = get_system_storage_dir().unwrap();
        assert_ne!(cache_dir, storage_dir);
    }

    #[test]
    fn test_fastembed_cache_dir() {
        let fastembed_cache = get_fastembed_cache_dir().unwrap();
        
        // Should contain both "octocode" and "fastembed" in the path
        assert!(fastembed_cache.to_string_lossy().contains("octocode"));
        assert!(fastembed_cache.to_string_lossy().contains("fastembed"));
        
        // Should be an absolute path
        assert!(fastembed_cache.is_absolute());
    }
}