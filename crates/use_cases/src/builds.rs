//! Use cases: build systems. `build` finds the `.sublime-build` for the active file and turns it
//! into a command, usually `exec`, which runs a process and writes its output to the `exec`
//! output panel, whose lines lead to errors (`next_result`, see [`crate::output_panels`]).
//!
//! The process runs in the background; [`Editor::poll_builds`] picks up what it wrote every frame.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use entities::build::{expand_arguments, selector_matches, BuildSystem};
use entities::command::Command;
use serde_json::{json, Map, Value};

use crate::editor::Editor;
use crate::gateways::{platform_name, ProcessCommand, ProcessEvent, ProcessRequest};
use crate::project::matches_pattern;

/// The output panel of builds.
const EXEC_PANEL: &str = "exec";

/// What `build` should do next.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildPlan {
    /// Run this command (the build's target).
    Run(Command),
    /// Several builds apply: the user picks one.
    Choose,
    /// There is nothing to build (the user has been told).
    Nothing,
}

/// A build system and variant the user picked.
#[derive(Debug, Clone, PartialEq)]
struct BuildChoice {
    resource: String,
    variant: Option<String>,
    /// `set_build_system`: also for files the build does not apply to.
    always: bool,
}

struct RunningBuild {
    started: Instant,
    command_line: String,
    working_dir: String,
    path: String,
    quiet: bool,
    cancelled: bool,
}

#[derive(Default)]
pub(crate) struct BuildState {
    choice: Option<BuildChoice>,
    running: Option<RunningBuild>,
    /// How to cancel the last build started (`cancel_build`).
    cancel: Option<Command>,
}

/// One line of the build picker.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildOption {
    pub caption: String,
    pub resource: String,
    pub variant: Option<String>,
}

impl Editor {
    /// Every `.sublime-build` in the packages. Problems go to the console.
    fn build_systems(&mut self) -> Vec<BuildSystem> {
        let include_sublime = self.global_settings().bool("import_sublime_packages", true);
        let mut systems = Vec::new();
        for resource in self.gateways.resources.find_resources("*.sublime-build", include_sublime) {
            match self.gateways.resources.load_json_resource(&resource, include_sublime) {
                Ok(json) => systems.extend(BuildSystem::from_json(&resource, &json, platform_name())),
                Err(e) => self.state.console.push_str(&format!("{resource}: {e}\n")),
            }
        }
        systems
    }

    /// Whether a build applies to the active file: by the file's scope, its name, or a `keyfiles`
    /// entry in its folder, one above it, or an open folder.
    fn build_applies(&self, build: &BuildSystem) -> bool {
        let Some(doc) = self.active_document() else {
            return false;
        };
        if selector_matches(&build.selector, &self.gateways.syntaxes.base_scope(&doc.syntax)) {
            return true;
        }
        let name = doc.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        if name.is_some_and(|name| build.file_patterns.iter().any(|p| matches_pattern(p, &name))) {
            return true;
        }
        let ancestors = doc.path.iter().flat_map(|p| p.ancestors().skip(1));
        let mut folders = ancestors.chain(self.state.folders.iter().map(PathBuf::as_path));
        folders.any(|folder| build.keyfiles.iter().any(|key| self.gateways.file_system.exists(&folder.join(key))))
    }

    /// `set_build_system {"file": …}`: an empty file goes back to choosing by the active file.
    pub fn set_build_system(&mut self, file: &str, variant: &str) {
        self.state.builds.choice = (!file.is_empty()).then(|| BuildChoice {
            resource: file.to_string(),
            variant: (!variant.is_empty()).then(|| variant.to_string()),
            always: true,
        });
    }

    /// `build`, with its optional arguments: a `file` to use from now on, or a `variant`.
    pub fn plan_build(&mut self, file: &str, variant: &str) -> BuildPlan {
        if !file.is_empty() {
            let variant = (!variant.is_empty()).then(|| variant.to_string());
            self.state.builds.choice = Some(BuildChoice { resource: file.to_string(), variant, always: false });
        }
        let systems = self.build_systems();
        let applying: Vec<&BuildSystem> = systems.iter().filter(|b| self.build_applies(b)).collect();
        let chosen = self.state.builds.choice.clone().and_then(|choice| {
            let build = systems.iter().find(|b| b.resource == choice.resource)?;
            (choice.always || applying.contains(&build)).then_some((build, choice.variant))
        });
        let (build, chosen_variant) = match (chosen, applying.as_slice()) {
            (Some((build, variant)), _) => (build, variant),
            (None, [build]) => (*build, None),
            (None, []) => {
                self.notify(self.tr("No build system for this file").to_string());
                return BuildPlan::Nothing;
            }
            (None, _) => return BuildPlan::Choose,
        };
        let variant = if variant.is_empty() { chosen_variant.as_deref() } else { Some(variant) };
        let arguments = build.arguments_for(variant);
        let command = Command::with_args(&build.target, expand_arguments(&Value::Object(arguments), &self.build_variables()));
        self.state.builds.cancel = Some(match &build.cancel {
            Some(Value::String(name)) => Command::new(name),
            Some(arguments) => Command::with_args(&build.target, arguments.clone()),
            None => Command::with_args(&build.target, json!({ "kill": true })),
        });
        if self.global_settings().bool("save_all_on_build", true) {
            self.save_all();
        }
        BuildPlan::Run(command)
    }

