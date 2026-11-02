//! [`ProcessGateway`] with `std::process`: a build's process, read by background threads that
//! wake the window when it writes.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use use_cases::gateways::{ProcessCommand, ProcessEvent, ProcessGateway, ProcessRequest};

/// `CREATE_NO_WINDOW`: console programs must not flash a window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

type Waker = Arc<dyn Fn() + Send + Sync>;

struct Running {
    pid: u32,
    events: Receiver<ProcessEvent>,
}

#[derive(Default)]
pub struct ChildProcesses {
    waker: Option<Waker>,
    running: Option<Running>,
}

impl ChildProcesses {
    /// `waker` runs whenever the process writes or ends, so the window shows it at once.
    pub fn with_waker(mut self, waker: Waker) -> Self {
        self.waker = Some(waker);
        self
    }
}

impl ProcessGateway for ChildProcesses {
    fn start(&mut self, request: &ProcessRequest) -> Result<(), String> {
        let mut command = base_command(&request.command)?;
        if let Some(dir) = &request.working_dir {
            command.current_dir(dir);
        }
        if let Some(path) = &request.path {
            command.env("PATH", expand_environment(path));
        }
        for (name, value) in &request.env {
            command.env(name, expand_environment(value));
        }
        command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        hide_and_group(&mut command);

        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let (sender, events) = channel();
        let readers: Vec<_> = [
            child.stdout.take().map(|out| Box::new(out) as Box<dyn Read + Send>),
            child.stderr.take().map(|err| Box::new(err) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|stream| {
            let (sender, waker) = (sender.clone(), self.waker.clone());
            thread::spawn(move || read_output(stream, &sender, waker.as_ref()))
        })
        .collect();
        let pid = child.id();
        let waker = self.waker.clone();
        thread::spawn(move || {
            let status = child.wait();
            for reader in readers {
                let _ = reader.join();
            }
            send(&sender, waker.as_ref(), ProcessEvent::Finished(status.ok().and_then(|s| s.code())));
        });
        // A new channel: what the previous process still writes goes nowhere.
        self.running = Some(Running { pid, events });
        Ok(())
    }

    fn poll(&mut self) -> Vec<ProcessEvent> {
        self.running.as_ref().map_or_else(Vec::new, |running| running.events.try_iter().collect())
    }

    fn kill(&mut self) {
        if let Some(running) = &self.running {
            kill_tree(running.pid);
        }
    }
}

impl Drop for ChildProcesses {
    fn drop(&mut self) {
        self.kill();
    }
}

fn send(sender: &Sender<ProcessEvent>, waker: Option<&Waker>, event: ProcessEvent) {
    let _ = sender.send(event);
    if let Some(wake) = waker {
        wake();
    }
}

/// Sends the text of a stream as it arrives, keeping a char cut between two reads for the next one.
fn read_output(mut stream: Box<dyn Read + Send>, sender: &Sender<ProcessEvent>, waker: Option<&Waker>) {
    let mut buffer = [0; 4096];
    let mut pending = Vec::new();
    while let Ok(count) = stream.read(&mut buffer) {
        if count == 0 {
            break;
        }
        pending.extend_from_slice(&buffer[..count]);
        let complete = match std::str::from_utf8(&pending) {
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            _ => pending.len(),
        };
        if complete > 0 {
            send(sender, waker, ProcessEvent::Output(String::from_utf8_lossy(&pending[..complete]).into_owned()));
            pending.drain(..complete);
        }
    }
    if !pending.is_empty() {
        send(sender, waker, ProcessEvent::Output(String::from_utf8_lossy(&pending).into_owned()));
    }
}

#[cfg(windows)]
fn base_command(command: &ProcessCommand) -> Result<Command, String> {
    use std::os::windows::process::CommandExt;
    Ok(match command {
        ProcessCommand::Shell(line) => {
            let mut command = Command::new("cmd");
            // Verbatim: `cmd` has its own quoting rules, which Rust's escaping would break.
            command.raw_arg(format!("/S /C \"{line}\""));
            command
        }
        ProcessCommand::Program(parts) => program(parts)?,
    })
}

#[cfg(not(windows))]
fn base_command(command: &ProcessCommand) -> Result<Command, String> {
    Ok(match command {
        ProcessCommand::Shell(line) => {
            let mut command = Command::new("/bin/sh");
            command.arg("-c").arg(line);
            command
        }
        ProcessCommand::Program(parts) => program(parts)?,
    })
}

fn program(parts: &[String]) -> Result<Command, String> {
    let (name, args) = parts.split_first().ok_or("no program to run")?;
    let mut command = Command::new(name);
    command.args(args);
    Ok(command)
}

#[cfg(windows)]
fn hide_and_group(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Its own process group, so killing it also reaches what it started.
#[cfg(not(windows))]
fn hide_and_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(windows)]
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let result = Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).creation_flags(CREATE_NO_WINDOW).output();
    if let Err(e) = result {
        tracing::warn!("could not kill process {pid}: {e}");
    }
}

#[cfg(not(windows))]
fn kill_tree(pid: u32) {
    if let Err(e) = Command::new("kill").args(["-TERM", "--", &format!("-{pid}")]).output() {
        tracing::warn!("could not kill process {pid}: {e}");
    }
}

/// `$NAME`, `${NAME}` and `%NAME%` replaced by this process's environment; unset ones stay.
fn expand_environment(value: &str) -> String {
    let is_name = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find(['$', '%']) {
        out.push_str(&rest[..at]);
        let marker = &rest[at..at + 1];
        let tail = &rest[at + 1..];
        let (name, after) = match (marker, tail.strip_prefix('{')) {
            ("$", Some(inner)) => inner.split_once('}').unwrap_or(("", tail)),
            ("$", None) => tail.split_at(tail.find(|c| !is_name(c)).unwrap_or(tail.len())),
            _ => tail.split_once('%').filter(|(name, _)| name.chars().all(is_name)).unwrap_or(("", tail)),
        };
        match std::env::var(name) {
            Ok(found) if !name.is_empty() => {
                out.push_str(&found);
                rest = after;
            }
            _ => {
                out.push_str(marker);
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}
