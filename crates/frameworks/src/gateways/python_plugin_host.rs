//! [`PluginHostGateway`] that runs `assets/python/plugin_host.py` in a Python 3 interpreter and
//! talks to it over stdin/stdout, one JSON object per line (the protocol is described in
//! `plugin_host.py` and `docs/plugins.md`). Like Sublime's own plugin host, plugins live in their
//! own process: a crash or an endless loop stops the host, not the editor.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use entities::document::DocumentId;
use serde_json::{json, Value};
use use_cases::gateways::{PluginCommandKind, PluginHostGateway, PluginLoad, PluginMessage};

/// The host and our `sublime` modules, written to `lib_dir` before the host starts.
const SUPPORT_FILES: &[(&str, &str)] = &[
    ("plugin_host.py", include_str!("../../../../assets/python/plugin_host.py")),
    ("codelibre_rpc.py", include_str!("../../../../assets/python/codelibre_rpc.py")),
    ("sublime_api.py", include_str!("../../../../assets/python/sublime_api.py")),
    ("sublime.py", include_str!("../../../../assets/python/sublime.py")),
    ("sublime_plugin.py", include_str!("../../../../assets/python/sublime_plugin.py")),
];

/// How long the editor waits for a plugin (the window does not respond meanwhile).
const TIMEOUT: Duration = Duration::from_secs(20);
/// After the host exits, how long to wait for the rest of what it wrote to stderr.
const EXIT_GRACE: Duration = Duration::from_millis(300);

/// Windows: start the interpreter without opening a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

enum Line {
    Message(Value),
    Stderr(String),
    Closed,
}

struct HostProcess {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<Line>,
}

pub struct PythonPluginHost {
    lib_dir: PathBuf,
    process: Option<HostProcess>,
}

impl PythonPluginHost {
    /// `lib_dir`: where the support modules are written (Sublime's `Lib/python` equivalent).
    pub fn new(lib_dir: PathBuf) -> Self {
        Self { lib_dir, process: None }
    }

    fn start(&mut self, interpreter: Option<&str>) -> Result<(), String> {
        self.stop();
        std::fs::create_dir_all(&self.lib_dir).map_err(|e| format!("{}: {e}", self.lib_dir.display()))?;
        for (name, contents) in SUPPORT_FILES {
            let path = self.lib_dir.join(name);
            std::fs::write(&path, contents).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let python = find_interpreter(interpreter)?;
        let mut child = command(&python)
            .arg("-I")
            .arg("-u")
            .arg(self.lib_dir.join("plugin_host.py"))
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", python.join(" ")))?;
        let (sender, lines) = mpsc::channel();
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let stdin = child.stdin.take().expect("piped stdin");
        spawn_reader(stdout, sender.clone(), true);
        spawn_reader(stderr, sender, false);
        self.process = Some(HostProcess { child, stdin, lines });
        Ok(())
    }

    fn send(&mut self, message: Value) -> Result<(), String> {
        let process = self.process.as_mut().ok_or("the plugin host is not running (Preferences > Reload Packages)")?;
        let written = writeln!(process.stdin, "{message}").and_then(|()| process.stdin.flush());
        if let Err(e) = written {
            self.stop();
            return Err(format!("the plugin host stopped: {e}"));
        }
        Ok(())
    }
}

impl Drop for PythonPluginHost {
    fn drop(&mut self) {
        self.stop();
    }
}

impl PluginHostGateway for PythonPluginHost {
    fn load(&mut self, request: &PluginLoad) -> Result<(), String> {
        self.start(request.interpreter.as_deref())?;
        let data = request.packages_path.parent().map(PathBuf::from).unwrap_or_default();
        let executable = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
        self.send(json!({
            "op": "load",
            "packages": request.packages.iter().map(|p| json!({
                "name": p.name,
                "dirs": p.dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "paths": {
                "packages": request.packages_path.display().to_string(),
                "installed_packages": data.join("Installed Packages").display().to_string(),
                "cache": data.join("Cache").display().to_string(),
                "executable": executable,
            },
        }))
    }

    fn run(&mut self, kind: PluginCommandKind, name: &str, args: &Value, view: Option<DocumentId>) -> Result<(), String> {
        let kind = match kind {
            PluginCommandKind::Text => "text",
            PluginCommandKind::Window => "window",
            PluginCommandKind::Application => "application",
        };
        self.send(json!({ "op": "run", "kind": kind, "name": name, "args": args, "view": view, "window": 1 }))
    }

    fn reply(&mut self, result: Result<Value, String>) -> Result<(), String> {
        self.send(match result {
            Ok(value) => json!({ "op": "result", "value": value }),
            Err(error) => json!({ "op": "result", "error": error }),
        })
    }

    fn receive(&mut self) -> Result<PluginMessage, String> {
        let process = self.process.as_mut().ok_or("the plugin host is not running (Preferences > Reload Packages)")?;
        match process.lines.recv_timeout(TIMEOUT) {
            Ok(Line::Message(value)) => parse_message(&value),
            Ok(Line::Stderr(text)) => Ok(PluginMessage::Log(text + "\n")),
            Ok(Line::Closed) | Err(RecvTimeoutError::Disconnected) => {
                // Whatever the host printed before exiting explains why.
                let mut output = String::new();
                while let Ok(Line::Stderr(text)) = process.lines.recv_timeout(EXIT_GRACE) {
                    output.push_str(&text);
                    output.push('\n');
                }
                self.stop();
                Err(format!("the plugin host stopped\n{output}"))
            }
            Err(RecvTimeoutError::Timeout) => {
                self.stop();
                Err(format!("a plugin did not answer in {} seconds; the plugin host was stopped", TIMEOUT.as_secs()))
            }
        }
    }

    fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = writeln!(process.stdin, r#"{{"op": "exit"}}"#);
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }
}

/// Reads lines in the background: JSON messages from stdout, plain text from stderr.
fn spawn_reader(stream: impl Read + Send + 'static, sender: Sender<Line>, is_stdout: bool) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let message = match serde_json::from_str::<Value>(&line) {
                Ok(value) if is_stdout => Line::Message(value),
                _ => Line::Stderr(line),
            };
            if sender.send(message).is_err() {
                return;
            }
        }
        if is_stdout {
            let _ = sender.send(Line::Closed);
        }
    });
}

