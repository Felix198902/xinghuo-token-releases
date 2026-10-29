use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopRuntimeStatus {
    pub helper_installed: bool,
    pub workspace_installed: bool,
    pub offline_installer_url: String,
}

pub fn current_status() -> DesktopRuntimeStatus {
    let (helper_installed, workspace_installed) = evaluate_roots(&runtime_roots());
    DesktopRuntimeStatus {
        helper_installed,
        workspace_installed,
        offline_installer_url: offline_installer_url_for(
            std::env::consts::OS,
            std::env::consts::ARCH,
        )
        .to_string(),
    }
}

fn evaluate_roots(roots: &[PathBuf]) -> (bool, bool) {
    let helper_installed = roots
        .iter()
        .any(|root| contains_named_file(&root.join("claude-code"), &["claude", "claude.exe"], 8));
    let workspace_installed = roots.iter().any(|root| {
        has_entries(&root.join("claude-code-vm")) || has_entries(&root.join("vm_bundles"))
    });
    (helper_installed, workspace_installed)
}

fn contains_named_file(directory: &Path, names: &[&str], depth: usize) -> bool {
    if depth == 0 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| names.contains(&name))
        {
            return true;
        }
        if path.is_dir() && contains_named_file(&path, names, depth - 1) {
            return true;
        }
    }
    false
}

fn has_entries(directory: &Path) -> bool {
    std::fs::read_dir(directory)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn runtime_roots() -> Vec<PathBuf> {
    let home = crate::config::get_home_dir();
    #[cfg(target_os = "macos")]
    {
        let support = home.join("Library/Application Support");
        return vec![support.join("Claude-3p"), support.join("Claude")];
    }
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Local"));
        let roaming = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData/Roaming"));
        return vec![local.join("Claude-3p"), roaming.join("Claude")];
    }
    #[cfg(target_os = "linux")]
    {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        return vec![config.join("Claude-3p"), config.join("Claude")];
    }
    #[allow(unreachable_code)]
    Vec::new()
}

fn offline_installer_url_for(os: &str, arch: &str) -> &'static str {
    let windows_arch = if arch == "aarch64" { "arm64" } else { "x64" };
    let mac_arch = if arch == "aarch64" { "arm64" } else { "x64" };
    match os {
        "windows" => match windows_arch {
            "arm64" => "https://claude.ai/api/desktop/win32/arm64/offline/latest/redirect",
            _ => "https://claude.ai/api/desktop/win32/x64/offline/latest/redirect",
        },
        "macos" => match mac_arch {
            "arm64" => "https://claude.ai/api/desktop/darwin/arm64/offline/latest/redirect",
            _ => "https://claude.ai/api/desktop/darwin/x64/offline/latest/redirect",
        },
        _ => "https://claude.com/docs/third-party/claude-desktop/installation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_real_helper_and_workspace_files() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Claude-3p");
        let helper = root.join("claude-code/v1/build/claude.exe");
        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::write(helper, "binary").unwrap();
        std::fs::create_dir_all(root.join("claude-code-vm/workspace")).unwrap();

        assert_eq!(evaluate_roots(&[root]), (true, true));
    }

    #[test]
    fn empty_download_directories_are_not_ready() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Claude-3p");
        std::fs::create_dir_all(root.join("claude-code")).unwrap();
        std::fs::create_dir_all(root.join("vm_bundles")).unwrap();

        assert_eq!(evaluate_roots(&[root]), (false, false));
    }

    #[test]
    fn offline_urls_match_platform_and_architecture() {
        assert!(offline_installer_url_for("windows", "x86_64").contains("win32/x64/"));
        assert!(offline_installer_url_for("windows", "aarch64").contains("win32/arm64/"));
        assert!(offline_installer_url_for("macos", "aarch64").contains("darwin/arm64/"));
        assert!(offline_installer_url_for("macos", "x86_64").contains("darwin/x64/"));
    }
}
