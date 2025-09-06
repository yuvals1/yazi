use anyhow::Result;
use yazi_fs::Filter;
use yazi_macro::{act, render, succ};
use yazi_parser::mgr::FilterOpt;
use yazi_shared::event::Data;

use crate::{Actor, Ctx};

pub struct FilterDo;

impl Actor for FilterDo {
	type Options = FilterOpt;

	const NAME: &str = "filter_do";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
        let filter = if opt.query.is_empty() {
            None
        } else {
            let mut f = if opt.deep { Filter::new_deep(&opt.query, opt.case)? } else { Filter::new(&opt.query, opt.case)? };
            if let Some(levels) = opt.levels { f = f.with_levels(levels); }
            Some(f)
        };

		let hovered = cx.hovered().map(|f| f.urn().to_owned());
		// Apply to current folder
		let changed_current = cx.current_mut().files.set_filter(filter.clone());

		// When deep is disabled or query cleared, purge any preview/history folder filters
		// to avoid stale deep-filter hiding items after exit.
		let mut changed_preview = false;
		if !opt.deep || filter.is_none() {
			let mut any = false;
			{
				let history = &mut cx.tab_mut().history;
				for f in history.values_mut() {
					any |= f.files.set_filter(None);
				}
			}
			changed_preview |= any;
		}

		// Ensure preview folder respects deep-filter. When deep is disabled, clear it.
		if let Some(f) = cx.hovered_folder_mut() {
			changed_preview = if opt.deep { f.files.set_filter(filter) } else { f.files.set_filter(None) };
		}

		if cx.hovered().map(|f| f.urn()) != hovered.as_deref() {
			act!(mgr:hover, cx, hovered)?;
			act!(mgr:peek, cx)?;
			act!(mgr:watch, cx)?;
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
