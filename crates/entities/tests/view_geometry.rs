//! Tests for the geometry of a view (`entities::view_geometry`).

use entities::view_geometry::ViewGeometry;

fn geometry() -> ViewGeometry {
    ViewGeometry {
        line_height: 20.0,
        em_width: 10.0,
        ..ViewGeometry::default()
    }
}

#[test]
fn points_and_layout_positions_go_both_ways() {
    let g = geometry();
    let text = "ab\n\tcd\n";
    assert_eq!(g.text_to_layout(text, 0), (0.0, 0.0));
    assert_eq!(g.text_to_layout(text, 2), (20.0, 0.0), "the end of a line");
    // A tab takes four columns.
    assert_eq!(g.text_to_layout(text, 4), (40.0, 20.0));
    assert_eq!(
        g.text_to_layout(text, 7),
        (0.0, 40.0),
        "the empty last line"
    );
    assert_eq!(g.layout_to_text(text, (41.0, 25.0)), 4);
    assert_eq!(
        g.layout_to_text(text, (16.0, 5.0)),
        2,
        "past a char's middle: the next"
    );
    assert_eq!(g.layout_to_text(text, (14.0, 5.0)), 1);
    assert_eq!(
        g.layout_to_text(text, (500.0, 5.0)),
        2,
        "past the end of the row"
    );
    assert_eq!(
        g.layout_to_text(text, (0.0, 900.0)),
        7,
        "below the text: its end"
    );
    assert_eq!(g.layout_extent(text), (60.0, 60.0));
}

#[test]
fn window_coordinates_follow_the_gutter_and_the_scroll() {
    let g = ViewGeometry {
        text_origin: (50.0, 0.0),
        viewport_position: (0.0, 100.0),
        ..geometry()
    };
    assert_eq!(g.window_to_layout((60.0, 10.0)), (10.0, 110.0));
    assert_eq!(g.layout_to_window((10.0, 110.0)), (60.0, 10.0));
}

#[test]
fn wrapped_lines_break_after_the_last_space_that_fits() {
    let g = ViewGeometry {
        wrap_columns: Some(6),
        ..geometry()
    };
    let text = "aaa bbb cccccccc";
    // Rows: "aaa " | "bbb " | "cccccc" | "cc".
    assert_eq!(g.text_to_layout(text, 4), (0.0, 20.0));
    assert_eq!(g.text_to_layout(text, 9), (10.0, 40.0));
    assert_eq!(g.text_to_layout(text, 15), (10.0, 60.0));
    assert_eq!(g.layout_to_text(text, (0.0, 65.0)), 14);
    assert_eq!(g.layout_extent(text), (60.0, 80.0));
}
