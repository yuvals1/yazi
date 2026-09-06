use std::{mem, time::Duration};

use anyhow::Result;
use tokio::pin;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use yazi_config::YAZI;
use yazi_core::mgr::CdSource;
use yazi_dds::Pubsub;
use yazi_fs::{FilesOp, path::{clean_url, expand_url}};
use yazi_macro::{act, input, log_if_err, render, succ};
use yazi_parser::mgr::CdForm;
use yazi_proxy::{CmpProxy, MgrProxy};
use yazi_shared::{Debounce, data::Data, url::{AsUrl, UrlBuf, UrlLike}};
use yazi_vfs::engine;
use yazi_widgets::input::InputEvent;

use crate::{Actor, Ctx};

pub struct Cd;

impl Actor for Cd {
	type Form = CdForm;

	const NAME: &str = "cd";

	fn act(cx: &mut Ctx, form: Self::Form) -> Result<Data> {
		act!(mgr:escape_visual, cx)?;
		if form.interactive {
			return Self::cd_interactive(cx);
		} else if form.target == *cx.cwd() {
			succ!();
		}

		// Carry over an active deep-filter if navigating within its root subtree
		let prev_filter = cx.current().entries.filter().cloned();

		// Stash first so it's possible to access the original cwd in hooks
		act!(mgr:stash, cx, &form).ok();

		// Take parent to history
		let tab = cx.tab_mut();
		if let Some(t) = tab.parent.take() {
			tab.history.insert(t);
		}

		// Current
		let rep = tab.history.remove_or(&form.target);
		let rep = mem::replace(&mut tab.current, rep);
		tab.history.insert(rep);

		// Parent
		if let Some(parent) = form.target.parent() {
			tab.parent = Some(tab.history.remove_or(parent));
		}

		// Apply previous deep filter to the new current folder if still under the same root
		if let Some(f) = prev_filter {
			if f.is_deep() {
				let apply = match (f.root(), form.target.as_local()) {
					(Some(root), Some(tp)) => root.as_local().is_some_and(|rp| tp.starts_with(rp)),
					// If no root recorded, conservatively apply
					(None, _) => true,
					_ => false,
				};
				if apply {
					tab.current.entries.set_filter(Some(f));
				} else {
					// Leaving the filter root: clear any residual filter on the destination
					tab.current.entries.set_filter(None);
				}
			}
		}

		log_if_err!(Pubsub::pub_after_cd(tab.id, tab.cwd()));
		act!(mgr:displace, cx)?;
		act!(mgr:hidden, cx).ok();
		act!(mgr:sort, cx).ok();
		act!(mgr:hover, cx)?;
		act!(mgr:refresh, cx)?;
		act!(app:title, cx).ok();
		succ!(render!());
	}
}

impl Cd {
	fn cd_interactive(cx: &mut Ctx) -> Result<Data> {
		let input = input!(cx, YAZI.input.cd(cx.cwd().as_url()))?;

		tokio::spawn(async move {
			let rx = Debounce::new(UnboundedReceiverStream::new(input), Duration::from_millis(50));
			pin!(rx);

			while let Some(result) = rx.next().await {
				match result {
					InputEvent::Submit(s) => {
						let Ok(url) = UrlBuf::try_from(s).map(expand_url) else { return };
						let Ok(url) = engine::absolute(&url).await else { return };
						let url = clean_url(url);

						let Ok(file) = engine::file(&url).await else { return };
						if file.is_dir() {
							return MgrProxy::cd(&url, CdSource::Cd);
						}

						if let Some((t, k)) = url.pair() {
							FilesOp::Upserting(t.into(), [(k.into(), file)].into()).emit();
						}
						MgrProxy::reveal(url);
					}
					InputEvent::Trigger(before, ticket) => {
						CmpProxy::trigger(before, ticket);
					}
					_ => break,
				}
			}
		});
		succ!();
	}
}
