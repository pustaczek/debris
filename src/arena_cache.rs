use std::{cell::RefCell, collections::HashMap, hash::Hash};

pub struct ArenaCache<K: Hash+Eq, V> {
	entries: RefCell<HashMap<K, Box<V>>>,
}

impl<K: Hash+Eq, V> ArenaCache<K, V> {
	pub fn new() -> ArenaCache<K, V> {
		ArenaCache {
			entries: RefCell::new(HashMap::new()),
		}
	}

	pub fn query<'a>(&'a self, key: K, computation: impl FnOnce(&K) -> V) -> &'a V {
		if !self.entries.borrow().contains_key(&key) {
			let value = Box::new(computation(&key));
			let value_ref = unsafe { (&*value as *const V).as_ref() }.unwrap();
			self.entries.borrow_mut().insert(key, value);
			value_ref
		} else {
			unsafe { (self.entries.borrow()[&key].as_ref() as *const V).as_ref() }.unwrap()
		}
	}
}
