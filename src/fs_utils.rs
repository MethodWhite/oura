use std::path::{Path, PathBuf};

pub fn safe_path(input: &str) -> Result<PathBuf, String> {
    let path = Path::new(input);
    if !path.exists() {
        return Err(format!("Path not found: {}", input));
    }
    std::fs::canonicalize(path)
        .map_err(|e| format!("Cannot resolve path {}: {}", input, e))
}

pub fn dir_size(path: &Path, max_depth: usize, max_files: usize) -> std::io::Result<u64> {
    let mut total = 0u64;
    let mut visited = std::collections::HashSet::new();
    if path.is_file() {
        return Ok(path.metadata()?.len());
    }
    walk_size(path, 0, max_depth, &mut total, &mut visited, &mut 0usize, max_files)?;
    Ok(total)
}

fn walk_size(
    path: &Path, depth: usize, max_depth: usize, total: &mut u64,
    visited: &mut std::collections::HashSet<u64>, count: &mut usize, max_files: usize,
) -> std::io::Result<()> {
    if depth > max_depth || *count > max_files { return Ok(()); }
    if path.is_dir() {
        walk_size_inner(path, depth, max_depth, total, visited, count, max_files)
    } else { Ok(()) }
}

fn walk_size_inner(
    path: &Path, depth: usize, max_depth: usize, total: &mut u64,
    visited: &mut std::collections::HashSet<u64>, count: &mut usize, max_files: usize,
) -> std::io::Result<()> {
    if depth > max_depth || *count > max_files { return Ok(()); }
    let pk = path_key(path);
    if pk != 0 && !visited.insert(pk) { return Ok(()); }
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            let p = e.path();
            if p.is_symlink() { continue; }
            if p.is_file() {
                *total += e.metadata()?.len();
                *count += 1;
                if *count > max_files { return Ok(()); }
            } else if p.is_dir() {
                walk_size_inner(&p, depth + 1, max_depth, total, visited, count, max_files)?;
            }
        }
    }
    Ok(())
}

fn path_key(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        if let Ok(meta) = path.metadata() {
            return std::os::unix::fs::MetadataExt::ino(&meta);
        }
    }
    #[cfg(not(unix))]
    {
        use std::hash::{Hash, Hasher};
        if let Ok(canonical) = std::fs::canonicalize(path) {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            canonical.hash(&mut hasher);
            return hasher.finish();
        }
    }
    0
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    if bytes == 0 { return "0B".into(); }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0; unit += 1;
    }
    format!("{:.1}{}", size, UNITS[unit])
}

pub fn find_readme(root: &Path) -> Option<PathBuf> {
    for name in &["README.md", "Readme.md", "readme.md", "README", "README.txt"] {
        let p = root.join(name);
        if p.exists() { return Some(p); }
    }
    None
}

pub fn scan_configs(root: &Path) -> Vec<String> {
    let mut configs = Vec::new();
    for name in &["Cargo.toml", "package.json", "pyproject.toml", "go.mod", "build.gradle", "CMakeLists.txt"] {
        if root.join(name).exists() { configs.push(name.to_string()); }
    }
    configs
}

pub fn collect_entries(
    path: &Path, depth: usize, max_depth: usize, max_entries: usize,
) -> Vec<(String, usize, bool, u64)> {
    let mut entries = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut count = 0usize;
    collect_entries_inner(path, depth, max_depth, &mut entries, &mut visited, &mut count, max_entries);
    entries
}

fn collect_entries_inner(
    path: &Path, depth: usize, max_depth: usize,
    entries: &mut Vec<(String, usize, bool, u64)>,
    visited: &mut std::collections::HashSet<u64>, count: &mut usize, max_entries: usize,
) {
    if depth > max_depth || !path.is_dir() || *count > max_entries { return; }
    let pk = path_key(path);
    if pk != 0 && !visited.insert(pk) { return; }
    if let Ok(readdir) = std::fs::read_dir(path) {
        for entry in readdir.flatten() {
            let p = entry.path();
            if p.is_symlink() { continue; }
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.starts_with('.') { continue; }
            let is_dir = p.is_dir();
            let size = if is_dir { 0 } else { p.metadata().map(|m| m.len()).unwrap_or(0) };
            entries.push((name.clone(), depth, is_dir, size));
            *count += 1;
            if *count > max_entries { return; }
            if is_dir {
                collect_entries_inner(&p, depth + 1, max_depth, entries, visited, count, max_entries);
            }
        }
    }
}

