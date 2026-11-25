//! Presses in the text that the mousemaps bind (see `mouse_controller`): the gestures that make
//! several selections (Ctrl + click, Alt + click, column drags…) and the commands bound to a
//! button (the side buttons switch tabs). The text widget only knows one selection and would move
//! its caret at the press, so while such a gesture goes on its state is put back after it is
//! shown (as `context_menu::keep_selection` does for a right click) and the gesture goes to the
//! use cases as `UiEvent::DragSelect`, every frame the pointer moves, until the button goes up.

use egui::text::CCursorRange;
use egui::text_edit::TextEditState;
use egui::{Id, PointerButton, Pos2};
use entities::command::Command;
use entities::document::DocumentId;
use entities::mousemap::Mousemap;
use interface_adapters::controllers::mouse_controller::{press_action, MouseButton, MousePress};
use interface_adapters::controllers::ui_controller::UiEvent;
use use_cases::editing::{DragBy, DragCombine, DragPoint, DragSelect};

use crate::input::mouse_modifiers;

/// Presses closer in time than this (seconds) and space (points) count as a double click.
const MULTI_CLICK_TIME: f64 = 0.4;
const MULTI_CLICK_DISTANCE: f32 = 6.0;

/// A press bound by the mousemaps, while its button is down.
#[derive(Clone)]
struct Gesture {
    anchor: DragPoint,
    head: DragPoint,
    /// The selections it makes; `None`: the widget's (a plain click whose release runs a command).
    drag: Option<(DragBy, DragCombine)>,
    /// Run when the button goes up.
    release: Option<Command>,
}

/// The last press in a view: when, where and how many in a row.
#[derive(Clone, Copy)]
struct LastPress {
    time: f64,
    pos: Pos2,
    clicks: u32,
}

fn gesture_id(id: Id) -> Id {
    id.with("drag_select")
}

/// A press this frame that may start a gesture: its button and where (from the events: a
/// release in the same frame already cleared egui's `press_origin`).
fn press(ctx: &egui::Context) -> Option<(PointerButton, Pos2)> {
    ctx.input(|i| {
        i.events.iter().find_map(|event| match *event {
            egui::Event::PointerButton {
                pos,
                button,
                pressed: true,
                ..
            } => Some((button, pos)),
            _ => None,
        })
    })
}

/// Before the text widget is shown: its state, when a gesture goes on or a button went down
/// (the widget would move its caret).
pub fn state_before(ctx: &egui::Context, id: Id) -> Option<TextEditState> {
    let going_on = ctx.data(|d| d.get_temp::<Gesture>(gesture_id(id)).is_some());
    (going_on || press(ctx).is_some())
        .then(|| TextEditState::load(ctx, id))
        .flatten()
}

/// Where the pointer is in the text: the char nearest to it and the columns past its row's end
/// (at the end of a wrapped row, its end is the next row's start: `after_row` tells them apart).
fn drag_point(galley: &egui::Galley, origin: Pos2, pointer: Pos2, char_width: f32) -> DragPoint {
    let cursor = galley.cursor_from_pos(pointer - origin);
    let row = galley.rows.get(cursor.rcursor.row);
    let last_of_line =
        row.is_some_and(|r| r.ends_with_newline) || cursor.rcursor.row + 1 >= galley.rows.len();
    let at_end = row.is_some_and(|r| cursor.rcursor.column >= r.char_count_excluding_newline());
    let past_end = match row {
        Some(row) if (last_of_line || at_end) && char_width > 0.0 => {
            ((pointer.x - origin.x - row.rect.right()) / char_width)
                .round()
                .max(0.0) as usize
        }
        _ => 0,
    };
    DragPoint {
        point: cursor.ccursor.index,
        past_end,
        after_row: at_end && !last_of_line,
    }
}

/// How many presses in a row this one is (2: a double click).
fn count_clicks(ctx: &egui::Context, id: Id, pos: Pos2) -> u32 {
    let key = id.with("last_press");
    let time = ctx.input(|i| i.time);
    let clicks = match ctx.data(|d| d.get_temp::<LastPress>(key)) {
        Some(last)
            if time - last.time < MULTI_CLICK_TIME
                && last.pos.distance(pos) < MULTI_CLICK_DISTANCE =>
        {
            last.clicks + 1
        }
        _ => 1,
    };
    ctx.data_mut(|d| d.insert_temp(key, LastPress { time, pos, clicks }));
    clicks
}

