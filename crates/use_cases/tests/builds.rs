//! Tests for build systems (`use_cases::builds`), with a scripted process.

use entities::command::Command;
use serde_json::json;
use test_support::{open, status, World};
use use_cases::builds::BuildPlan;
use use_cases::gateways::ProcessCommand::{Program, Shell};
use use_cases::gateways::ProcessEvent::{Finished, Output};
use use_cases::Editor;

const PYTHON: &str = "Packages/Python/Python.sublime-build";

fn python_build() -> &'static str {
    r#"{
        "selector": "source.python",
        "shell_cmd": "python \"$file\"",
        "file_regex": "^File \"(.*)\", line (\\d+)",
        "variants": [{ "name": "Lint", "shell_cmd": "lint \"$file_name\"" }]
    }"#
}

fn world() -> World {
    World::new().with_file("/p/a.py", "print(1)\n").with_file("/p/notes.txt", "x\n").with_resource(PYTHON, python_build())
}

fn panel_text(editor: &Editor) -> String {
    editor.shown_output_panel().expect("the panel is shown").document.text.clone()
}

/// What `build` would run for the active file.
fn planned(editor: &mut Editor) -> Command {
    match editor.plan_build("", "") {
        BuildPlan::Run(command) => command,
        other => panic!("expected a command, got {other:?}"),
    }
}

/// Runs the build and shows the panel, as the controller does.
fn build(editor: &mut Editor) {
    let command = planned(editor);
    assert_eq!(command.name, "exec");
    assert!(editor.exec(&command.args));
    editor.show_output_panel("exec");
}

#[test]
fn a_build_is_chosen_by_the_scope_of_the_file_and_its_variables_expand() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    assert_eq!(planned(&mut editor).args["shell_cmd"], "python \"/p/a.py\"");
    assert!(world.processes.started.borrow().is_empty(), "planning runs nothing");
}

#[test]
fn a_file_no_build_applies_to_is_reported() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/notes.txt");
    assert_eq!(editor.plan_build("", ""), BuildPlan::Nothing);
    assert_eq!(status(&editor), "No build system for this file");
}

