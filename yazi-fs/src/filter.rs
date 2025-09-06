use std::{ffi::OsStr, fmt::Display, ops::Range, path::Path};

use anyhow::Result;
use regex::bytes::{Regex, RegexBuilder};
use yazi_shared::event::Cmd;
use yazi_shared::url::UrlBuf;

#[derive(Clone)]
pub struct Filter {
    raw:   String,
    regex: Regex,
    deep:  bool,
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
        Ok(Self { raw: s.to_owned(), regex, deep: false, levels: None, root: None })
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
        Ok(Self { raw: s.to_owned(), regex, deep: true, levels: None, root: None })
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
		self.regex.is_match(name.as_ref().as_encoded_bytes())
	}

	#[inline]
	pub fn highlighted(&self, name: impl AsRef<OsStr>) -> Option<Vec<Range<usize>>> {
		self.regex.find(name.as_ref().as_encoded_bytes()).map(|m| vec![m.range()])
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
        // Quick non-recursive implementation for now
        // We'll improve this with proper async scanning later
        // "levels" means how many descendant levels to include relative to the directory being checked:
        // - 0 = no recursion; 1 = include children; 2 = include grandchildren; etc.
        // Default fallback is 2 (children + grandchildren).
        let max_depth = self.levels.unwrap_or(2);
        self.dir_contains_match_impl(dir, 0, max_depth)
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
