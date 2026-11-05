//! How a window is split into groups (Sublime's `set_layout`): column and row boundaries as
//! fractions of the window, and the cells, one per group, that span some of them.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Column boundaries from 0 to 1, in order (`[0, 0.5, 1]`: two columns).
    pub cols: Vec<f64>,
    /// Row boundaries from 0 to 1, in order.
    pub rows: Vec<f64>,
    /// One per group: `[x1, y1, x2, y2]`, indexes into `cols` and `rows`.
    pub cells: Vec<[usize; 4]>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::columns(1)
    }
}

impl Layout {
    /// `count` columns of the same width (View > Layout > Columns).
    pub fn columns(count: usize) -> Self {
        let count = count.max(1);
        Self {
            cols: even(count),
            rows: vec![0.0, 1.0],
            cells: (0..count).map(|i| [i, 0, i + 1, 1]).collect(),
        }
    }

    /// `count` rows of the same height.
    pub fn rows(count: usize) -> Self {
        let count = count.max(1);
        Self {
            cols: vec![0.0, 1.0],
            rows: even(count),
            cells: (0..count).map(|i| [0, i, 1, i + 1]).collect(),
        }
    }

    /// Two columns by two rows, groups row by row.
    pub fn grid() -> Self {
        Self {
            cols: even(2),
            rows: even(2),
            cells: vec![[0, 0, 1, 1], [1, 0, 2, 1], [0, 1, 1, 2], [1, 1, 2, 2]],
        }
    }

    pub fn group_count(&self) -> usize {
        self.cells.len()
    }

    /// `{"cols": […], "rows": […], "cells": [[x1, y1, x2, y2], …]}`; `None` when it is not a
    /// layout: boundaries that do not go from 0 to 1 in order, cells outside them or empty.
    pub fn from_json(value: &Value) -> Option<Self> {
        let numbers = |key: &str| -> Option<Vec<f64>> { value.get(key)?.as_array()?.iter().map(Value::as_f64).collect() };
        let cols = numbers("cols")?;
        let rows = numbers("rows")?;
        let cells = value
            .get("cells")?
            .as_array()?
            .iter()
            .map(|cell| {
                let parts: Vec<usize> = cell.as_array()?.iter().map(|n| n.as_u64().map(|n| n as usize)).collect::<Option<_>>()?;
                <[usize; 4]>::try_from(parts).ok()
            })
            .collect::<Option<Vec<_>>>()?;
        let layout = Self { cols, rows, cells };
        layout.is_valid().then_some(layout)
    }

    pub fn to_json(&self) -> Value {
        json!({ "cols": self.cols, "rows": self.rows, "cells": self.cells })
    }

    fn is_valid(&self) -> bool {
        let boundaries = |b: &[f64]| b.len() >= 2 && b.windows(2).all(|w| w[0] <= w[1]) && b.iter().all(|x| (0.0..=1.0).contains(x));
        boundaries(&self.cols)
            && boundaries(&self.rows)
            && !self.cells.is_empty()
            && self
                .cells
                .iter()
                .all(|&[x1, y1, x2, y2]| x1 < x2 && y1 < y2 && x2 < self.cols.len() && y2 < self.rows.len())
    }

    /// Where group `group` is, as fractions of the window: `[left, top, right, bottom]`.
    pub fn cell_rect(&self, group: usize) -> Option<[f64; 4]> {
        let &[x1, y1, x2, y2] = self.cells.get(group)?;
        Some([self.cols[x1], self.rows[y1], self.cols[x2], self.rows[y2]])
    }

    /// The group next to `group` in reading order (`forward`), wrapping around: Sublime's
    /// `focus_neighboring_group`.
    pub fn neighbor(&self, group: usize, forward: bool) -> usize {
        let count = self.group_count();
        if forward {
            (group + 1) % count
        } else {
            (group + count - 1) % count
        }
    }
}

fn even(count: usize) -> Vec<f64> {
    (0..=count).map(|i| i as f64 / count as f64).collect()
}
