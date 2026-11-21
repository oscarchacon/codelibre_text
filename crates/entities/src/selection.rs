/// A selection inside a document, as char indices. `anchor == head` is a plain caret.
///
/// `anchor` stays put while extending a selection; `head` is the end that moves (where the caret is).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn caret(position: usize) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The lower end of the selection.
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }

    /// The upper end of the selection.
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn len(&self) -> usize {
        self.end() - self.start()
    }

    /// The same selection limited to a text of `len` chars.
    pub fn clamped(&self, len: usize) -> Self {
        Self::new(self.anchor.min(len), self.head.min(len))
    }
}

/// Whether Sublime's selection set makes `a` and `b` one region: two selections that overlap
/// (not those that only touch), or a caret at or inside the other (two carets: at one point).
fn merges_with(a: Selection, b: Selection) -> bool {
    match (a.is_empty(), b.is_empty()) {
        (false, false) => a.start() < b.end() && b.start() < a.end(),
        (true, _) => b.start() <= a.head && a.head <= b.end(),
        (false, true) => a.start() <= b.head && b.head <= a.end(),
    }
}

/// Adds `added` to `selections` as Sublime's `Selection.add` does, and returns where the
/// region now holding it is. The regions it overlaps become one with it, which keeps the
/// direction of `added` (of the selection it went into, for a caret); `selections` stays sorted.
/// Checked against Sublime 4215: `(0, 3)` then `(5, 2)` is `(5, 0)`; `(0, 2)` and `(2, 4)` stay
/// two; a caret at either end of a selection goes into it.
pub fn add_region(selections: &mut Vec<Selection>, added: Selection) -> usize {
    let mut region = added;
    let mut reversed = (!added.is_empty()).then_some(added.head < added.anchor);
    while let Some(index) = selections.iter().position(|&s| merges_with(s, region)) {
        let met = selections.remove(index);
        if reversed.is_none() && !met.is_empty() {
            reversed = Some(met.head < met.anchor);
        }
        let (start, end) = (region.start().min(met.start()), region.end().max(met.end()));
        region = if reversed == Some(true) {
            Selection::new(end, start)
        } else {
            Selection::new(start, end)
        };
    }
    let index =
        selections.partition_point(|s| (s.start(), s.end()) < (region.start(), region.end()));
    selections.insert(index, region);
    index
}

/// The selections as one sorted set without overlaps, `primary` added last (see [`add_region`]):
/// the region that holds `primary`, and the others.
pub fn merged(
    primary: Selection,
    others: impl IntoIterator<Item = Selection>,
) -> (Selection, Vec<Selection>) {
    let mut set = Vec::new();
    for other in others {
        add_region(&mut set, other);
    }
    let index = add_region(&mut set, primary);
    let primary = set.remove(index);
    (primary, set)
}

/// Takes `removed` out of `selections` as Sublime's `Selection.subtract` does (sorted, as given).
/// A selection loses the part `removed` covers: the one piece left keeps its direction, two go
/// forward. A caret goes when `removed` holds it (its ends included) or is at the same point; a
/// caret strictly inside a selection cuts it in two. Selections that only touch `removed` stay.
/// Checked against Sublime 4215; it may leave no selection at all.
pub fn subtract_region(selections: &[Selection], removed: Selection) -> Vec<Selection> {
    let (cut_start, cut_end) = (removed.start(), removed.end());
    let mut left = Vec::with_capacity(selections.len() + 1);
    for &s in selections {
        if s.is_empty() {
            if !(cut_start <= s.head && s.head <= cut_end) {
                left.push(s);
            }
            continue;
        }
        let (start, end) = (s.start(), s.end());
        if removed.is_empty() {
            if start < cut_start && cut_start < end {
                left.push(Selection::new(start, cut_start));
                left.push(Selection::new(cut_start, end));
            } else {
                left.push(s);
            }
            continue;
        }
        if end <= cut_start || cut_end <= start {
            left.push(s);
            continue;
        }
        let pieces: Vec<(usize, usize)> = [(start, cut_start), (cut_end, end)]
            .into_iter()
            .filter(|(a, b)| a < b)
            .collect();
        match pieces.as_slice() {
            [(a, b)] if s.head < s.anchor => left.push(Selection::new(*b, *a)),
            pieces => left.extend(pieces.iter().map(|&(a, b)| Selection::new(a, b))),
        }
    }
    left
}
