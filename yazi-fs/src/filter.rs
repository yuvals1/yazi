use std::{ffi::OsStr, fmt::Display, ops::Range, path::Path};

use anyhow::Result;
use regex::bytes::{Regex, RegexBuilder};
use yazi_shared::event::Cmd;

#[derive(Clone)]
pub struct Filter {
	raw:   String,
	regex: Regex,
	deep:  bool,
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
		Ok(Self { raw: s.to_owned(), regex, deep: false })
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
		Ok(Self { raw: s.to_owned(), regex, deep: true })
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

	/// Check if a directory contains any matching files (recursively)
	/// Returns true if any file in the directory tree matches the filter
	pub fn dir_contains_match(&self, dir: &Path) -> bool {
		// Quick non-recursive implementation for now
		// We'll improve this with proper async scanning later
		self.dir_contains_match_impl(dir, 0, 3) // Max depth of 3 for now
	}

	fn dir_contains_match_impl(&self, dir: &Path, depth: usize, max_depth: usize) -> bool {
		if depth >= max_depth {
			return false;
		}

		let Ok(entries) = std::fs::read_dir(dir) else {
			return false;
		};

		for entry in entries.flatten() {
			let Ok(ft) = entry.file_type() else {
				continue;
			};

			// Check the filename
			if let Some(name) = entry.file_name().to_str() {
				if self.matches(name) {
					return true;
				}
			}

			// Recursively check subdirectories
			if ft.is_dir() {
				if self.dir_contains_match_impl(&entry.path(), depth + 1, max_depth) {
					return true;
				}
			}
		}

		false
	}
}

impl PartialEq for Filter {
	fn eq(&self, other: &Self) -> bool { self.raw == other.raw && self.deep == other.deep }
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
