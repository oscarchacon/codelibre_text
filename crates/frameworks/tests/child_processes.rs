//! Tests for `gateways::child_processes`: they run real programs through the system's shell.

use std::time::{Duration, Instant};

use codelibre_text::gateways::child_processes::ChildProcesses;
use use_cases::gateways::{ProcessCommand, ProcessEvent, ProcessGateway, ProcessRequest};

fn shell(line: &str) -> ProcessRequest {
    ProcessRequest { command: ProcessCommand::Shell(line.to_string()), working_dir: None, env: Vec::new(), path: None }
}

/// Everything the process wrote, and its exit code, once it ends.
fn wait(processes: &mut ChildProcesses) -> (String, Option<i32>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut output = String::new();
    while Instant::now() < deadline {
        for event in processes.poll() {
            match event {
                ProcessEvent::Output(text) => output.push_str(&text),
                ProcessEvent::Finished(code) => return (output, code),
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("the process did not end; wrote: {output}");
}

#[test]
fn a_shell_line_writes_its_output_and_exit_code() {
    let mut processes = ChildProcesses::default();
    processes.start(&shell("echo hello && exit 3")).unwrap();
    let (output, code) = wait(&mut processes);
    assert_eq!(output.trim(), "hello");
    assert_eq!(code, Some(3));
}

#[test]
fn quotes_reach_the_shell_as_written() {
    let mut processes = ChildProcesses::default();
    processes.start(&shell("echo \"two  spaces\"")).unwrap();
    let (output, _) = wait(&mut processes);
    assert!(output.contains("two  spaces"), "{output}");
}

#[test]
fn stderr_is_read_too() {
    let mut processes = ChildProcesses::default();
    processes.start(&shell("echo oops 1>&2")).unwrap();
    let (output, code) = wait(&mut processes);
    assert_eq!(output.trim(), "oops");
    assert_eq!(code, Some(0));
}

#[test]
fn the_working_folder_and_the_environment_are_used() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("marker.txt"), "found").unwrap();
    let mut request = shell(if cfg!(windows) { "type marker.txt && echo %MY_VALUE%" } else { "cat marker.txt && echo $MY_VALUE" });
    request.working_dir = Some(dir.path().to_path_buf());
    request.env = vec![("MY_VALUE".to_string(), "from-env".to_string())];
    let mut processes = ChildProcesses::default();
    processes.start(&request).unwrap();
    let (output, _) = wait(&mut processes);
    assert!(output.contains("found") && output.contains("from-env"), "{output}");
}

#[test]
fn environment_values_can_use_the_current_environment() {
    let expanded = if cfg!(windows) { "x;%PATH%" } else { "x:$PATH" };
    let mut request = shell(if cfg!(windows) { "echo %MY_PATH%" } else { "echo $MY_PATH" });
    request.env = vec![("MY_PATH".to_string(), expanded.to_string())];
    let mut processes = ChildProcesses::default();
    processes.start(&request).unwrap();
    let (output, _) = wait(&mut processes);
    let path = std::env::var("PATH").unwrap();
    assert!(output.trim().ends_with(path.trim()), "{output}");
}

#[test]
fn a_program_that_does_not_exist_is_an_error() {
    let mut processes = ChildProcesses::default();
    let request = ProcessRequest { command: ProcessCommand::Program(vec!["no-such-program-here".into()]), ..shell("") };
    assert!(processes.start(&request).is_err());
}

#[test]
fn killing_ends_the_process_and_its_children() {
    let mut processes = ChildProcesses::default();
    processes.start(&shell(if cfg!(windows) { "ping -n 30 127.0.0.1" } else { "sleep 30" })).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let started = Instant::now();
    processes.kill();
    wait(&mut processes);
    assert!(started.elapsed() < Duration::from_secs(10), "{:?}", started.elapsed());
}

#[test]
fn a_new_process_replaces_the_events_of_the_old_one() {
    let mut processes = ChildProcesses::default();
    processes.start(&shell("echo old")).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    processes.start(&shell("echo new")).unwrap();
    let (output, _) = wait(&mut processes);
    assert_eq!(output.trim(), "new");
}
