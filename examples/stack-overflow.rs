use debris::Find;

const HTML: &'static str = include_str!("./stack-overflow.html");

fn main() -> debris::Result<()> {
	let doc = debris::Document::from_str(HTML);
	for v in doc.find_all(".question-summary").take(5) {
		let votes: i64 = v.find(".votes span")?.text().parse()?;
		let title = v.find(".summary .question-hyperlink")?.text().string();
		println!("[{:+}] {}", votes, title);
	}
	Ok(())
}
