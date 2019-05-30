mod arena_cache;
pub mod error;

use crate::error::Operation;
use scraper::Selector;
use std::{fmt, io, str::FromStr};

pub use error::{Error, R};

pub struct Document {
	tree: scraper::Html,
	selector_cache: arena_cache::ArenaCache<&'static str, Selector>,
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

	pub fn find_all(&self, selector: &'static str) -> Collection {
		Collection {
			document: &self,
			source: None,
			selector,
			iterator: self.tree.root_element().select(self.compile_selector(selector)),
			index: 0,
		}
	}

	pub fn root(&self) -> Node {
		Node {
			document: &self,
			source: None,
			operation: Some(Operation::Root),
			element: self.tree.root_element(),
		}
	}

	fn compile_selector(&self, selector: &'static str) -> &Selector {
		self.selector_cache.query(selector, |selector| scraper::Selector::parse(selector).unwrap())
	}
}

pub struct Node<'a> {
	document: &'a Document,
	source: Option<&'a Node<'a>>,
	operation: Option<Operation>,
	element: scraper::ElementRef<'a>,
}
impl<'a> Node<'a> {
	pub fn text(&self) -> Text {
		let mut value = String::new();
		for chunk in self.element.text() {
			value += chunk;
		}
		Text {
			source: self,
			operation: Operation::Text,
			value,
		}
	}

	pub fn text_with_brs(&self) -> Text {
		let mut value = String::new();
		for node in self.element.descendants() {
			if let Some(text) = node.value().as_text() {
				value += text;
			}
		}
		Text {
			source: self,
			operation: Operation::TextWithBrs,
			value,
		}
	}

	pub fn find_all(&self, selector: &'static str) -> Collection {
		Collection {
			document: self.document,
			source: Some(&self),
			selector,
			iterator: self.element.select((self.document).compile_selector(selector)),
			index: 0,
		}
	}

	pub fn find(&self, selector: &'static str) -> R<Node> {
		let mut hits = self.find_all(selector);
		let hit = hits.next();
		let is_only = hits.next().is_none();
		match (hit, is_only) {
			(Some(hit), true) => Ok(hit),
			(Some(_), false) => Err(error::Error::new(error::Reason::MultipleFound, &[Operation::Find { selector }], self)),
			(None, _) => Err(error::Error::new(error::Reason::NotFound, &[Operation::Find { selector }], self)),
		}
	}
}
impl<'a> fmt::Debug for Node<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.element.html())
	}
}

pub struct Collection<'a> {
	document: &'a Document,
	source: Option<&'a Node<'a>>,
	selector: &'static str,
	iterator: scraper::element_ref::Select<'a, 'a>,
	index: usize,
}
impl<'a> Iterator for Collection<'a> {
	type Item = Node<'a>;

	fn next(&mut self) -> Option<Node<'a>> {
		self.iterator.next().map(|element| {
			let r = Node {
				document: self.document,
				source: self.source,
				operation: Some(Operation::FindAll {
					selector: self.selector,
					index: self.index,
				}),
				element,
			};
			self.index += 1;
			r
		})
	}
}

pub struct Text<'a> {
	source: &'a Node<'a>,
	value: String,
	operation: Operation,
}
impl<'a> Text<'a> {
	pub fn parse_trimmed<T>(&self) -> R<T>
	where
		T: FromStr,
		<T as std::str::FromStr>::Err: std::error::Error+Send+Sync+'static,
	{
		self.value
			.trim()
			.parse()
			.map_err(|e| error::Error::new(error::Reason::External(Box::new(e)), &[Operation::Parse, self.operation.clone()], self.source))
	}

	pub fn string(self) -> String {
		self.value
	}

	pub fn error(&self, message: impl AsRef<str>) -> error::Error {
		error::Error::new(error::Reason::Logic(message.as_ref().to_owned()), &[Operation::External], self.source)
	}
}
impl<'a> fmt::Display for Text<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.write_str(&self.value)
	}
}
