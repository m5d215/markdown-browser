use std::path::{Path, PathBuf};

/// A loadable view target. Files are read from disk and can be
/// watched; URLs are fetched over HTTPS once per navigation;
/// directories carry no buffer and only drive the directory browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    File(PathBuf),
    Url(String),
    Dir(PathBuf),
}

impl Source {
    pub fn display(&self) -> String {
        match self {
            Self::File(p) => p.display().to_string(),
            Self::Url(u) => u.clone(),
            Self::Dir(p) => format!("{}/", p.display()),
        }
    }

    pub fn as_file(&self) -> Option<&Path> {
        match self {
            Self::File(p) => Some(p),
            Self::Url(_) | Self::Dir(_) => None,
        }
    }

    pub fn as_dir(&self) -> Option<&Path> {
        match self {
            Self::Dir(p) => Some(p),
            Self::File(_) | Self::Url(_) => None,
        }
    }

    /// Parse a CLI argument: treats `http(s)://` as a URL and everything
    /// else as a filesystem path.
    pub fn from_arg(arg: &str) -> Self {
        if is_url(arg) {
            Self::Url(arg.to_string())
        } else {
            Self::File(PathBuf::from(arg))
        }
    }
}

pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Look for a top-level README inside `dir`. Returns the first hit in
/// priority order — biased towards markdown extensions and the
/// `README` casing GitHub treats as canonical.
pub fn find_readme(dir: &Path) -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "README.md",
        "README.markdown",
        "readme.md",
        "readme.markdown",
        "index.md",
    ];
    for name in CANDIDATES {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// True when the URL's last path segment looks like plain text we can
/// render in-app: markdown / txt extensions, or an extensionless name
/// (LICENSE, README, etc.). Returns false for paths ending in a slash
/// (directory listings) and for any other extension (assumed binary or
/// HTML — punted to the OS).
pub fn url_path_is_renderable(url: &url::Url) -> bool {
    let path = url.path();
    let segment = match path.rsplit('/').find(|s| !s.is_empty()) {
        Some(s) if !path.ends_with('/') => s,
        _ => return false,
    };
    let lower = segment.to_ascii_lowercase();
    match lower.rfind('.') {
        Some(idx) => matches!(&lower[idx + 1..], "md" | "markdown" | "txt" | "text"),
        None => true,
    }
}
