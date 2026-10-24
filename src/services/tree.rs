//! File tree - Directory tree for sidebar

use std::path::{Path, PathBuf};
use ignore::Walk;

use crate::ui::sidebar::{TreeNode, FileIcon};

/// Build a tree from a directory path
pub fn build_tree(dir_path: &Path) -> TreeNode {
    let mut root = TreeNode::new(dir_path.to_path_buf());
    root.expanded = true;
    root.icon = FileIcon::FolderOpen;
    root.children = build_children(dir_path, 0);
    root.set_depth(0);
    root
}

fn build_children(dir_path: &Path, depth: usize) -> Vec<TreeNode> {
    let mut children = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir_path) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = entry.metadata();

            if let Ok(metadata) = metadata {
                // Skip hidden files unless it's a git directory
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with('.') && !name_str.starts_with(".git") {
                        continue;
                    }
                }

                if metadata.is_dir() {
                    dirs.push(path);
                } else if metadata.is_file() {
                    files.push(path);
                }
            }
        }

        // Sort: directories first, then files
        dirs.sort();
        files.sort();

        for dir in dirs {
            let mut node = TreeNode::new(dir.clone());
            node.expanded = depth < 2; // Expand only first 2 levels by default
            node.set_depth(depth);
            node.children = build_children(&dir, depth + 1);
            node.icon = if node.expanded { FileIcon::FolderOpen } else { FileIcon::Folder };
            children.push(node);
        }

        for file in files {
            let mut node = TreeNode::new(file.clone());
            node.set_depth(depth);
            node.icon = FileIcon::File;

            // Determine file icon based on extension
            if let Some(ext) = file.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                match ext_str.as_str() {
                    "rs" | "py" | "js" | "ts" | "cpp" | "c" | "h" | "hpp" | "java" | "go" => {
                        // Source file
                    }
                    "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "css" => {
                        // Config file
                    }
                    "md" | "txt" => {
                        // Text file
                    }
                    _ => {}
                }
            }

            children.push(node);
        }
    }

    children
}

/// Filter tree nodes matching a search query
pub fn filter_nodes<'a>(node: &'a TreeNode, query: &str) -> Vec<&'a TreeNode> {
    let mut results = Vec::new();
    filter_recursive(node, query, &mut results);
    results
}

fn filter_recursive<'a>(node: &'a TreeNode, query: &str, results: &mut Vec<&'a TreeNode>) {
    let name_lower = node.name.to_lowercase();
    let query_lower = query.to_lowercase();

    if name_lower.contains(&query_lower) {
        results.push(node);
        return;
    }

    if node.expanded {
        for child in &node.children {
            filter_recursive(child, query, results);
        }
    }
}