#[test]
fn file_patterns_and_keyfiles_also_choose_a_build() {
    let world = World::new()
        .with_file("/p/notes.txt", "x")
        .with_file("/q/sub/a.c", "x")
        .with_file("/q/Makefile", "all:")
        .with_resource("Packages/U/Text.sublime-build", r#"{ "file_patterns": ["*.txt"], "cmd": ["wc"] }"#)
        .with_resource("Packages/U/Make.sublime-build", r#"{ "keyfiles": ["Makefile"], "cmd": ["make"] }"#);
    let mut editor = world.editor();
    open(&mut editor, "/p/notes.txt");
    assert_eq!(planned(&mut editor).args["cmd"], json!(["wc"]));
    open(&mut editor, "/q/sub/a.c");
    assert_eq!(planned(&mut editor).args["cmd"], json!(["make"]));
}

#[test]
fn several_builds_for_a_file_are_a_choice_that_stays() {
    let world = world().with_resource("Packages/U/Other.sublime-build", r#"{ "selector": "source.python", "cmd": ["other"] }"#);
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    assert_eq!(editor.plan_build("", ""), BuildPlan::Choose);

    let BuildPlan::Run(command) = editor.plan_build("Packages/U/Other.sublime-build", "") else {
        panic!("the file was given");
    };
    assert_eq!(command.args["cmd"], json!(["other"]));
    assert_eq!(planned(&mut editor).args["cmd"], json!(["other"]), "remembered");
}

#[test]
fn the_picker_lists_builds_and_variants_for_the_file() {
    let world = world().with_resource("Packages/U/Web.sublime-build", r#"{ "selector": "text.html", "cmd": ["x"] }"#);
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    editor.refresh_build_options();
    let results = editor.search(use_cases::search::SearchKind::BuildSystems, "");
    let captions: Vec<&str> = results.iter().map(|c| c.caption.as_str()).collect();
    assert_eq!(captions, ["Python", "Python - Lint"]);
}

#[test]
fn a_variant_replaces_the_arguments_of_the_build() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    let BuildPlan::Run(command) = editor.plan_build("", "Lint") else {
        panic!("one build applies");
    };
    assert_eq!(command.args["shell_cmd"], "lint \"a.py\"");
}

#[test]
fn set_build_system_applies_to_any_file_until_cleared() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/notes.txt");
    editor.set_build_system(PYTHON, "Lint");
    assert_eq!(planned(&mut editor).args["shell_cmd"], "lint \"notes.txt\"");
    editor.set_build_system("", "");
    assert_eq!(editor.plan_build("", ""), BuildPlan::Nothing);
}

#[test]
fn building_saves_the_named_files_first_unless_told_not_to() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    editor.text_for_editing(editor.active_document().unwrap().id).unwrap().insert(0, '#');
    planned(&mut editor);
    assert_eq!(world.file_text("/p/a.py"), "#print(1)\n");

    let world = self::world().with_preferences(json!({ "save_all_on_build": false }));
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    editor.text_for_editing(editor.active_document().unwrap().id).unwrap().insert(0, '#');
    planned(&mut editor);
    assert_eq!(world.file_text("/p/a.py"), "print(1)\n");
}

#[test]
fn exec_runs_next_to_the_file_and_the_output_reaches_the_panel() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    build(&mut editor);
    {
        let started = world.processes.started.borrow();
        assert_eq!(started[0].command, Shell("python \"/p/a.py\"".into()));
        assert_eq!(started[0].working_dir.as_deref(), Some(std::path::Path::new("/p")));
    }
    assert!(editor.build_is_running());

    world.processes.events.borrow_mut().extend([Output("hello\r\n".into()), Output("wor".into()), Output("ld\n".into()), Finished(Some(0))]);
    editor.poll_builds();
    let text = panel_text(&editor);
    assert!(text.starts_with("hello\nworld\n\n[Finished in "), "{text}");
    assert!(!text.contains("[cmd:"), "{text}");
    assert_eq!(status(&editor), "Build finished");
    assert!(!editor.build_is_running());
}

#[test]
fn a_failed_build_says_what_ran_and_counts_its_errors() {
    let world = world();
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    build(&mut editor);
    world.processes.events.borrow_mut().extend([Output("File \"a.py\", line 1\n".into()), Finished(Some(2))]);
    editor.poll_builds();
    let text = panel_text(&editor);
    assert!(text.contains("with exit code 2]\n[cmd: python \"/p/a.py\"]\n[dir: /p]\n"), "{text}");
    assert_eq!(status(&editor), "Build finished with 1 error");
}

#[test]
fn quiet_builds_only_report_failures() {
    let world = World::new();
    let mut editor = world.editor();
    let quiet = json!({ "cmd": ["x"], "quiet": true });
    editor.exec(&quiet);
    editor.show_output_panel("exec");
    world.processes.events.borrow_mut().push_back(Finished(Some(0)));
    editor.poll_builds();
    assert_eq!(panel_text(&editor), "");

    editor.exec(&quiet);
    world.processes.events.borrow_mut().push_back(Finished(Some(1)));
    editor.poll_builds();
    assert!(panel_text(&editor).contains("exit code 1"));
}

#[test]
fn cmd_can_be_a_program_or_a_shell_line_with_env_and_path() {
    let world = World::new();
    let mut editor = world.editor();
    editor.exec(&json!({ "cmd": ["a", "b c"], "working_dir": "/w", "env": { "K": "v" }, "path": "/bin" }));
    editor.exec(&json!({ "cmd": ["a", "b"], "shell": true }));
    editor.exec(&json!({ "cmd": "a b" }));
    let started = world.processes.started.borrow();
    assert_eq!(started[0].command, Program(vec!["a".into(), "b c".into()]));
    assert_eq!(started[0].working_dir.as_deref(), Some(std::path::Path::new("/w")));
    assert_eq!(started[0].env, [("K".to_string(), "v".to_string())]);
    assert_eq!(started[0].path.as_deref(), Some("/bin"));
    assert_eq!(started[1].command, Shell("a b".into()));
    assert_eq!(started[2].command, Shell("a b".into()));
}

