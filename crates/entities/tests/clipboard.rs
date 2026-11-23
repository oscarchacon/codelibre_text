use entities::clipboard::*;
use entities::selection::Selection;
use entities::text_commands::replace_selections_each;

fn carets(points: &[usize]) -> Vec<Selection> {
    points.iter().map(|&p| Selection::caret(p)).collect()
}

#[test]
fn copying_joins_the_selected_texts_with_newlines_and_leaves_carets_out() {
    let text = "one two three";
    let selections = [
        Selection::new(0, 3),
        Selection::caret(5),
        Selection::new(8, 13),
    ];
    assert_eq!(
        copied(text, &selections, true),
        Some(Copied {
            text: "one\nthree".into(),
            whole_lines: false
        })
    );
}

#[test]
fn copying_carets_only_takes_each_line_once_with_its_newline() {
    let text = "ab\ncd\nef";
    assert_eq!(
        copied(text, &carets(&[0, 1, 7]), true),
        Some(Copied {
            text: "ab\nef\n".into(),
            whole_lines: true
        })
    );
    assert_eq!(copied(text, &carets(&[0, 7]), false), None);
}

#[test]
fn cutting_whole_lines_deletes_each_once_and_the_last_one_s_newline_before_it() {
    let mut text = String::from("ab\ncd\nef");
    let after = delete_full_lines(&mut text, &carets(&[1, 2, 7]));
    assert_eq!(text, "cd");
    assert_eq!(after.len(), 3);
    assert!(after.iter().all(|s| s.head <= 2 && s.is_empty()));

    let mut text = String::from("ab\ncd\nef");
    let after = delete_full_lines(&mut text, &carets(&[4]));
    assert_eq!((text.as_str(), after), ("ab\nef", carets(&[3])));
}

#[test]
fn pasting_whole_lines_puts_them_above_the_carets_lines_which_stay() {
    let mut text = String::from("ab\ncd");
    let pieces = line_pieces("1\n2\n", 2);
    assert_eq!(pieces, ["1\n", "2\n"]);
    let after = insert_above_lines(&mut text, &carets(&[1, 4]), &pieces);
    assert_eq!(text, "1\nab\n2\ncd");
    assert_eq!(after, carets(&[3, 8]));

    let mut text = String::from("ab\ncd");
    let pieces = line_pieces("x\n", 2);
    insert_above_lines(&mut text, &carets(&[1, 4]), &pieces);
    assert_eq!(text, "x\nab\nx\ncd");
}

#[test]
fn a_paste_shares_its_lines_out_only_when_they_match_the_selections() {
    assert_eq!(paste_pieces("a\nb\nc", 3), ["a", "b", "c"]);
    assert_eq!(paste_pieces("a\r\nb", 2), ["a", "b"]);
    assert_eq!(paste_pieces("a\nb", 3), ["a\nb"; 3]);
    assert_eq!(paste_pieces("a\nb", 1), ["a\nb"]);

    let mut text = String::from("x y z");
    let after = replace_selections_each(
        &mut text,
        &[
            Selection::new(0, 1),
            Selection::caret(2),
            Selection::new(4, 5),
        ],
        &paste_pieces("one\ntwo\nthree", 3),
    );
    assert_eq!(text, "one twoy three");
    assert_eq!(after, carets(&[3, 7, 14]));
}
