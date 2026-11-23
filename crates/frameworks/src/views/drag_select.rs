//! The mouse gestures in the text that make several selections (Ctrl + click, Alt + click, column
//! drags…, see `mouse_controller`). The text widget only knows one selection and would move its
//! caret at the press, so while such a gesture goes on its state is put back after it is shown
//! (as `context_menu::keep_selection` does for a right click) and the gesture goes to the use
//! cases as `UiEvent::DragSelect`, every frame the pointer moves, until the button goes up.

use egui::text::CCursorRange;
use egui::text_edit::TextEditState;
use egui::{Id, PointerButton, Pos2};
use entities::document::DocumentId;
use interface_adapters::controllers::mouse_controller::{
    drag_gesture, MouseButton, MousePlatform, MousePress,
};
use interface_adapters::controllers::ui_controller::UiEvent;
use use_cases::editing::{DragBy, DragCombine, DragPoint, DragSelect};

/// Presses closer in time than this (seconds) and space (points) count as a double click.
const MULTI_CLICK_TIME: f64 = 0.4;
const MULTI_CLICK_DISTANCE: f32 = 6.0;

/// A gesture going on in a view.
#[derive(Clone, Copy)]
struct Gesture {
    anchor: DragPoint,
    head: DragPoint,
    by: DragBy,
    combine: DragCombine,
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
                button:
                    button @ (PointerButton::Primary
                    | PointerButton::Secondary
                    | PointerButton::Middle),
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

/// Where the pointer is in the text: the char nearest to it and the columns past its line's end.
fn drag_point(galley: &egui::Galley, origin: Pos2, pointer: Pos2, char_width: f32) -> DragPoint {
    let cursor = galley.cursor_from_pos(pointer - origin);
    let row = galley.rows.get(cursor.rcursor.row);
    let last_of_line =
        row.is_some_and(|r| r.ends_with_newline) || cursor.rcursor.row + 1 >= galley.rows.len();
    let past_end = match row {
        Some(row) if last_of_line && char_width > 0.0 => ((pointer.x - origin.x - row.rect.right())
            / char_width)
            .round()
            .max(0.0) as usize,
        _ => 0,
    };
    DragPoint {
        point: cursor.ccursor.index,
        past_end,
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
}

/// After the text widget is shown: starts, follows or ends a gesture, and gives the widget its
/// state from before (`before`) while one goes on; the selection to go on with then. `in_text`
/// tells whether a point is over the text (not the gutter or another view).
pub fn follow(
    ctx: &egui::Context,
    text: &Text,
    before: Option<TextEditState>,
    in_text: impl Fn(Pos2) -> bool,
    events: &mut Vec<UiEvent>,
) -> Option<CCursorRange> {
    let key = gesture_id(text.id);
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let at = |pos: Pos2| drag_point(text.galley, text.origin, pos, text.char_width);
    let mut gesture = ctx.data(|d| d.get_temp::<Gesture>(key));
    let new_press = press(ctx).filter(|&(_, pos)| in_text(pos));
    // A gesture whose button went up while the view was not shown (another tab came to the
    // front), or one a new press replaces, is over: it must not move the selections now.
    let released_now = ctx.input(|i| i.pointer.any_released());
    let down_now = ctx.input(|i| i.pointer.any_down());
    if gesture.is_some() && (new_press.is_some() || (!down_now && !released_now)) {
        ctx.data_mut(|d| d.remove::<Gesture>(key));
        events.push(UiEvent::DragSelectEnded);
        gesture = None;
    }
    let mut pressed = false;
    if gesture.is_none() {
        let (button, pos) = new_press?;
        let modifiers = ctx.input(|i| i.modifiers);
        let button = match button {
            PointerButton::Secondary => MouseButton::Secondary,
            PointerButton::Middle => MouseButton::Middle,
            _ => MouseButton::Primary,
        };
        let clicks = if button == MouseButton::Primary {
            count_clicks(ctx, text.id, pos)
        } else {
            1
        };
        let press = MousePress {
            button,
            shift: modifiers.shift,
            alt: modifiers.alt,
            command: modifiers.command,
            clicks,
        };
        let (by, combine) = drag_gesture(press, MousePlatform::current())?;
        let point = at(pos);
        gesture = Some(Gesture {
            anchor: point,
            head: point,
            by,
            combine,
        });
        pressed = true;
    }
    let mut gesture = gesture?;
    let down = ctx.input(|i| i.pointer.any_down());
    let head = pointer.map_or(gesture.head, at);
    if pressed || head != gesture.head {
        gesture.head = head;
        events.push(UiEvent::DragSelect(
            text.document,
            DragSelect {
                anchor: gesture.anchor,
                head,
                by: gesture.by,
                combine: gesture.combine,
                pressed,
            },
        ));
    }
    if down {
        ctx.data_mut(|d| d.insert_temp(key, gesture));
    } else {
        ctx.data_mut(|d| d.remove::<Gesture>(key));
        events.push(UiEvent::DragSelectEnded);
    }
    let state = before?;
    let range = state.cursor.char_range();
    state.store(ctx, text.id);
    range
}
