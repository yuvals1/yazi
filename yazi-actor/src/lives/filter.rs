use std::ops::Deref;

use mlua::{AnyUserData, MetaMethod, UserData, UserDataMethods};

use super::{Lives, PtrCell};

pub(super) struct Filter {
    inner: PtrCell<yazi_fs::Filter>,
    tab:   PtrCell<yazi_core::tab::Tab>,
}

impl Deref for Filter {
	type Target = yazi_fs::Filter;

	fn deref(&self) -> &Self::Target { &self.inner }
}

impl Filter {
    pub(super) fn make(inner: &yazi_fs::Filter, tab: &yazi_core::tab::Tab) -> mlua::Result<AnyUserData> {
        Lives::scoped_userdata(Self { inner: inner.into(), tab: tab.into() })
    }
}

impl UserData for Filter {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::ToString, |_, me, ()| Ok(me.to_string()));
        methods.add_method("is_deep", |_, me, ()| Ok(me.is_deep()));
        methods.add_method("levels", |_, me, ()| Ok(me.levels()));
        methods.add_method("distance", |_, me, ()| Ok(me.inner.distance_from_root(&me.tab.current.url)));
    }
}
