use std::{io, fmt};
use scraper::Selector;
use std::str::FromStr;

mod arena_cache;
mod type_id;

pub struct Error {
	reason: Reason,
	operations: Vec<Operation>,
}
pub type Result<T> = std::result::Result<T, Error>;

pub trait Find: Context {
	fn find_all(&self, selector: &'static str) -> Collection;
	fn find(&self, selector: &'static str) -> Result<Node> {
		let mut iter = self.find_all(selector).iterator;
		let element = iter.next();
		let is_only = iter.next().is_none();
		match element {
			Some(element) if is_only => Ok(Node {
				document: self.get_document(),
				source: self.get_as_source(),
				operation: Operation::Find { selector },
				element,
			}),
			Some(_) => Err(self.make_error(Reason::MultipleFound, Operation::Find { selector })),
			None => Err(self.make_error(Reason::NotFound, Operation::Find { selector })),
		}
	}
	fn find_first(&self, selector: &'static str) -> Result<Node> {
		match self.find_all(selector).iterator.next() {
			Some(element) => Ok(Node {
				document: self.get_document(),
				source: self.get_as_source(),
				operation: Operation::FindFirst { selector },
				element
			}),
			None => Err(self.make_error(Reason::NotFound, Operation::FindFirst { selector })),
		}

	}
}
pub trait Context {
	fn get_document(&self) -> &Document;
	fn get_source(&self) -> Option<&Node>;
	fn get_operation(&self) -> Option<Operation>;
	fn get_as_source(&self) -> Option<&Node>;
	fn error(&self, reason: impl fmt::Debug+Send+Sync+'static) -> Error {
		self.make_error(Reason::External(Box::new(reason)), Operation::External)
	}
	fn make_error(&self, reason: Reason, operation: Operation) -> Error {
		let mut operations = self.collect_operations();
		operations.push(operation);
		Error {
			reason,
			operations,
		}
	}
	fn collect_operations(&self) -> Vec<Operation> {
		let mut ops = self.get_source().map_or(Vec::new(), Context::collect_operations);
		if let Some(op) = self.get_operation() {
			ops.push(op);
		}
		ops
	}
}

#[derive(Debug)]
pub enum Reason {
	NotFound,
	MultipleFound,
	External(Box<dyn fmt::Debug+Send+Sync>),
}
#[derive(Clone, Debug)]
pub enum Operation {
	Find { selector: &'static str },
	FindAll { selector: &'static str, index: usize },
	FindFirst { selector: &'static str },
	Text,
	Parse { r#type: type_id::Type },
	External,
}

pub struct Document {
	tree: scraper::Html,
	selector_cache: arena_cache::ArenaCache<&'static str, Selector>,
}
pub struct Node<'a> {
	document: &'a Document,
	source: Option<&'a Node<'a>>,
	operation: Operation,
	element: scraper::ElementRef<'a>,
}
pub struct Collection<'a> {
	document: &'a Document,
	source: Option<&'a Node<'a>>,
	selector: &'static str,
	iterator: scraper::element_ref::Select<'a, 'a>,
	index: usize,
}
pub struct Text<'a> {
	document: &'a Document,
	source: &'a Node<'a>,
	operation: Operation,
	value: String,
}

impl Document {
	pub fn from_str(html: &str) -> Document {
		Document {
			tree: scraper::Html::parse_document(html),
			selector_cache: arena_cache::ArenaCache::new(),
		}
	}
	pub fn from_read(mut read: impl io::Read) -> io::Result<Document> {
		let mut buf = String::new();
		read.read_to_string(&mut buf)?;
		Ok(Document::from_str(&buf))
	}
	fn compile_selector(&self, selector: &'static str) -> &Selector {
		self.selector_cache.query(selector, |selector| scraper::Selector::parse(selector).unwrap())
	}
}
impl Context for Document {
	fn get_document(&self) -> &Document {
		self
	}
	fn get_source(&self) -> Option<&Node> {
		None
	}
	fn get_operation(&self) -> Option<Operation> {
		None
	}
	fn get_as_source(&self) -> Option<&Node> {
		None
	}
}
impl Find for Document {
	fn find_all(&self, selector: &'static str) -> Collection {
		Collection {
			document: self,
			source: None,
			selector,
			iterator: self.tree.root_element().select(self.compile_selector(selector)),
			index: 0,
		}
	}
}

impl<'a> Node<'a> {
	pub fn text(&self) -> Text {
		let mut value = String::new();
		for chunk in self.element.text() {
			value += chunk;
		}
		Text {
			document: self.document,
			source: self,
			operation: Operation::Text,
			value: value.trim().to_owned(),
		}
	}
}
impl<'a> Context for Node<'a> {
	fn get_document(&self) -> &Document {
		self.document
	}
	fn get_source(&self) -> Option<&Node> {
		self.source
	}
	fn get_operation(&self) -> Option<Operation> {
		Some(self.operation.clone())
	}
	fn get_as_source(&self) -> Option<&Node> {
		Some(self)
	}
}
impl<'a> Find for Node<'a> {
	fn find_all(&self, selector: &'static str) -> Collection {
		Collection {
			document: self.document,
			source: Some(self),
			selector,
			iterator: self.element.select(self.document.compile_selector(selector)),
			index: 0
		}
	}
}
impl<'a> fmt::Debug for Node<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", &self.element.html())
	}
}

impl<'a> Iterator for Collection<'a> {
	type Item = Node<'a>;
	fn next(&mut self) -> Option<Node<'a>> {
		self.iterator.next().map(|element| {
			let node = Node {
				document: self.document,
				operation: Operation::FindAll { selector: self.selector, index: self.index },
				source: self.source,
				element,
			};
			self.index += 1;
			node
		})
	}
}

impl<'a> Text<'a> {
	pub fn string(&self) -> String {
		self.value.clone()
	}
	pub fn parse<T>(&self) -> Result<T> where T: FromStr+typename::TypeName+'static, <T as FromStr>::Err: fmt::Debug+Send+Sync+'static {
		self.value
			.parse()
			.map_err(|inner| {
				self.make_error(Reason::External(Box::new(inner)), Operation::Parse { r#type: type_id::Type::of::<T>() })
			})
	}
	pub fn map<T, E: fmt::Debug+Send+Sync+'static>(&self, f: impl FnOnce(&str) -> std::result::Result<T, E>) -> Result<T> {
		f(&self.value)
			.map_err(|inner| self.make_error(Reason::External(Box::new(inner)), Operation::External))
	}
}
impl<'a> Context for Text<'a> {
	fn get_document(&self) -> &Document {
		self.document
	}
	fn get_source(&self) -> Option<&Node> {
		Some(self.source)
	}
	fn get_operation(&self) -> Option<Operation> {
		Some(self.operation.clone())
	}
	fn get_as_source(&self) -> Option<&Node> {
		None
	}
}

impl fmt::Debug for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		writeln!(f, "debris::Error {{")?;
		writeln!(f, "    reason: {:?},", self.reason)?;
		if !self.operations.is_empty() {
			writeln!(f, "    operations: [")?;
			for op in &self.operations {
				writeln!(f, "        {:?},", op)?;
			}
			writeln!(f, "    ],")?;
		} else {
			writeln!(f, "    operations: [],")?;
		}
		write!(f, "}}")?;
		Ok(())
	}
}