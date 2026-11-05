//! Tests for window layouts (`entities::layout`).

use entities::layout::Layout;
use serde_json::json;

#[test]
fn the_menus_layouts() {
    assert_eq!(Layout::default().group_count(), 1);
    let columns = Layout::columns(3);
    assert_eq!(columns.group_count(), 3);
    assert_eq!(columns.cells[2], [2, 0, 3, 1]);
    let rect = columns.cell_rect(1).unwrap();
    assert!((rect[0] - 1.0 / 3.0).abs() < 1e-9 && (rect[2] - 2.0 / 3.0).abs() < 1e-9);
    assert_eq!(Layout::rows(2).cell_rect(1), Some([0.0, 0.5, 1.0, 1.0]));
    assert_eq!(Layout::grid().cell_rect(3), Some([0.5, 0.5, 1.0, 1.0]));
    assert_eq!(Layout::grid().cell_rect(4), None);
}

#[test]
fn layouts_are_read_and_written_as_sublime_writes_them() {
    let value = json!({ "cols": [0.0, 0.5, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 1, 1], [1, 0, 2, 1]] });
    let layout = Layout::from_json(&value).unwrap();
    assert_eq!(layout, Layout::columns(2));
    assert_eq!(layout.to_json(), value);
}

#[test]
fn what_is_not_a_layout() {
    let not = |value| Layout::from_json(&value).is_none();
    assert!(not(json!({})));
    assert!(not(json!({ "cols": [0.0, 1.0], "rows": [0.0, 1.0], "cells": [] })), "no cells");
    assert!(not(json!({ "cols": [0.0, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 2, 1]] })), "a cell past the columns");
    assert!(not(json!({ "cols": [0.0, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 0, 1]] })), "an empty cell");
    assert!(not(json!({ "cols": [0.0, 0.7, 0.5, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 1, 1]] })), "columns out of order");
    assert!(not(json!({ "cols": [0.0, 1.0], "rows": [0.0, 1.0], "cells": [[0, 0, 1]] })), "a cell of three numbers");
}

#[test]
fn neighboring_groups_wrap_around() {
    let layout = Layout::columns(3);
    assert_eq!(layout.neighbor(2, true), 0);
    assert_eq!(layout.neighbor(0, false), 2);
    assert_eq!(layout.neighbor(1, true), 2);
}
