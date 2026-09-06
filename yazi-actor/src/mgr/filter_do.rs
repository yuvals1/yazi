use anyhow::Result;
use yazi_fs::Filter;
use yazi_macro::{act, render, succ};
use yazi_parser::mgr::FilterForm;
use yazi_shared::data::Data;

use crate::{Actor, Ctx};

pub struct FilterDo;

impl Actor for FilterDo {
	type Form = FilterForm;

	const NAME: &str = "filter_do";

	fn act(cx: &mut Ctx, Self::Form { opt }: Self::Form) -> Result<Data> {
		let filter = if let Some(tags) = &opt.tags {
			// Tag-mode filter: support flat and deep
			let tags_vec: Vec<String> =
				tags.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
			if tags_vec.is_empty() {
				None
			} else if opt.deep {
				let mut f = Filter::new_tags_deep(tags_vec, opt.tag_all);
				if let Some(levels) = opt.levels {
					f = f.with_levels(levels);
				}
				f = f.with_root(cx.cwd().clone());
				Some(f)
			} else {
				Some(Filter::new_tags(tags_vec, opt.tag_all))
			}
		} else if opt.query.is_empty() {
			None
		} else {
			let mut f = if opt.deep {
				Filter::new_deep(&opt.query, opt.case)?
			} else {
				Filter::new(&opt.query, opt.case)?
			};
			if let Some(levels) = opt.levels {
				f = f.with_levels(levels);
			}
			// Record the root directory where the filter was invoked
			f = f.with_root(cx.cwd().clone());
			Some(f)
		};

		let hovered = cx.hovered().map(|f| f.key().into());
		// Apply to current folder
		let changed_current = cx.current_mut().entries.set_filter(filter.clone());

		// When deep is disabled or query cleared, purge any preview/history folder filters
		// to avoid stale deep-filter hiding items after exit.
		let mut changed_preview = false;
		if !opt.deep || filter.is_none() {
			let mut any = false;
			{
				let history = &mut cx.tab_mut().history;
				for f in history.values_mut() {
					any |= f.entries.set_filter(None);
				}
			}
			// Also clear on parent folder if it exists (e.g., the root where deep filter started)
			if let Some(parent) = cx.parent_mut() {
				any |= parent.entries.set_filter(None);
			}
			changed_preview |= any;
		}

		// Ensure preview folder respects deep-filter. When deep is disabled, clear it.
		if let Some(f) = cx.hovered_folder_mut() {
			changed_preview = if opt.deep { f.entries.set_filter(filter) } else { f.entries.set_filter(None) };
		}

		if cx.hovered().map(|f| f.key()) != hovered.as_ref().map(Into::into) {
			act!(mgr:hover, cx, hovered)?;
			act!(mgr:peek, cx)?;
			act!(mgr:watch, cx).ok();
		}

		// If the filter affected either current or preview, refresh the preview pane now
		if changed_current || changed_preview {
			act!(mgr:peek, cx, true)?;
		}

		if opt.done {
			act!(mgr:update_paged, cx)?;
		}

		succ!(render!());
	}
}
