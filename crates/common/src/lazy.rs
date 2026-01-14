use std::{ops::Deref, sync::OnceLock};

pub struct Lazy<T> {
    pub value: OnceLock<T>,
    init: Option<fn() -> T>,
}

impl<T> Lazy<T> {
    pub const fn new(init: Option<fn() -> T>) -> Self {
        Self {
            value: OnceLock::new(),
            init,
        }
    }
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        if let Some(init) = self.init {
            self.value.get_or_init(init)
        } else {
            self.value.get().unwrap()
        }
    }
}
