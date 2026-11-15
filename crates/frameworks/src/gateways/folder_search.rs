//! [`FileSearchGateway`] for Find in Files: lists the files on a background thread, then reads
//! and searches them on as many threads as the machine has, sending the results in path order.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use entities::find_in_files::{search_bytes, search_text, FileHits, PathFilter};
use use_cases::gateways::{FileSearchEvent, FileSearchGateway, FileSearchRequest};

type Waker = Arc<dyn Fn() + Send + Sync>;

struct Running {
    events: Receiver<FileSearchEvent>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct FolderSearch {
    waker: Option<Waker>,
    running: Option<Running>,
}

impl FolderSearch {
    /// `waker` runs whenever the search finds something, so the window shows it at once.
    pub fn with_waker(mut self, waker: Waker) -> Self {
        self.waker = Some(waker);
        self
    }
}

impl FileSearchGateway for FolderSearch {
    fn start(&mut self, request: FileSearchRequest) {
        self.cancel();
        let (sender, events) = channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let waker = self.waker.clone();
        std::thread::spawn(move || search(request, &sender, &stop, waker.as_ref()));
        self.running = Some(Running { events, cancelled });
    }

    fn poll(&mut self) -> Vec<FileSearchEvent> {
        let Some(running) = &self.running else {
            return Vec::new();
        };
        let events: Vec<FileSearchEvent> = running.events.try_iter().collect();
        if events.contains(&FileSearchEvent::Done) {
            self.running = None;
        }
        events
    }

    fn cancel(&mut self) {
        if let Some(running) = self.running.take() {
            running.cancelled.store(true, Ordering::Relaxed);
        }
    }
}

fn search(
    request: FileSearchRequest,
    sender: &Sender<FileSearchEvent>,
    cancelled: &AtomicBool,
    waker: Option<&Waker>,
) {
    let send = |event: FileSearchEvent| {
        let _ = sender.send(event);
        if let Some(wake) = waker {
            wake();
        }
    };
    let files = list_files(&request.roots, &request.filter, cancelled);
    if cancelled.load(Ordering::Relaxed) {
        return;
    }
    send(FileSearchEvent::Listed(files.len()));
    let Ok(Some(pattern)) = request.options.pattern() else {
        send(FileSearchEvent::Done);
        return;
    };
    let open: HashMap<&Path, &str> = request
        .open_texts
        .iter()
        .map(|(path, text)| (path.as_path(), text.as_str()))
        .collect();

    // Workers take files by index; the results go out in that order.
    let next = AtomicUsize::new(0);
    let found: Mutex<BTreeMap<usize, Option<FileSearchEvent>>> = Mutex::default();
    let sent = Mutex::new(0usize);
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                if cancelled.load(Ordering::Relaxed) {
                    return;
                }
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(index) else {
                    return;
                };
                let event = search_file(path, &pattern, &request, &open);
                found.lock().expect("not poisoned").insert(index, event);
                // Sends what is ready in order.
                let mut sent = sent.lock().expect("not poisoned");
                let mut found = found.lock().expect("not poisoned");
                while let Some(event) = found.remove(&*sent) {
                    *sent += 1;
                    if let Some(event) = event {
                        if !cancelled.load(Ordering::Relaxed) {
                            send(event);
                        }
                    }
                }
            });
        }
    });
    if !cancelled.load(Ordering::Relaxed) {
        send(FileSearchEvent::Done);
    }
}

fn search_file(
    path: &Path,
    pattern: &regex::Regex,
    request: &FileSearchRequest,
    open: &HashMap<&Path, &str>,
) -> Option<FileSearchEvent> {
    let (lines, matches) = if let Some(text) = open.get(path) {
        search_text(pattern, text, request.context_lines)
    } else {
        let too_big = request.max_file_size > 0
            && std::fs::metadata(path).is_ok_and(|m| m.len() > request.max_file_size);
        if too_big {
            return None;
        }
        match std::fs::read(path) {
            Ok(bytes) => search_bytes(pattern, &bytes, request.context_lines)?,
            Err(error) => {
                return Some(FileSearchEvent::Failed(
                    path.to_path_buf(),
                    error.to_string(),
                ))
            }
        }
    };
    (matches > 0).then(|| {
        FileSearchEvent::Found(FileHits {
            path: path.to_path_buf(),
            lines,
            matches,
        })
    })
}

/// The files under the roots that the filter accepts, each once, sorted within each root.
fn list_files(roots: &[PathBuf], filter: &PathFilter, cancelled: &AtomicBool) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if filter.accepts_file(root) && seen.insert(root.clone()) {
                files.push(root.clone());
            }
            continue;
        }
        let excluded = filter.clone();
        let walker = ignore::WalkBuilder::new(root)
            .standard_filters(false)
            .follow_links(true)
            .sort_by_file_path(|a, b| a.cmp(b))
            .filter_entry(move |entry| {
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                entry.depth() == 0 || !is_dir || !excluded.skips_folder(entry.path())
            })
            .build();
        for entry in walker.flatten() {
            if cancelled.load(Ordering::Relaxed) {
                return files;
            }
            let path = entry.path();
            if entry.file_type().is_some_and(|t| t.is_file())
                && filter.accepts_file(path)
                && seen.insert(path.to_path_buf())
            {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}
