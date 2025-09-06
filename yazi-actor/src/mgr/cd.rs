use std::{mem, time::Duration};

use anyhow::Result;
use tokio::pin;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use yazi_config::popup::InputCfg;
use yazi_dds::Pubsub;
use yazi_fs::{File, FilesOp, path::expand_url};
use yazi_macro::{act, err, render, succ};
use yazi_parser::mgr::CdOpt;
use yazi_proxy::{CmpProxy, InputProxy, MgrProxy};
use yazi_shared::{Debounce, errors::InputError, event::Data, url::UrlBuf};

use crate::{Actor, Ctx};

pub struct Cd;

impl Actor for Cd {
	type Options = CdOpt;

	const NAME: &str = "cd";

    fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
        act!(mgr:escape_visual, cx)?;
        if opt.interactive {
            return Self::cd_interactive();
        }

        // Carry over an active deep-filter if navigating within its root subtree
        let prev_filter = cx.current().files.filter().cloned();

        let tab = cx.tab_mut();
        if opt.target == *tab.cwd() {
            succ!();
        }

		// Take parent to history
		if let Some(rep) = tab.parent.take() {
			tab.history.insert(rep.url.to_owned(), rep);
		}

		// Backstack
		if opt.source.big_jump() {
			if tab.current.url.is_regular() {
				tab.backstack.push(tab.current.url.as_url());
			}
			if opt.target.is_regular() {
				tab.backstack.push(opt.target.as_url());
			}
		}

		// Current
		let rep = tab.history.remove_or(&opt.target);
		let rep = mem::replace(&mut tab.current, rep);
		tab.history.insert(rep.url.to_owned(), rep);

        // Parent
        if let Some(parent) = opt.target.parent() {
            tab.parent = Some(tab.history.remove_or(parent));
        }

        // Apply previous deep filter to the new current folder if still under the same root
        if let Some(f) = prev_filter {
            if f.is_deep() {
                let apply = match (f.root(), yazi_shared::url::UrlBuf::from(&opt.target).as_path()) {
                    (Some(root), Some(tp)) => root.as_path().is_some_and(|rp| tp.starts_with(rp)),
                    // If no root recorded, conservatively apply
                    (None, _) => true,
                    _ => false,
                };
                if apply {
                    tab.current.files.set_filter(Some(f));
                } else {
                    // Leaving the filter root: clear any residual filter on the destination
                    tab.current.files.set_filter(None);
                }
            }
        }

        err!(Pubsub::pub_after_cd(tab.id, tab.cwd()));
        act!(mgr:hidden, cx)?;
        act!(mgr:sort, cx)?;
        act!(mgr:hover, cx)?;
        act!(mgr:refresh, cx)?;
        succ!(render!());
    }
}

impl Cd {
	fn cd_interactive() -> Result<Data> {
		let input = InputProxy::show(InputCfg::cd());

		tokio::spawn(async move {
			let rx = Debounce::new(UnboundedReceiverStream::new(input), Duration::from_millis(50));
			pin!(rx);

			while let Some(result) = rx.next().await {
				match result {
					Ok(s) => {
						let Ok(url) = UrlBuf::try_from(s).map(expand_url) else { return };

						let Ok(file) = File::new(&url).await else { return };
						if file.is_dir() {
							return MgrProxy::cd(&url);
						}

						if let Some(p) = url.parent() {
							FilesOp::Upserting(p.into(), [(url.urn().to_owned(), file)].into()).emit();
						}
						MgrProxy::reveal(&url);
					}
					Err(InputError::Completed(before, ticket)) => {
						CmpProxy::trigger(&before, ticket);
					}
					_ => break,
				}
			}
		});
		succ!();
	}
}
