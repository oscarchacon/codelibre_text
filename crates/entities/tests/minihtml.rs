//! Tests for `entities::minihtml`.

use entities::minihtml::{decode_entities, parse, plain_text, Span};

fn text(s: &str) -> Span {
    Span { text: s.into(), ..Default::default() }
}

#[test]
fn styles_nest_and_whitespace_collapses() {
    let lines = parse("<b>bold <i>both</i></b>   and\n  <code>x()</code>");
    assert_eq!(
        lines,
        vec![vec![
            // A space between words goes with the next one.
            Span { text: "bold".into(), bold: true, ..Default::default() },
            Span { text: " both".into(), bold: true, italic: true, ..Default::default() },
            text(" and"),
            Span { text: " x()".into(), monospace: true, ..Default::default() },
        ]]
    );
}

#[test]
fn blocks_and_breaks_make_lines() {
    let html = "<div><h1>Title</h1><p>one<br>two</p><ul><li>a</li><li>b</li></ul></div>";
    assert_eq!(plain_text(html), "Title\none\ntwo\n• a\n• b");
    assert!(parse(html)[0][0].heading && parse(html)[0][0].bold);
}

#[test]
fn links_colors_and_skipped_contents() {
    let html = r##"<body id="x"><style>a { color: red; }</style>
        <a href="subl:open {&quot;x&quot;: 1}">open</a>
        <span style="color: #f00; background-color: var(--bg); font-weight: bold">red</span>
        <font color='blue'>blue</font><script>ignored()</script></body>"##;
    let lines = parse(html);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let spans = &lines[0];
    assert_eq!(spans[0].text, "open");
    assert_eq!(spans[0].link.as_deref(), Some(r#"subl:open {"x": 1}"#));
    assert!(spans[0].underline);
    let red = spans.iter().find(|s| s.text.trim() == "red").unwrap_or_else(|| panic!("{spans:?}"));
    assert_eq!((red.color.as_deref(), red.background.as_deref(), red.bold), (Some("#f00"), Some("var(--bg)"), true));
    assert_eq!(spans.iter().find(|s| s.text.trim() == "blue").unwrap().color.as_deref(), Some("blue"));
    assert_eq!(plain_text(html), "open red blue");
    assert!(!plain_text(html).contains("ignored") && !plain_text(html).contains("color: red"));
}

#[test]
fn preformatted_text_keeps_its_lines_and_spaces() {
    assert_eq!(plain_text("<pre>a  b\n  c</pre>after"), "a  b\n  c\nafter");
}

#[test]
fn entities_are_decoded() {
    assert_eq!(decode_entities("a &amp; b &lt;&gt; &#65;&#x42; &unknown; &"), "a & b <> AB &unknown; &");
    assert_eq!(plain_text("x&nbsp;&nbsp;y é<!-- comment -->"), "x  y é");
}
