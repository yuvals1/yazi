use std::time::Duration;

use anyhow::Result;
use tokio::pin;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};
use yazi_config::YAZI;
use yazi_core::mgr::FilterOpt;
use yazi_macro::{input, succ};
use yazi_parser::mgr::FilterForm;
use yazi_proxy::MgrProxy;
use yazi_shared::{Debounce, data::Data};
use yazi_widgets::input::InputEvent;

use crate::{Actor, Ctx};

pub struct Filter;

impl Actor for Filter {
	type Form = FilterForm;

	const NAME: &str = "filter";

	fn act(cx: &mut Ctx, Self::Form { opt }: Self::Form) -> Result<Data> {
		let mut input_opt = YAZI.input.filter();
		if opt.deep {
			let lv = opt.levels.unwrap_or(2);
			input_opt.title = format!("Filter (deep L={lv}):");
		}
		let input = input!(cx, input_opt)?;

		tokio::spawn(async move {
			let rx = Debounce::new(UnboundedReceiverStream::new(input), Duration::from_millis(50));
			pin!(rx);

			while let Some(event) = rx.next().await {
				let done = event.is_submit();
				let (InputEvent::Submit(s) | InputEvent::Type(s)) = event else { continue };

                MgrProxy::filter_do(FilterOpt {
                    query: s.into(),
                    case: opt.case,
                    done,
                    deep: opt.deep,
                    levels: opt.levels,
                    tags: opt.tags.clone(),
                    tag_all: opt.tag_all,
                    combine_or: opt.combine_or,
                });
            }
        });
        succ!();
    }
}
