use std::{fmt::Display, ops::Range, path::Path};

use anyhow::Result;
use regex::bytes::{Regex, RegexBuilder};
use yazi_shared::{event::Action, strand::AsStrand, url::{UrlBuf, UrlLike}};

use super::Normalizer;

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
		let pat = Normalizer::normalize(s)?;
		let regex = match case {
			FilterCase::Smart => {
				let uppercase = pat.chars().any(|c| c.is_uppercase());
				RegexBuilder::new(&pat).case_insensitive(!uppercase).build()?
			}
			FilterCase::Sensitive => Regex::new(&pat)?,
			FilterCase::Insensitive => RegexBuilder::new(&pat).case_insensitive(true).build()?,
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
	#[allow(private_bounds)]
	pub fn matches<T>(&self, name: T) -> bool
	where
		T: AsStrand,
	{
		match &self.matcher {
			Match::NameRegex { regex } => regex.is_match(name.as_strand().encoded_bytes()),
			Match::Tags { .. } => false,
		}
	}

	#[allow(private_bounds)]
	pub fn matches_entry<T>(&self, name: T, path: Option<&Path>) -> bool
	where
		T: AsStrand,
	{
		match &self.matcher {
			Match::NameRegex { regex } => regex.is_match(name.as_strand().encoded_bytes()),
			Match::Tags { tags, all } => {
				// Direct entry checks should bypass cache so UI changes to tags
				// are reflected immediately in filtering results.
				if let Some(p) = path { has_required_tags_nocache(p, tags, *all) } else { false }
			}
		}
	}

	#[allow(private_bounds)]
	pub fn highlighted<T>(&self, name: T) -> Option<Vec<Range<usize>>>
	where
		T: AsStrand,
	{
		match &self.matcher {
			Match::NameRegex { regex } => regex.find(name.as_strand().encoded_bytes()).map(|m| vec![m.range()]),
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
		let root_path = root.as_local()?;
		let url_path = url.as_local()?;
		let rel = url_path.strip_prefix(root_path).ok()?;
		// If same directory, distance is 0
		if rel.as_os_str().is_empty() {
			return Some(0);
		}
		Some(rel.components().count())
	}

	/// Check if a directory contains any matching files (recursively)
	/// Returns true if any file in the directory tree matches the filter
	pub fn dir_contains_match(&self, dir: &Path) -> bool {
		// "levels" means how many descendant levels to include relative to `dir`:
		// 0 = no recursion; 1 = include children; 2 = include grandchildren; etc.
		// Default fallback is 2 (children + grandchildren).
		let max_depth = self.levels.unwrap_or(2);
		if max_depth == 0 {
			return false;
		}

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
			if ft.is_dir() && depth + 1 < max_depth && self.dir_contains_match_impl(&entry.path(), depth + 1, max_depth) {
				return true;
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
			if depth + 1 <= max_depth && !ft.is_dir() && has_required_tags_nocache(&entry.path(), required, all) {
				return true;
			}

			// Subdirectories: recurse if within budget
			if ft.is_dir() && depth + 1 < max_depth {
				// A tagged subdirectory also satisfies containment
				if has_required_tags_nocache(&entry.path(), required, all) {
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

// --- FilterCase
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FilterCase {
	Smart,
	#[default]
	Sensitive,
	Insensitive,
}

impl From<&Action> for FilterCase {
	fn from(a: &Action) -> Self {
		match (a.bool("smart"), a.bool("insensitive")) {
			(true, _) => Self::Smart,
			(_, false) => Self::Sensitive,
			(_, true) => Self::Insensitive,
		}
	}
}


// Old cached version retained for reference; remove if not needed in future
// fn has_required_tags(path: &Path, required: &[String], all: bool) -> bool { ... }

// Non-cached variant for immediate correctness after tag mutations.
fn has_required_tags_nocache(path: &Path, required: &[String], all: bool) -> bool {
    if required.is_empty() { return false }
    let existing = crate::tags::tags_for_nocache(path);
    if existing.is_empty() { return false }

    if all {
        required.iter().all(|t| {
            let key = t.split_once('\n').map(|(h, _)| h).unwrap_or(t).to_lowercase();
            existing.iter().any(|e| *e == key)
        })
    } else {
        required.iter().any(|t| {
            let key = t.split_once('\n').map(|(h, _)| h).unwrap_or(t).to_lowercase();
            existing.iter().any(|e| *e == key)
        })
    }
}