/// The text widget of `document` and where its text is drawn.
pub struct Text<'a> {
    pub id: Id,
    pub document: DocumentId,
    pub galley: &'a egui::Galley,
    pub origin: Pos2,
    pub char_width: f32,
    pub mousemap: &'a Mousemap,
}

/// What [`follow`] leaves to the view.
#[derive(Default)]
pub struct Followed {
    /// The selection the widget goes on with while a gesture makes the selections.
    pub selection: Option<CCursorRange>,
    /// The command of a button that went up, to run once the widget's click is told (a
    /// `goto_definition` on Ctrl + click is about the caret the click put).
    pub release: Option<Command>,
}

/// The mousemaps' button of an egui one.
fn mouse_button(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Primary => MouseButton::Primary,
        PointerButton::Secondary => MouseButton::Secondary,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Extra1 => MouseButton::Back,
        PointerButton::Extra2 => MouseButton::Forward,
    }
}

/// After the text widget is shown: starts, follows or ends a gesture, and gives the widget its
/// state from before (`before`) while one makes the selections. `in_text` tells whether a point
/// is over the text (not the gutter or another view).
pub fn follow(
    ctx: &egui::Context,
    text: &Text,
    before: Option<TextEditState>,
    in_text: impl Fn(Pos2) -> bool,
    events: &mut Vec<UiEvent>,
) -> Followed {
    let key = gesture_id(text.id);
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let at = |pos: Pos2| drag_point(text.galley, text.origin, pos, text.char_width);
    let mut gesture = ctx.data(|d| d.get_temp::<Gesture>(key));
    let new_press = press(ctx).filter(|&(_, pos)| in_text(pos));
    // A gesture whose button went up while the view was not shown (another tab came to the
    // front), or one a new press replaces, is over: it must not move the selections (nor run
    // its command) now.
    let released_now = ctx.input(|i| i.pointer.any_released());
    let down_now = ctx.input(|i| i.pointer.any_down());
    if let Some(old) = &gesture {
        if new_press.is_some() || (!down_now && !released_now) {
            ctx.data_mut(|d| d.remove::<Gesture>(key));
            if old.drag.is_some() {
                events.push(UiEvent::DragSelectEnded);
            }
            gesture = None;
        }
    }
    let mut pressed = false;
    if gesture.is_none() {
        let Some((button, pos)) = new_press else {
            return Followed::default();
        };
        let button = mouse_button(button);
        let clicks = if button == MouseButton::Primary {
            count_clicks(ctx, text.id, pos)
        } else {
            1
        };
        let press = MousePress {
            button,
            modifiers: mouse_modifiers(ctx.input(|i| i.modifiers)),
            clicks,
        };
        let Some(action) = press_action(text.mousemap, press) else {
            return Followed::default();
        };
        if let Some(command) = action.press {
            events.push(UiEvent::RunCommand(command));
        }
        let point = at(pos);
        gesture = Some(Gesture {
            anchor: point,
            head: point,
            drag: action.drag,
            release: action.release,
        });
        pressed = true;
    }
    let Some(mut gesture) = gesture else {
        return Followed::default();
    };
    let down = ctx.input(|i| i.pointer.any_down());
    let head = pointer.map_or(gesture.head, at);
    if let Some((by, combine)) = gesture.drag {
        if pressed || head != gesture.head {
            events.push(UiEvent::DragSelect(
                text.document,
                DragSelect {
                    anchor: gesture.anchor,
                    head,
                    by,
                    combine,
                    pressed,
                },
            ));
        }
    }
    gesture.head = head;
    let drags = gesture.drag.is_some();
    let mut release = None;
    if down {
        ctx.data_mut(|d| d.insert_temp(key, gesture));
    } else {
        ctx.data_mut(|d| d.remove::<Gesture>(key));
        if drags {
            events.push(UiEvent::DragSelectEnded);
        }
        release = gesture.release;
    }
    // Only a gesture that makes the selections keeps the widget's from before.
    let selection = before.filter(|_| drags).and_then(|state| {
        let range = state.cursor.char_range();
        state.store(ctx, text.id);
        range
    });
    Followed { selection, release }
}