    /// Reads the builds and variants to pick from (`SearchKind::BuildSystems` lists them): those
    /// for the active file, or all if none applies.
    pub fn refresh_build_options(&mut self) {
        let systems = self.build_systems();
        let applying: Vec<&BuildSystem> = systems.iter().filter(|b| self.build_applies(b)).collect();
        let shown = if applying.is_empty() { systems.iter().collect() } else { applying };
        let mut options = Vec::new();
        for build in shown {
            options.push(BuildOption { caption: build.name.clone(), resource: build.resource.clone(), variant: None });
            for variant in &build.variants {
                options.push(BuildOption {
                    caption: format!("{} - {}", build.name, variant.name),
                    resource: build.resource.clone(),
                    variant: Some(variant.name.clone()),
                });
            }
        }
        self.state.build_options = options;
    }

    /// The command that stops the last build (`exec` with `kill` unless the build says otherwise).
    pub fn cancel_build_command(&self) -> Command {
        self.state.builds.cancel.clone().unwrap_or_else(|| Command::with_args("exec", json!({ "kill": true })))
    }

    /// `$file`, `$folder` and the other variables of a build.
    pub fn build_variables(&self) -> HashMap<String, String> {
        let text = |value: &str| value.to_string();
        let mut variables = HashMap::from([
            (text("packages"), self.gateways.resources.paths().packages.display().to_string()),
            (text("platform"), text(platform_name())),
        ]);
        if let Some(folder) = self.state.folders.first() {
            variables.insert(text("folder"), folder.display().to_string());
        }
        if let Some(path) = self.active_document().and_then(|d| d.path.as_deref()) {
            let part = |value: Option<&std::ffi::OsStr>| value.map(|v| v.to_string_lossy().into_owned()).unwrap_or_default();
            variables.insert(text("file"), path.display().to_string());
            variables.insert(text("file_path"), path.parent().map(|p| p.display().to_string()).unwrap_or_default());
            variables.insert(text("file_name"), part(path.file_name()));
            variables.insert(text("file_base_name"), part(path.file_stem()));
            variables.insert(text("file_extension"), part(path.extension()));
        }
        variables
    }

    /// Sublime's `exec`: runs `shell_cmd` or `cmd`, replacing a build still running. True when the
    /// output panel should be shown.
    pub fn exec(&mut self, args: &Value) -> bool {
        if args.get("kill").and_then(Value::as_bool).unwrap_or(false) {
            self.cancel_build();
            return false;
        }
        let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default();
        let Some(command) = process_command(args) else {
            self.notify(self.tr("Nothing to run: the build has no shell_cmd or cmd").to_string());
            return false;
        };
        if self.state.builds.running.is_some() {
            self.gateways.processes.kill();
            self.state.builds.running = None;
        }
        let working_dir = Some(text("working_dir")).filter(|d| !d.is_empty()).map(PathBuf::from).or_else(|| self.default_build_dir());
        let result_base = working_dir.as_ref().map_or(String::new(), |d| d.display().to_string());
        self.open_exec_panel(text("file_regex"), text("line_regex"), &result_base);

        let path = args.get("path").and_then(Value::as_str).map(String::from);
        let request = ProcessRequest { command: command.clone(), working_dir, env: environment(args.get("env")), path: path.clone() };
        match self.gateways.processes.start(&request) {
            Ok(()) => {
                self.state.builds.running = Some(RunningBuild {
                    started: Instant::now(),
                    command_line: command_line(&command),
                    working_dir: result_base,
                    path: path.unwrap_or_default(),
                    quiet: args.get("quiet").and_then(Value::as_bool).unwrap_or(false),
                    cancelled: false,
                });
            }
            Err(e) => {
                let message = self.tr_args("[Could not run: {error}]", &[("error", &e)]);
                self.append_to_exec_panel(&format!("{message}\n"));
            }
        }
        self.global_settings().bool("show_panel_on_build", true)
    }

