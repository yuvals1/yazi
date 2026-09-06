use serde::Deserialize;
use yazi_fs::file::File;
use yazi_shared::Condition;

use crate::{Icon, Mixable};

#[derive(Debug, Deserialize)]
pub struct IconCond {
	r#if:            Condition,
	#[serde(flatten)]
	pub(crate) icon: Icon,
}

impl IconCond {
	pub(crate) fn matches(&self, file: &File, hovered: bool) -> bool {
		self.eval_with_tag(file, hovered).0
	}

	// Evaluates the condition; the second return value reports whether any
	// "tag:"/"tag=" term was seen, so tag-priority matching can require it.
	pub(crate) fn eval_with_tag(&self, file: &File, hovered: bool) -> (bool, bool) {
		use std::cell::Cell;

		let seen_tag = Cell::new(false);
		let f = |s: &str| match s {
			"dir" => file.is_dir(),
			"hidden" => file.is_hidden(),
			"link" => file.is_link(),
			"orphan" => file.is_orphan(),
			"dummy" => file.is_dummy(),
			"block" => file.is_block(),
			"char" => file.is_char(),
			"fifo" => file.is_fifo(),
			"sock" => file.is_sock(),
			"exec" => file.is_exec(),
			"sticky" => file.is_sticky(),
			"hovered" => hovered,
			_ => {
				// Tag condition: allow terms like "tag:Red" or "tag=red"
				if let Some(rest) = s.strip_prefix("tag:").or_else(|| s.strip_prefix("tag=")) {
					seen_tag.set(true);
					let needle = rest.trim_matches(['"', '\\']).to_ascii_lowercase();
					file.tags().iter().any(|t| t.eq_ignore_ascii_case(&needle))
				} else {
					false
				}
			}
		};

		(self.r#if.eval(f) == Some(true), seen_tag.get())
	}
}

impl Mixable for IconCond {}