#[test]
fn exec_with_nothing_to_run_is_refused() {
    let world = World::new();
    let mut editor = world.editor();
    assert!(!editor.exec(&json!({ "working_dir": "/w" })));
    assert!(world.processes.started.borrow().is_empty());
    assert_eq!(status(&editor), "Nothing to run: the build has no shell_cmd or cmd");
}

#[test]
fn a_new_build_kills_the_one_running_and_starts_clean() {
    let world = World::new();
    let mut editor = world.editor();
    let args = json!({ "cmd": ["x"] });
    editor.exec(&args);
    editor.show_output_panel("exec");
    world.processes.events.borrow_mut().push_back(Output("old".into()));
    editor.poll_builds();
    editor.exec(&args);
    assert_eq!(world.processes.kills.get(), 1);
    assert_eq!(panel_text(&editor), "", "the panel starts empty");
}

#[test]
fn cancelling_kills_the_process_and_says_so() {
    let world = World::new();
    let mut editor = world.editor();
    editor.exec(&json!({ "cmd": ["x"] }));
    editor.show_output_panel("exec");
    assert!(!editor.exec(&json!({ "kill": true })));
    assert_eq!(world.processes.kills.get(), 1);
    world.processes.events.borrow_mut().push_back(Finished(None));
    editor.poll_builds();
    assert!(panel_text(&editor).ends_with("\n[Cancelled]\n"), "{}", panel_text(&editor));
}

#[test]
fn cancelling_with_no_build_is_reported() {
    let world = World::new();
    let mut editor = world.editor();
    editor.cancel_build();
    assert_eq!(world.processes.kills.get(), 0);
    assert_eq!(status(&editor), "No build is running");
}

#[test]
fn a_process_that_does_not_start_writes_the_error_to_the_panel() {
    let world = World::new();
    *world.processes.start_error.borrow_mut() = Some("no such program".into());
    let mut editor = world.editor();
    assert!(editor.exec(&json!({ "cmd": ["x"] })));
    editor.show_output_panel("exec");
    assert_eq!(panel_text(&editor), "[Could not run: no such program]\n");
    assert!(!editor.build_is_running());
}

#[test]
fn a_custom_target_gets_the_arguments_and_cancels_as_it_says() {
    let cases: [(&str, Command); 3] = [
        (r#"{ "target": "mine", "cmd": ["x"], "selector": "source.python" }"#, Command::with_args("mine", json!({ "kill": true }))),
        (r#"{ "target": "mine", "cancel": "stop_it", "selector": "source.python" }"#, Command::new("stop_it")),
        (r#"{ "target": "mine", "cancel": { "stop": 1 }, "selector": "source.python" }"#, Command::with_args("mine", json!({ "stop": 1 }))),
    ];
    for (json, cancel) in cases {
        let world = World::new().with_file("/p/a.py", "").with_resource("Packages/U/Mine.sublime-build", json);
        let mut editor = world.editor();
        open(&mut editor, "/p/a.py");
        assert_eq!(planned(&mut editor).name, "mine");
        assert_eq!(editor.cancel_build_command(), cancel);
    }
}

#[test]
fn a_build_file_that_cannot_be_read_goes_to_the_console() {
    let world = world().with_resource("Packages/U/Broken.sublime-build", "{ nope");
    let mut editor = world.editor();
    open(&mut editor, "/p/a.py");
    assert_eq!(planned(&mut editor).name, "exec");
    assert!(editor.state().console.contains("Broken.sublime-build"));
}