    /// Empties the `exec` panel and sets it up for this build's results.
    fn open_exec_panel(&mut self, file_regex: &str, line_regex: &str, base_dir: &str) {
        let id = self.create_output_panel(EXEC_PANEL);
        self.state.plugins.view_settings.insert(
            id,
            Map::from_iter([
                ("result_file_regex".to_string(), json!(file_regex)),
                ("result_line_regex".to_string(), json!(line_regex)),
                ("result_base_dir".to_string(), json!(base_dir)),
            ]),
        );
        if let Some(doc) = self.any_document_mut(id) {
            doc.read_only = true;
        }
        self.remember_results_panel(EXEC_PANEL);
    }

    /// Without a `working_dir`, a build runs next to its file, else in the first open folder.
    fn default_build_dir(&self) -> Option<PathBuf> {
        let file_dir = self.active_document().and_then(|d| d.path.as_deref()).and_then(Path::parent).map(Path::to_path_buf);
        file_dir.or_else(|| self.state.folders.first().cloned())
    }

    pub fn cancel_build(&mut self) {
        match &mut self.state.builds.running {
            Some(running) => {
                running.cancelled = true;
                self.gateways.processes.kill();
            }
            None => self.notify(self.tr("No build is running").to_string()),
        }
    }

    pub fn build_is_running(&self) -> bool {
        self.state.builds.running.is_some()
    }

    /// Every frame: writes what the build printed to its panel, and reports its end.
    pub fn poll_builds(&mut self) {
        if self.state.builds.running.is_none() {
            return;
        }
        for event in self.gateways.processes.poll() {
            match event {
                // Line endings in the panel are `\n`, whatever the program writes.
                ProcessEvent::Output(text) => self.append_to_exec_panel(&text.replace('\r', "")),
                ProcessEvent::Finished(code) => self.finish_build(code),
            }
        }
    }

    fn append_to_exec_panel(&mut self, text: &str) {
        if let Some(id) = self.find_output_panel(EXEC_PANEL) {
            // The panel may be gone (a plugin destroyed it); the build goes on.
            let _ = self.append_text(id, text, true);
        }
    }

    fn finish_build(&mut self, code: Option<i32>) {
        let Some(build) = self.state.builds.running.take() else {
            return;
        };
        let seconds = format!("{:.1}", build.started.elapsed().as_secs_f32());
        let time = [("time", &seconds as &dyn std::fmt::Display)];
        let mut report = if build.cancelled {
            self.tr("[Cancelled]").to_string()
        } else if code == Some(0) {
            self.tr_args("[Finished in {time}s]", &time)
        } else {
            let code = code.map_or_else(|| "?".to_string(), |c| c.to_string());
            let details = [("time", &seconds as &dyn std::fmt::Display), ("code", &code)];
            self.tr_args("[Finished in {time}s with exit code {code}]", &details)
        };
        if code != Some(0) && !build.cancelled {
            report.push_str(&format!("\n[cmd: {}]\n[dir: {}]", build.command_line, build.working_dir));
            if !build.path.is_empty() {
                report.push_str(&format!("\n[path: {}]", build.path));
            }
        }
        if !build.quiet || code != Some(0) {
            self.append_to_exec_panel(&format!("\n{report}\n"));
        }
        let errors = self.find_output_panel(EXEC_PANEL).map_or(0, |id| self.output_results(id).len());
        self.notify(match errors {
            0 => self.tr("Build finished").to_string(),
            n => self.tr_n("Build finished with {count} error", "Build finished with {count} errors", n, &[]),
        });
    }
}

/// `shell_cmd`, or `cmd` as a program and its arguments (a line for the shell with `"shell": true`
/// or when it is a string).
fn process_command(args: &Value) -> Option<ProcessCommand> {
    if let Some(line) = args.get("shell_cmd").and_then(Value::as_str).filter(|l| !l.is_empty()) {
        return Some(ProcessCommand::Shell(line.to_string()));
    }
    match args.get("cmd")? {
        Value::String(line) if !line.is_empty() => Some(ProcessCommand::Shell(line.clone())),
        Value::Array(parts) => {
            let parts: Vec<String> = parts.iter().filter_map(Value::as_str).map(String::from).collect();
            match args.get("shell").and_then(Value::as_bool) {
                _ if parts.is_empty() => None,
                Some(true) => Some(ProcessCommand::Shell(parts.join(" "))),
                _ => Some(ProcessCommand::Program(parts)),
            }
        }
        _ => None,
    }
}

fn command_line(command: &ProcessCommand) -> String {
    match command {
        ProcessCommand::Shell(line) => line.clone(),
        ProcessCommand::Program(parts) => parts.join(" "),
    }
}

fn environment(env: Option<&Value>) -> Vec<(String, String)> {
    let values = env.and_then(Value::as_object).into_iter().flatten();
    values.filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_string()))).collect()
}