fn parse_message(value: &Value) -> Result<PluginMessage, String> {
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(String::from);
    match value.get("op").and_then(Value::as_str) {
        Some("call") => Ok(PluginMessage::Call {
            function: text("fn").unwrap_or_default(),
            args: value.get("args").and_then(Value::as_array).cloned().unwrap_or_default(),
        }),
        Some("log") => Ok(PluginMessage::Log(text("text").unwrap_or_default())),
        Some("done") => Ok(PluginMessage::Done { error: text("error") }),
        Some("loaded") => {
            let commands = value
                .get("commands")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    let kind = match c.get(1)?.as_str()? {
                        "text" => PluginCommandKind::Text,
                        "window" => PluginCommandKind::Window,
                        _ => PluginCommandKind::Application,
                    };
                    Some((c.get(0)?.as_str()?.to_string(), kind))
                })
                .collect();
            let errors = value
                .get("errors")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(String::from))
                .collect();
            Ok(PluginMessage::Loaded { commands, errors })
        }
        _ => Err(format!("unexpected message from the plugin host: {value}")),
    }
}

/// A Python 3 interpreter, as a program and its first arguments: the `python_interpreter`
/// setting, else the first of `py -3`, `python3`, `python` and `uv python find` that works.
pub fn find_interpreter(setting: Option<&str>) -> Result<Vec<String>, String> {
    if let Some(path) = setting {
        let python = vec![path.to_string()];
        return if is_python3(&python) {
            Ok(python)
        } else {
            Err(format!("\"python_interpreter\": {path} is not a Python 3 interpreter"))
        };
    }
    let candidates = [vec!["py", "-3"], vec!["python3"], vec!["python"]];
    for candidate in candidates {
        let python: Vec<String> = candidate.into_iter().map(String::from).collect();
        if is_python3(&python) {
            return Ok(python);
        }
    }
    let found_by_uv = command(&["uv".to_string(), "python".into(), "find".into()])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|path| !path.is_empty());
    if let Some(path) = found_by_uv {
        if is_python3(std::slice::from_ref(&path)) {
            return Ok(vec![path]);
        }
    }
    Err("no Python 3 interpreter found: install Python 3 or set \"python_interpreter\" in Preferences".into())
}

/// Runs the interpreter once. (The Windows Store's `python.exe` placeholder fails here.)
fn is_python3(python: &[String]) -> bool {
    command(python)
        .args(["-c", "import sys; print(sys.version_info[0])"])
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "3")
}

fn command(program_and_args: &[String]) -> Command {
    let mut command = Command::new(&program_and_args[0]);
    command.args(&program_and_args[1..]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
