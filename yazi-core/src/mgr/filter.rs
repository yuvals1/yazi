use yazi_fs::FilterCase;
use yazi_macro::impl_data_any;
use yazi_shared::event::ActionCow;
use yazi_shim::SStr;

#[derive(Clone, Debug, Default)]
pub struct FilterOpt {
	pub query: SStr,
	pub case:  FilterCase,
	pub done:  bool,
	pub deep:  bool,
	pub levels: Option<usize>,
	pub tags: Option<SStr>,
	pub tag_all: bool,
	pub combine_or: bool,
}

impl_data_any!(FilterOpt);

impl TryFrom<ActionCow> for FilterOpt {
	type Error = anyhow::Error;

	fn try_from(mut a: ActionCow) -> Result<Self, Self::Error> {
		Ok(Self {
			query: a.take_first().unwrap_or_default(),
			case: FilterCase::from(&*a),
			done: a.bool("done"),
			deep: a.bool("deep"),
			levels: a.take("levels").ok(),
			tags: a.take("tags").ok(),
			tag_all: a.bool("tag-all") || a.bool("tag_all"),
			// Parse combine mode: default AND; allow --combine=or
			combine_or: matches!(a.str("combine").to_ascii_lowercase().as_str(), "or"),
		})
	}
}
