//! Presenters: build the [`view_models`](crate::view_models) for each part of the window from the
//! application state ([`use_cases::Editor`]) and the [`ViewState`](crate::view_state::ViewState).

pub mod dialogs;
pub mod editor;
pub mod find_panel;
pub mod menu;
pub mod output_panel;
pub mod quick_panel;
pub mod side_bar;
pub mod status_bar;
pub mod tabs;
pub mod window;
