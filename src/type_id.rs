use std::fmt;

#[derive(Clone)]
pub struct Type {
	name: String,
}
impl Type {
	pub fn of<T: typename::TypeName+'static>() -> Type {
		Type {
			name: T::type_name(),
		}
	}
}
impl fmt::Debug for Type {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.name)
	}
}