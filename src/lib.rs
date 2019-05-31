use std::fmt;
use scraper::{Selector, ElementRef};
use std::str::FromStr;

mod arena_cache;
mod type_id;

pub struct Error {
	reason: Reason,
	operations: Vec<Operation>,
	pub snapshots: Vec<String>,
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
			snapshots: self.collect_snapshots(),
		}
	}
	fn collect_operations(&self) -> Vec<Operation> {
		let mut ops = self.get_source().map_or(Vec::new(), Context::collect_operations);
		if let Some(op) = self.get_operation() {
			ops.push(op);
		}
		ops
	}
	fn collect_snapshots(&self) -> Vec<String> {
		let mut sss = self.get_source().map_or_else(|| vec![self.get_document().tree.root_element().html()], Context::collect_snapshots);
		if let Some(v) = self.get_as_source() {
			sss.push(v.element.html());
		}
		sss
	}
}

#[derive(Debug)]
pub enum Reason {
	NotFound,
	MultipleFound,
	ExpectedElement,
	ExpectedText,
	External(Box<dyn fmt::Debug+Send+Sync>),
}
#[derive(Clone, Debug)]
pub enum Operation {
	Find { selector: &'static str },
	FindAll { selector: &'static str, index: usize },
	FindFirst { selector: &'static str },
	Child { index: usize },
	ChildText { index: usize },
	Text,
	TextBr,
	Attr { key: &'static str },
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
	pub fn new(html: &str) -> Document {
		Document {
			tree: scraper::Html::parse_document(html),
			selector_cache: arena_cache::ArenaCache::new(),
		}
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
	pub fn child(&self, index: usize) -> Result<Node> {
		match self.element.children().nth(index) {
			Some(node) => Ok(Node {
				document: self.document,
				source: Some(self),
				operation: Operation::Child { index },
				element: ElementRef::wrap(node).ok_or_else(|| self.make_error(Reason::ExpectedElement, Operation::Child { index }))?,
			}),
			None => Err(self.make_error(Reason::NotFound, Operation::Child { index })),
		}
	}
	pub fn text_child(&self, index: usize) -> Result<Text> {
		match self.element.children().nth(index) {
			Some(node) => Ok(Text {
				document: self.document,
				source: self,
				operation: Operation::ChildText { index },
				value: node.value().as_text().ok_or_else(|| self.make_error(Reason::ExpectedText, Operation::ChildText { index }))?.trim().to_owned(),
			}),
			None => Err(self.make_error(Reason::NotFound, Operation::Child { index })),
		}
	}
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
	pub fn text_br(&self) -> Text {
		let mut value = String::new();
		for v in self.element.descendants() {
			match v.value() {
				scraper::node::Node::Text(text) => value += &*text,
				scraper::node::Node::Element(element) if element.name() == "br" => value += "\n",
				_ => (),
			}
		}
		Text {
			document: self.document,
			source: self,
			operation: Operation::TextBr,
			value: value.trim().to_owned(),
		}
	}
	pub fn attr(&self, key: &'static str) -> Result<Text> {
		let value = self.element.value().attr(key).ok_or_else(|| self.make_error(Reason::NotFound, Operation::Attr { key }))?;
		Ok(Text {
			document: self.document,
			source: self,
			operation: Operation::Attr { key },
			value: value.to_owned(),
		})
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
	pub fn as_str(&self) -> &str {
		&self.value
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
impl<'a> PartialEq<&str> for Text<'a> {
	fn eq(&self, other: &&str) -> bool {
		self.as_str() == *other
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
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "encountered unexpected HTML structure ({:?} in {:?})", self.reason, self.operations)
	}
}
impl std::error::Error for Error {}
