use wasm_backtrace::Backtrace;
use scraper::{ElementRef, Selector};
use std::{fmt, str::FromStr};

mod arena_cache;

#[derive(Debug)]
pub struct Error {
	pub reason: Reason,
	pub operations: Vec<Operation>,
	pub snapshots: Vec<String>,
	pub backtrace: Backtrace,
}
pub type Result<T> = std::result::Result<T, Error>;

pub trait Find: Context {
	fn find_all(&self, selector: &'static str) -> Collection;
	fn find(&self, selector: &'static str) -> Result<Node> {
		let mut iter = self.find_all(selector).iterator;
		let element = iter.next();
		let is_only = iter.next().is_none();
		match element {
			Some(element) if is_only => {
				Ok(Node { document: self.get_document(), source: self.get_as_source(), operation: Operation::Find { selector }, element })
			},
			Some(_) => Err(self.make_error(Reason::MultipleFound, Operation::Find { selector })),
			None => Err(self.make_error(Reason::NotFound, Operation::Find { selector })),
		}
	}
	fn find_first(&self, selector: &'static str) -> Result<Node> {
		match self.find_all(selector).iterator.next() {
			Some(element) => {
				Ok(Node { document: self.get_document(), source: self.get_as_source(), operation: Operation::FindFirst { selector }, element })
			},
			None => Err(self.make_error(Reason::NotFound, Operation::FindFirst { selector })),
		}
	}
	fn find_nth(&self, selector: &'static str, index: usize) -> Result<Node> {
		match self.find_all(selector).iterator.nth(index) {
			Some(element) => {
				Ok(Node { document: self.get_document(), source: self.get_as_source(), operation: Operation::FindNth { selector, index }, element })
			},
			None => Err(self.make_error(Reason::NotFound, Operation::FindNth { selector, index })),
		}
	}
}
pub trait Context {
	fn get_document(&self) -> &Document;
	fn get_source(&self) -> Option<&Node>;
	fn get_operation(&self) -> Option<Operation>;
	fn get_as_source(&self) -> Option<&Node>;
	fn error(&self, reason: impl fmt::Debug+fmt::Display+Send+Sync+'static) -> Error {
		self.make_error(Reason::External(Box::new(reason)), Operation::External)
	}
	fn make_error(&self, reason: Reason, operation: Operation) -> Error {
		let mut operations = self.collect_operations();
		operations.push(operation);
		Error { reason, operations, snapshots: self.collect_snapshots(), backtrace: Backtrace::new() }
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

pub trait DebugDisplay: fmt::Debug+fmt::Display {}
impl<T: fmt::Debug+fmt::Display> DebugDisplay for T {
}

#[derive(Debug)]
pub enum Reason {
	NotFound,
	MultipleFound,
	ExpectedElement,
	ExpectedText,
	External(Box<dyn DebugDisplay+Send+Sync>),
}
#[derive(Clone, Debug)]
pub enum Operation {
	Find { selector: &'static str },
	FindAll { selector: &'static str, index: usize },
	FindFirst { selector: &'static str },
	FindNth { selector: &'static str, index: usize },
	Child { index: usize },
	ChildText { index: usize },
	Parent,
	Text,
	TextMultiline,
	Attr { key: &'static str },
	Parse,
	External,
}

pub struct Document {
	pub tree: scraper::Html,
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
		Document { tree: scraper::Html::parse_document(html), selector_cache: arena_cache::ArenaCache::new() }
	}

	pub fn html(&self) -> String {
		self.tree.root_element().html()
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
		Collection { document: self, source: None, selector, iterator: self.tree.root_element().select(self.compile_selector(selector)), index: 0 }
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

	pub fn parent(&self) -> Result<Node> {
		match self.element.parent() {
			Some(node) => Ok(Node {
				document: self.document,
				source: Some(self),
				operation: Operation::Parent,
				element: ElementRef::wrap(node).ok_or_else(|| self.make_error(Reason::ExpectedElement, Operation::Parent))?,
			}),
			None => Err(self.make_error(Reason::NotFound, Operation::Parent)),
		}
	}

	pub fn text(&self) -> Text {
		let mut value = String::new();
		for chunk in self.element.text() {
			value += chunk;
		}
		Text { document: self.document, source: self, operation: Operation::Text, value: value.trim().to_owned() }
	}

	pub fn text_multiline(&self) -> Text {
		let mut value = String::new();
		for v in self.element.descendants() {
			match v.value() {
				scraper::node::Node::Text(text) => value += &*text,
				scraper::node::Node::Element(element) if element.name() == "br" => value += "\n",
				_ => (),
			}
		}
		Text { document: self.document, source: self, operation: Operation::TextMultiline, value: value.trim().to_owned() }
	}

	pub fn attr(&self, key: &'static str) -> Result<Text> {
		let value = self.element.value().attr(key).ok_or_else(|| self.make_error(Reason::NotFound, Operation::Attr { key }))?;
		Ok(Text { document: self.document, source: self, operation: Operation::Attr { key }, value: value.to_owned() })
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
			index: 0,
		}
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

	pub fn parse<T>(&self) -> Result<T>
	where
		T: FromStr+'static,
		<T as FromStr>::Err: fmt::Debug+fmt::Display+Send+Sync+'static,
	{
		self.value.parse().map_err(|inner| self.make_error(Reason::External(Box::new(inner)), Operation::Parse))
	}

	pub fn map<T, E: fmt::Debug+fmt::Display+Send+Sync+'static>(&self, f: impl FnOnce(&str) -> std::result::Result<T, E>) -> Result<T> {
		f(&self.value).map_err(|inner| self.make_error(Reason::External(Box::new(inner)), Operation::External))
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

impl fmt::Debug for Document {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.tree.root_element().html())
	}
}
impl fmt::Debug for Node<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.element.html())
	}
}
impl fmt::Debug for Text<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self.as_str())
	}
}

fn fmt_multiple(n: usize) -> String {
	match n {
		1 => "1st".to_owned(),
		2 => "2nd".to_owned(),
		3 => "3rd".to_owned(),
		_ => format!("{}th", n),
	}
}

impl fmt::Display for Reason {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Reason::NotFound => write!(f, "not found"),
			Reason::MultipleFound => write!(f, "found too many"),
			Reason::ExpectedElement => write!(f, "expected element"),
			Reason::ExpectedText => write!(f, "expected text"),
			Reason::External(inner) => fmt::Display::fmt(&**inner, f),
		}
	}
}

impl fmt::Display for Operation {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Operation::Find { selector } => write!(f, "'{}'", selector),
			Operation::FindAll { selector, index } => write!(f, "{} of '{}'", fmt_multiple(*index), selector),
			Operation::FindFirst { selector } => write!(f, "first '{}'", selector),
			Operation::FindNth { selector, index } => write!(f, "{} '{}'", fmt_multiple(*index), selector),
			Operation::Child { index } => write!(f, "{} child", fmt_multiple(*index)),
			Operation::ChildText { index } => write!(f, "{} child text", fmt_multiple(*index)),
			Operation::Parent => write!(f, "parent"),
			Operation::Text => write!(f, "text"),
			Operation::TextMultiline => write!(f, "multiline text"),
			Operation::Attr { key } => write!(f, "attr '{}'", key),
			Operation::Parse => write!(f, "parse"),
			Operation::External => write!(f, "external"),
		}
	}
}
impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{} {}", self.reason, self.operations.iter().rev().map(Operation::to_string).collect::<Vec<_>>().join(" "))
	}
}
impl std::error::Error for Error {
}
