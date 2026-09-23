use super::*;

#[test]
fn empty_and_whitespace_documents_are_rejected() {
    assert!(parse_full_document("").is_err());
    assert!(parse_full_document(" \n\t").is_err());
}

#[test]
fn second_root_and_trailing_junk_are_rejected() {
    assert!(parse_full_document("<a/><b/>").is_err());
    assert!(parse_full_document("<a/>junk").is_err());
}

#[test]
fn malformed_attributes_are_rejected_but_valid_attributes_are_kept() {
    assert!(parse_full_document(r#"<a bad="x></a>"#).is_err());
    let root = parse_full_document(r#"<a answer="1" note="a&amp;b"/>"#).unwrap();
    let node = &root.children[0];
    assert_eq!(node.attr("answer"), Some("1"));
    assert_eq!(node.attr("note"), Some("a&b"));
}

#[test]
fn text_and_cdata_are_preserved() {
    let root = parse_full_document("<a>text<![CDATA[<raw>]]>tail</a>").unwrap();
    assert_eq!(root.children[0].text, "text<raw>tail");
}

#[test]
fn comments_do_not_change_document_contents() {
    let root =
        parse_full_document("<!-- before --><a><!-- inside -->ok</a><!-- after -->").unwrap();
    assert_eq!(root.children[0].text, "ok");
}

#[test]
fn malformed_comments_are_rejected() {
    assert!(parse_full_document("<a><!-- invalid -- comment --></a>").is_err());
}
