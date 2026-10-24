//! Macro recording - Record and playback keystroke sequences

use chrono::Utc;
use uuid::Uuid;

/// A single macro keypress/event
#[derive(Debug, Clone)]
pub struct MacroEvent {
    pub timestamp: f64,
    pub key: String,
    pub modifiers: Modifiers,
    pub character: Option<char>,
    pub event_type: EventType,
}

/// The type of macro event
#[derive(Debug, Clone)]
pub enum EventType {
    KeyPress,
    TextInsert,
    Command,
    MouseClick,
}

/// Keyboard modifiers at time of event
#[derive(Debug, Clone)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub mac_ctrl: bool, // macOS Ctrl key (Cmd)
}

impl Modifiers {
    pub fn new(ctrl: bool, alt: bool, shift: bool, mac_ctrl: bool) -> Self {
        Self { ctrl, alt, shift, mac_ctrl }
    }

    pub fn none() -> Self {
        Self { ctrl: false, alt: false, shift: false, mac_ctrl: false }
    }

    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.ctrl { parts.push("Ctrl"); }
        if self.alt { parts.push("Alt"); }
        if self.shift { parts.push("Shift"); }
        if self.mac_ctrl { parts.push("Cmd"); }
        if parts.is_empty() {
            String::new()
        } else {
            parts.join("+")
        }
    }
}

/// A recorded macro
#[derive(Debug, Clone)]
pub struct Macro {
    pub id: Uuid,
    pub name: String,
    pub events: Vec<MacroEvent>,
    pub created_at: f64,
    pub run_count: usize,
}

impl Macro {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            events: Vec::new(),
            created_at: Utc::now().timestamp_millis() as f64 / 1000.0,
            run_count: 0,
        }
    }

    pub fn add_event(&mut self, event: MacroEvent) {
        self.events.push(event);
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }
}

/// Macro recorder state
#[derive(Debug)]
pub struct MacroRecorder {
    /// Currently recording macro
    pub recording: Option<Macro>,
    /// List of saved macros
    pub saved_macros: Vec<Macro>,
    /// Whether macro recording is active
    pub is_recording: bool,
}

impl MacroRecorder {
    pub fn new() -> Self {
        Self {
            recording: None,
            saved_macros: Vec::new(),
            is_recording: false,
        }
    }

    /// Start recording a new macro
    pub fn start_recording(&mut self, name: Option<String>) {
        if !self.is_recording {
            let macro_name = name.unwrap_or_else(|| {
                format!("Macro {}", self.saved_macros.len() + 1)
            });

            self.recording = Some(Macro::new(macro_name));
            self.is_recording = true;
        }
    }

    /// Stop recording and save the macro
    pub fn stop_recording(&mut self) {
        if let Some(macro_) = self.recording.take() {
            if !macro_.is_empty() {
                self.saved_macros.push(macro_);
            } else {
                // Discard empty macro
            }
        }
        self.is_recording = false;
    }

    /// Toggle recording
    pub fn toggle_recording(&mut self, name: Option<String>) {
        if self.is_recording {
            self.stop_recording();
        } else {
            self.start_recording(name);
        }
    }

    /// Record a key event
    pub fn record_key(&mut self, key: &str, modifiers: Modifiers, character: Option<char>) {
        if let Some(ref mut recording) = self.recording {
            recording.add_event(MacroEvent {
                timestamp: Utc::now().timestamp_millis() as f64 / 1000.0,
                key: key.to_string(),
                modifiers: modifiers.clone(),
                character,
                event_type: EventType::KeyPress,
            });
        }
    }

    /// Record a text insertion
    pub fn record_text(&mut self, text: &str) {
        if let Some(ref mut recording) = self.recording {
            recording.add_event(MacroEvent {
                timestamp: Utc::now().timestamp_millis() as f64 / 1000.0,
                key: String::new(),
                modifiers: Modifiers::none(),
                character: Some(text.chars().next().unwrap_or(' ')),
                event_type: EventType::TextInsert,
            });
        }
    }

