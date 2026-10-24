//! egui views. Each one draws a view model and reports what the user did as
//! [`UiEvent`](interface_adapters::controllers::ui_controller::UiEvent)s; views never change
//! application state themselves (the one exception, typing into the text area, is explained in
//! [`editor_view`]).

pub mod dialogs;
pub mod editor_view;
pub mod find_panel;
pub mod menu_bar;
pub mod quick_panel;
pub mod side_bar;
pub mod status_bar;
pub mod tabs;
pub mod theme;
