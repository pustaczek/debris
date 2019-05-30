use crate::Node;
use std::fmt;

#[derive(Clone, Debug)]
pub enum Operation {
	Root,
	Find { selector: &'static str },
	FindAll { selector: &'static str, index: usize },
	Text,
	TextWithBrs,
	Parse,
	External,
}

#[derive(Debug)]
pub enum Reason {
	NotFound,
	MultipleFound,
	External(Box<dyn fmt::Debug+Send+Sync>),
	Logic(String),
}

pub struct Error {
	reason: Reason,
	operations: Vec<Operation>,
	snapshots: Vec<String>,
	backtrace: backtrace::Backtrace,
}

impl Error {
	pub fn new(reason: Reason, last_operations: &[Operation], node: &Node) -> Error {
		let mut operations = Vec::new();
		let mut snapshots = Vec::new();
		operations.extend(last_operations.iter().map(Operation::clone));
		operations.reverse();
		for node in std::iter::successors(Some(node), |node| node.source) {
			if let Some(operation) = &node.operation {
				operations.push(operation.clone());
			}
			snapshots.push(node.element.html());
		}
		snapshots.push(node.document.tree.root_element().html());
		operations.reverse();
		snapshots.reverse();
		Error {
			reason,
			operations,
			snapshots,
			backtrace: backtrace::Backtrace::new(),
		}
	}

	pub fn reason(&self) -> &Reason {
		&self.reason
	}

	pub fn operations(&self) -> &[Operation] {
		&self.operations
	}

	pub fn snapshots(&self) -> &[String] {
		&self.snapshots
	}

	pub fn backtrace(&self) -> &backtrace::Backtrace {
		&self.backtrace
	}
}

impl fmt::Debug for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(
			f,
			"debris::Error {{ reason: {:?}, operations: {:?}, snapshots: ..., backtrace: ... }}",
			self.reason, self.operations
		)
	}
}

pub type R<T> = Result<T, Error>;
