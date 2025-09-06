use std::{ffi::OsStr, fmt::Display, ops::Range, path::Path};

use anyhow::Result;
use regex::bytes::{Regex, RegexBuilder};
use yazi_shared::event::Cmd;
use yazi_shared::url::UrlBuf;

#[derive(Clone)]
enum Match {
    NameRegex { regex: Regex },
    Tags { tags: Vec<String>, all: bool },
}

#[derive(Clone)]
pub struct Filter {
    raw:    String,
    matcher: Match,
    deep:   bool,
    levels: Option<usize>,
    root:   Option<UrlBuf>,
}

impl Filter {
	pub fn new(s: &str, case: FilterCase) -> Result<Self> {
		let regex = match case {
			FilterCase::Smart => {
				let uppercase = s.chars().any(|c| c.is_uppercase());
				RegexBuilder::new(s).case_insensitive(!uppercase).build()?
			}
			FilterCase::Sensitive => Regex::new(s)?,
			FilterCase::Insensitive => RegexBuilder::new(s).case_insensitive(true).build()?,
		};
        Ok(Self { raw: s.to_owned(), matcher: Match::NameRegex { regex }, deep: false, levels: None, root: None })
    }

	pub fn new_deep(s: &str, case: FilterCase) -> Result<Self> {
		let regex = match case {
			FilterCase::Smart => {
				let uppercase = s.chars().any(|c| c.is_uppercase());
				RegexBuilder::new(s).case_insensitive(!uppercase).build()?
			}
			FilterCase::Sensitive => Regex::new(s)?,
			FilterCase::Insensitive => RegexBuilder::new(s).case_insensitive(true).build()?,
		};
        Ok(Self { raw: s.to_owned(), matcher: Match::NameRegex { regex }, deep: true, levels: None, root: None })
    }

    pub fn new_tags(tags: Vec<String>, all: bool) -> Self {
        let raw = format!("tags:{}", tags.join(","));
        Self { raw, matcher: Match::Tags { tags, all }, deep: false, levels: None, root: None }
    }

    pub fn new_tags_deep(tags: Vec<String>, all: bool) -> Self {
        let raw = format!("tags:{}", tags.join(","));
        Self { raw, matcher: Match::Tags { tags, all }, deep: true, levels: None, root: None }
    }

    #[inline]
    pub fn with_levels(mut self, levels: usize) -> Self {
        self.levels = Some(levels);
        self
    }

    #[inline]
    pub fn with_root(mut self, root: UrlBuf) -> Self {
        self.root = Some(root);
        self
    }

	#[inline]
    pub fn matches(&self, name: impl AsRef<OsStr>) -> bool {
        match &self.matcher {
            Match::NameRegex { regex } => regex.is_match(name.as_ref().as_encoded_bytes()),
            Match::Tags { .. } => false,
        }
    }

    pub fn matches_entry(&self, name: impl AsRef<OsStr>, path: Option<&Path>) -> bool {
        match &self.matcher {
            Match::NameRegex { regex } => regex.is_match(name.as_ref().as_encoded_bytes()),
            Match::Tags { tags, all } => {
                if let Some(p) = path { has_required_tags(p, tags, *all) } else { false }
            }
        }
    }

	#[inline]
    pub fn highlighted(&self, name: impl AsRef<OsStr>) -> Option<Vec<Range<usize>>> {
        match &self.matcher {
            Match::NameRegex { regex } => regex.find(name.as_ref().as_encoded_bytes()).map(|m| vec![m.range()]),
            Match::Tags { .. } => None,
        }
    }

    #[inline]
    pub fn is_deep(&self) -> bool { self.deep }

    #[inline]
    pub fn levels(&self) -> Option<usize> { self.levels }

    #[inline]
    pub fn root(&self) -> Option<&UrlBuf> { self.root.as_ref() }

    #[inline]
    pub fn distance_from_root(&self, url: &UrlBuf) -> Option<usize> {
        let root = self.root.as_ref()?;
        let Some(root_path) = root.as_path() else { return None };
        let Some(url_path) = url.as_path() else { return None };
        let rel = url_path.strip_prefix(root_path).ok()?;
        // If same directory, distance is 0
        if rel.as_os_str().is_empty() { return Some(0) }
        Some(rel.components().count())
    }

	/// Check if a directory contains any matching files (recursively)
	/// Returns true if any file in the directory tree matches the filter
    pub fn dir_contains_match(&self, dir: &Path) -> bool {
        // "levels" means how many descendant levels to include relative to `dir`:
        // 0 = no recursion; 1 = include children; 2 = include grandchildren; etc.
        // Default fallback is 2 (children + grandchildren).
        let max_depth = self.levels.unwrap_or(2);
        if max_depth == 0 { return false; }

        match &self.matcher {
            Match::NameRegex { .. } => self.dir_contains_match_impl(dir, 0, max_depth),
            Match::Tags { tags, all } => self.dir_contains_match_impl_tags(dir, 0, max_depth, tags, *all),
        }
    }