pub struct CleanupContext {
    patterns: Vec<String>,
    dir_patterns: Vec<String>,
    candidates: Vec<(String, String)>,
    total_size: u64,
    now: u64,
    max_age: u64,
    max_depth: usize,
}

impl CleanupContext {
    pub fn new(
        patterns: Vec<String>, dir_patterns: Vec<String>,
        now: u64, max_age: u64, max_depth: usize,
    ) -> Self {
        Self { patterns, dir_patterns, candidates: Vec::new(), total_size: 0, now, max_age, max_depth }
    }

    pub fn walk(&mut self, path: &Path, depth: usize) {
        let mut visited = std::collections::HashSet::new();
        self.walk_inner(path, depth, &mut visited);
    }

    fn walk_inner(&mut self, path: &Path, depth: usize, visited: &mut std::collections::HashSet<u64>) {
        if depth > self.max_depth { return; }
    let pk = path_key(path);
    if pk != 0 && !visited.insert(pk) { return; }
    if let Ok(readdir) = std::fs::read_dir(path) {
            for entry in readdir.flatten() {
                let p = entry.path();
                if p.is_symlink() { continue; }
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if p.is_dir() {
                    if self.dir_patterns.iter().any(|d| name == d.as_str()) {
                        let size = dir_size(&p, 5, 1000).unwrap_or(0);
                        self.candidates.push((p.to_string_lossy().to_string(), "dir".into()));
                        self.total_size += size;
                    } else {
                        self.walk_inner(&p, depth + 1, visited);
                    }
                } else if self.patterns.iter().any(|pat| {
                    if let Some(ext) = pat.strip_prefix('*') {
                        name.ends_with(ext)
                    } else { name == *pat }
                }) {
                    let aged = self.now.saturating_sub(
                        entry.metadata().ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs()).unwrap_or(0)
                    );
                    if aged >= self.max_age {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        self.candidates.push((p.to_string_lossy().to_string(), "file".into()));
                        self.total_size += size;
                    }
                }
            }
        }
    }

    pub fn candidates(&self) -> &[(String, String)] { &self.candidates }
    pub fn total_size(&self) -> u64 { self.total_size }
}

pub async fn run_command_timeout(
    args: &[&str], dir: &Path, secs: u64,
) -> Result<std::process::Output, String> {
    let program = args[0].to_string();
    let cmd_args: Vec<String> = args[1..].iter().map(|s| s.to_string()).collect();
    let dir = dir.to_path_buf();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(secs),
        tokio::process::Command::new(&program)
            .args(&cmd_args)
            .current_dir(&dir)
            .output(),
    ).await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(format!("Command failed: {}", e)),
        Err(_) => Err(format!("Command timed out after {}s: {:?}", secs, args)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0B");
        assert_eq!(format_size(512), "512.0B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0KB");
        assert_eq!(format_size(2048), "2.0KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1048576), "1.0MB");
    }

    #[test]
    fn test_format_size_gb() {
        let one_gb = 1073741824u64;
        assert_eq!(format_size(one_gb), "1.0GB");
    }

    #[test]
    fn test_find_readme_nonexistent() {
        let dir = TempDir::new().unwrap();
        assert!(find_readme(dir.path()).is_none());
    }

    #[test]
    fn test_find_readme_exists() {
        let dir = TempDir::new().unwrap();
        let readme_path = dir.path().join("README.md");
        std::fs::write(&readme_path, "# Test").unwrap();
        assert_eq!(find_readme(dir.path()), Some(readme_path));
    }

    #[test]
    fn test_scan_configs_cargo() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        let configs = scan_configs(dir.path());
        assert!(configs.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn test_scan_configs_empty() {
        let dir = TempDir::new().unwrap();
        let configs = scan_configs(dir.path());
        assert!(configs.is_empty());
    }

    #[test]
    fn test_collect_entries_empty_dir() {
        let dir = TempDir::new().unwrap();
        let entries = collect_entries(dir.path(), 0, 3, 100);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_dir_size_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello").unwrap();
        let size = dir_size(&file, 0, 100).unwrap();
        assert_eq!(size, 5);
    }

    #[test]
    fn test_safe_path_nonexistent() {
        let result = safe_path("/nonexistent/path/that/does/not/exist");
        assert!(result.is_err());
    }
}