    /// Record a command execution
    pub fn record_command(&mut self, command: &str) {
        if let Some(ref mut recording) = self.recording {
            recording.add_event(MacroEvent {
                timestamp: Utc::now().timestamp_millis() as f64 / 1000.0,
                key: command.to_string(),
                modifiers: Modifiers::none(),
                character: None,
                event_type: EventType::Command,
            });
        }
    }

    /// Get all saved macros
    pub fn list_macros(&self) -> &[Macro] {
        &self.saved_macros
    }

    /// Get a macro by ID
    pub fn get_macro(&self, id: Uuid) -> Option<&Macro> {
        self.saved_macros.iter().find(|m| m.id == id)
    }

    /// Delete a macro
    pub fn delete_macro(&mut self, id: Uuid) {
        self.saved_macros.retain(|m| m.id != id);
    }

    /// Clear all macros
    pub fn clear_all(&mut self) {
        self.saved_macros.clear();
    }
}

/// Macro player - replays recorded events
pub struct MacroPlayer {
    /// Events being replayed
    pub events: Vec<MacroEvent>,
    /// Current event index
    pub current_index: usize,
    /// Whether playback is in progress
    pub is_playing: bool,
    /// Callback to execute each event
    pub on_event: Option<Box<dyn Fn(&MacroEvent) -> bool>>, // Returns false to stop early
}

impl MacroPlayer {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            current_index: 0,
            is_playing: false,
            on_event: None,
        }
    }

    /// Start playing a macro
    pub fn play(&mut self, macro_: &Macro) {
        self.events = macro_.events.clone();
        self.current_index = 0;
        self.is_playing = true;
    }

    /// Play the last recorded macro
    pub fn play_last(&mut self, macros: &[Macro]) -> bool {
        if let Some(last) = macros.last() {
            self.play(last);
            true
        } else {
            false
        }
    }

    /// Process the next event in the playback queue
    pub fn play_next(&mut self) -> bool {
        if self.current_index >= self.events.len() || !self.is_playing {
            self.is_playing = false;
            return false;
        }

        let event = &self.events[self.current_index];
        self.current_index += 1;

        // Execute the event via callback
        if let Some(ref callback) = self.on_event {
            if !callback(event) {
                self.is_playing = false;
                return false;
            }
        }

        // Continue if more events
        self.current_index < self.events.len()
    }

    /// Stop playback
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.current_index = 0;
    }

    /// Check if currently playing
    pub fn is_active(&self) -> bool {
        self.is_playing
    }
}

/// Render the macro panel in egui
pub fn render_macro_panel(
    ui: &mut egui::Ui,
    recorder: &MacroRecorder,
    is_recording: bool,
    colors: &ThemeColors,
) {
    use egui::{Frame, Margin};

    ui.label(egui::RichText::new("Macros").color(colors.fg));
    ui.separator();

    // Recording status
    if is_recording {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("●")
                .color(egui::Color32::from_rgb(255, 80, 80))
                .monospace());
            ui.label(egui::RichText::new("Recording...")
                .color(egui::Color32::from_rgb(255, 80, 80)));
        });
    } else if recorder.is_recording {
        // During recording, events accumulate
    }

    // Saved macros
    for macro_ in recorder.list_macros() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&macro_.name)
                .color(colors.fg));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(&format!("{} events", macro_.event_count()))
                    .color(colors.line_number)
                    .monospace()
                    .size(11.0));
            });
        });
    }

    if recorder.list_macros().is_empty() {
        ui.label(egui::RichText::new("No macros recorded yet")
            .color(colors.line_number));
    }

    ui.separator();
}

/// Theme colors reference
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: egui::Color32,
    pub fg: egui::Color32,
    pub border: egui::Color32,
    pub line_number: egui::Color32,
}

impl ThemeColors {
    pub fn dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(25, 25, 35),
            fg: egui::Color32::from_rgb(200, 200, 200),
            border: egui::Color32::from_rgb(60, 60, 80),
            line_number: egui::Color32::from_rgb(100, 100, 120),
        }
    }

    pub fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(240, 240, 245),
            fg: egui::Color32::from_rgb(40, 40, 40),
            border: egui::Color32::from_rgb(180, 180, 200),
            line_number: egui::Color32::from_rgb(120, 120, 140),
        }
    }
}
