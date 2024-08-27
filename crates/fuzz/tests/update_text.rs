use loro::LoroDoc;

#[test]
fn update_text() {
    let doc = LoroDoc::new();
    let text = doc.get_text("text");
    text.update("ϼCCC");
    text.update("2");
    assert_eq!(&text.to_string(), "2");
}