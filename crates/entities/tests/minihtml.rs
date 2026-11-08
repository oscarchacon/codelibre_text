//! Tests for `entities::minihtml`.

use entities::minihtml::{data_uri_bytes, decode_entities, parse, parse_html, plain_text, Block, Content, FontSize, Sides, Span};

#[test]
fn style_rules_apply_by_class_id_and_tag_most_specific_last() {
    let html = r#"<style>
        /* a comment */
        span { color: red; }
        .note { color: green; font-weight: bold; }
        #only { color: blue; }
        div .inside { font-style: italic; }
        a:hover { color: pink; }
    </style>
    <span>a</span> <span class="note">b</span> <span class="note" id="only">c</span>
    <div><span class="inside">d</span></div> <span class="inside">e</span>"#;
    let spans: Vec<Span> = parse(html).into_iter().flatten().collect();
    let find = |t: &str| spans.iter().find(|s| s.text.trim() == t).unwrap_or_else(|| panic!("{t} in {spans:?}")).clone();
    assert_eq!(find("a").color.as_deref(), Some("red"));
    assert_eq!((find("b").color.as_deref(), find("b").bold), (Some("green"), true));
    assert_eq!(find("c").color.as_deref(), Some("blue"), "an id is more specific than a class");
    assert!(find("d").italic && !find("e").italic, "descendant selector");
}

#[test]
fn custom_properties_font_sizes_and_display_none() {
    let html = r#"<style>html { --warn: #ff8800; } .big { font-size: 1.5em; } .px { font-size: 20px; }</style>
        <span style="color: var(--warn)">w</span><span style="color: var(--redish)">r</span>
        <span class="big">x<span style="font-size: 50%">y</span></span><span class="px">z</span>
        <h1>T</h1><span style="display: none">hidden</span>"#;
    let spans: Vec<Span> = parse(html).into_iter().flatten().collect();
    let find = |t: &str| spans.iter().find(|s| s.text.trim() == t).unwrap().clone();
    assert_eq!(find("w").color.as_deref(), Some("#ff8800"));
    assert_eq!(find("r").color.as_deref(), Some("var(--redish)"), "the color scheme's variables are the view's");
    assert_eq!(find("x").font_size, Some(FontSize::Em(1.5)));
    assert_eq!(find("y").font_size, Some(FontSize::Em(0.75)));
    assert_eq!(find("z").font_size, Some(FontSize::Px(20.0)));
    assert_eq!(find("T").font_size, Some(FontSize::Em(1.5)));
    assert!(!spans.iter().any(|s| s.text.contains("hidden")));
}

#[test]
fn blocks_keep_their_box_and_tables_their_cells() {
    let html = r#"<div style="padding: 4px 8px; margin-top: 2px; border: 1px solid #333; border-radius: 3px; background-color: #eee">
        boxed
        <table><tr><th>k</th><td>v</td></tr><tbody><tr><td>a</td><td>b</td></tr></tbody></table>
        <hr>
        <img src="data:image/png;base64,iVBORw0KGgo=" width="16" height=8 alt="pic">
    </div>"#;
    let document = parse_html(html);
    let Some(Content::Block(boxed)) = document.content.first() else { panic!("{document:?}") };
    assert_eq!(boxed.style.padding, Sides { top: 4.0, right: 8.0, bottom: 4.0, left: 8.0 });
    assert_eq!(boxed.style.margin.top, 2.0);
    assert_eq!((boxed.style.border_width, boxed.style.border_color.as_deref(), boxed.style.border_radius), (1.0, Some("#333"), 3.0));
    assert_eq!(boxed.style.background.as_deref(), Some("#eee"));
    let Some(Content::Table(rows)) = boxed.content.iter().find(|c| matches!(c, Content::Table(_))) else { panic!("{boxed:?}") };
    let cell_text = |cell: &Block| match cell.content.first() {
        Some(Content::Line(line)) => line.iter().map(|s| s.text.as_str()).collect::<String>(),
        _ => String::new(),
    };
    let texts: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(cell_text).collect()).collect();
    assert_eq!(texts, [["k", "v"], ["a", "b"]]);
    assert!(boxed.content.contains(&Content::Rule));
    let image = boxed.content.iter().find_map(|c| match c {
        Content::Line(line) => line.iter().find_map(|s| s.image.clone()),
        _ => None,
    });
    let image = image.expect("the image");
    assert_eq!((image.width, image.height), (Some(16.0), Some(8.0)));
    assert_eq!(data_uri_bytes(&image.src), Some(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    assert_eq!(plain_text("<table><tr><td>a</td><td>b</td></tr></table>"), "a  b");
}

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