    fn dir_contains_match_impl(&self, dir: &Path, depth: usize, max_depth: usize) -> bool {
		let Ok(entries) = std::fs::read_dir(dir) else {
			return false;
		};

		for entry in entries.flatten() {
			let Ok(ft) = entry.file_type() else {
				continue;
			};

			// Check the filename
			if let Some(name) = entry.file_name().to_str() {
				// A file at current iteration is at distance (depth + 1) from the original directory
				if depth + 1 <= max_depth && self.matches(name) {
					return true;
				}
			}

			// Recursively check subdirectories up to max_depth levels
			if ft.is_dir() && depth + 1 < max_depth {
				if self.dir_contains_match_impl(&entry.path(), depth + 1, max_depth) {
					return true;
				}
			}
		}

		false
    }

    fn dir_contains_match_impl_tags(
        &self,
        dir: &Path,
        depth: usize,
        max_depth: usize,
        required: &[String],
        all: bool,
    ) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };

            // Files at this level count if within depth budget
            if depth + 1 <= max_depth && !ft.is_dir() {
                if has_required_tags(&entry.path(), required, all) {
                    return true;
                }
            }

            // Subdirectories: recurse if within budget
            if ft.is_dir() && depth + 1 < max_depth {
                // A tagged subdirectory also satisfies containment
                if has_required_tags(&entry.path(), required, all) {
                    return true;
                }
                if self.dir_contains_match_impl_tags(&entry.path(), depth + 1, max_depth, required, all) {
                    return true;
                }
            }
        }
        false
    }
}

impl PartialEq for Filter {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw && self.deep == other.deep && self.levels == other.levels
    }
}

impl Display for Filter {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.raw) }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FilterCase {
	Smart,
	#[default]
	Sensitive,
	Insensitive,
}

impl From<&Cmd> for FilterCase {
	fn from(c: &Cmd) -> Self {
		match (c.bool("smart"), c.bool("insensitive")) {
			(true, _) => Self::Smart,
			(_, false) => Self::Sensitive,
			(_, true) => Self::Insensitive,
		}
	}
}

#[cfg(target_os = "macos")]
fn parse_mdls_tags_output(s: &str) -> Vec<String> {
    // mdls -raw kMDItemUserTags can output formats like:
    //   (\n    "Red",\n    "Blue"\n)
    // or
    //   (\n    Red\n)
    // or (null)
    let t = s.trim();
    if t == "(null)" || t.is_empty() { return vec![]; }

    // Strip surrounding parentheses if present
    let inner = t.strip_prefix('(').and_then(|u| u.strip_suffix(')')).unwrap_or(t);

    let mut tags = Vec::new();
    for line in inner.lines() {
        // Split by comma as well, since mdls may separate by commas
        for part in line.split(',') {
            let mut token = part.trim().trim_matches('"').trim().to_string();
            if token.is_empty() || token == "(null)" { continue; }
            // Normalize possible suffix like "Blue\n0" (usually seen in xattr, but be safe)
            if let Some((head, _)) = token.split_once('\n') { token = head.to_string(); }
            if !token.is_empty() { tags.push(token); }
        }
    }
    tags
}

#[cfg(target_os = "macos")]
fn mdls_tags(path: &Path) -> Option<Vec<String>> {
    use std::process::Command;
    let s = path.to_str()?;
    let output = Command::new("mdls")
        .args(["-raw", "-name", "kMDItemUserTags", s])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let t = text.trim();
    if t == "(null)" {
        return Some(Vec::new());
    }
    let tags = parse_mdls_tags_output(t);
    Some(tags)
}

#[cfg(not(target_os = "macos"))]
fn mdls_tags(_path: &Path) -> Option<Vec<String>> { None }

fn has_required_tags(path: &Path, required: &[String], all: bool) -> bool {
    let Some(existing) = mdls_tags(path) else { return false };
    if required.is_empty() { return false }
    if existing.is_empty() { return false }
    let mut exist_lower: Vec<String> = existing.into_iter().map(|s| s.to_lowercase()).collect();
    // Normalize possible suffixes like "Red\n0" by splitting at newline
    for e in &mut exist_lower {
        if let Some((head, _)) = e.split_once('\n') { *e = head.to_string(); }
    }
    if all {
        required.iter().all(|t| {
            let t = t.to_lowercase();
            let key = if let Some((h, _)) = t.split_once('\n') { h.to_string() } else { t };
            exist_lower.iter().any(|e| *e == key)
        })
    } else {
        required.iter().any(|t| {
            let t = t.to_lowercase();
            let key = if let Some((h, _)) = t.split_once('\n') { h.to_string() } else { t };
            exist_lower.iter().any(|e| *e == key)
        })
    }
}
