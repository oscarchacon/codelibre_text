"""Columns with tabs expanded, as ``Default.indentation`` gives them (Alignment imports
``line_and_normed_pt``)."""

import sublime


def _tab_size(view):
    try:
        return max(1, int(view.settings().get("tab_size", 4)))
    except (TypeError, ValueError):
        return 4


def normed_column(view, text):
    """The column ``text`` (the start of a line) ends at, each tab reaching the next tab stop."""
    tab_size = _tab_size(view)
    column = 0
    for char in text:
        column = column + tab_size - column % tab_size if char == "\t" else column + 1
    return column


def line_and_normed_pt(view, pt):
    """``(row, column)`` of ``pt``, the column counting tabs as up to ``tab_size`` spaces."""
    row, _ = view.rowcol(pt)
    start = view.text_point(row, 0)
    return row, normed_column(view, view.substr(sublime.Region(start, pt)))


def pt_from_line_and_normed_pt(view, line_and_pt):
    """The point at that row and expanded column (the end of the line when it is shorter)."""
    row, wanted = line_and_pt
    pt = view.text_point(row, 0)
    end = view.line(pt).end()
    tab_size = _tab_size(view)
    column = 0
    while pt < end and column < wanted:
        char = view.substr(pt)
        column = column + tab_size - column % tab_size if char == "\t" else column + 1
        pt += 1
    return pt


def normed_indentation_pt(view, sel, non_space=False):
    """The expanded width of the indentation (spaces and tabs) of the line ``sel`` begins on.
    ``non_space`` is taken for Sublime's signature; it counts the same characters."""
    text = view.substr(view.line(sel.begin()))
    return normed_column(view, text[: len(text) - len(text.lstrip(" \t"))])
