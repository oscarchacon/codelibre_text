//! Tests for `interface_adapters::controllers::mouse_controller`.

use interface_adapters::controllers::mouse_controller::{
    drag_gesture, MouseButton, MousePlatform, MousePress,
};
use use_cases::editing::{DragBy, DragCombine};

fn press(button: MouseButton, keys: &str, clicks: u32) -> MousePress {
    MousePress {
        button,
        shift: keys.contains("shift"),
        alt: keys.contains("alt"),
        command: keys.contains("command"),
        clicks,
    }
}

use DragBy::*;
use DragCombine::*;
use MouseButton::*;
use MousePlatform::*;

#[test]
fn windows_and_linux_follow_sublimes_mousemaps() {
    for platform in [Windows, Linux] {
        let gesture = |button, keys, clicks| drag_gesture(press(button, keys, clicks), platform);
        assert_eq!(gesture(Primary, "", 1), None);
        assert_eq!(gesture(Primary, "shift", 1), None);
        assert_eq!(gesture(Primary, "command", 1), Some((Characters, Add)));
        assert_eq!(gesture(Primary, "command", 2), Some((Words, Add)));
        assert_eq!(gesture(Primary, "command", 3), Some((Lines, Add)));
        assert_eq!(gesture(Primary, "alt", 1), Some((Characters, Subtract)));
        assert_eq!(gesture(Primary, "shift alt", 1), Some((Columns, Replace)));
        assert_eq!(
            gesture(Primary, "command shift alt", 1),
            Some((Columns, Add))
        );
        assert_eq!(gesture(Secondary, "shift", 1), Some((Columns, Replace)));
        assert_eq!(gesture(Secondary, "command shift", 1), Some((Columns, Add)));
        assert_eq!(gesture(Secondary, "", 1), None);
    }
    let middle = |platform| drag_gesture(press(Middle, "", 1), platform);
    assert_eq!(middle(Windows), Some((Columns, Replace)));
    assert_eq!(middle(Linux), None);
}

#[test]
fn macos_follows_sublimes_mousemap() {
    let gesture = |button, keys, clicks| drag_gesture(press(button, keys, clicks), MacOs);
    assert_eq!(gesture(Primary, "", 1), None);
    assert_eq!(gesture(Primary, "command", 1), Some((Characters, Add)));
    assert_eq!(gesture(Primary, "command", 2), Some((Words, Add)));
    assert_eq!(
        gesture(Primary, "shift command", 1),
        Some((Characters, Subtract))
    );
    assert_eq!(gesture(Primary, "alt", 1), Some((Columns, Replace)));
    assert_eq!(gesture(Primary, "command alt", 1), Some((Columns, Add)));
    assert_eq!(gesture(Middle, "", 1), Some((Columns, Replace)));
    assert_eq!(gesture(Middle, "command", 1), Some((Columns, Add)));
    assert_eq!(gesture(Secondary, "shift", 1), None);
}
