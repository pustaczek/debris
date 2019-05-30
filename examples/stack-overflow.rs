const HTML: &'static str = include_str!("./stack-overflow.html");

fn main() -> debris::R<()> {
	let doc = debris::Document::from_str(HTML);
	for v in doc.find_all(".question-summary") {
		let votes: i64 = v.find(".votes > .mini-counts > span")?.text().parse_trimmed()?;
		let title = v.find(".summary .question-hyperlink")?.text().string();
		println!("[{:+}] {}", votes, title);
	}
	Ok(())
}
