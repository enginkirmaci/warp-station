//! The file list's tree: changed paths grouped into folders.
//!
//! The tree is built once per change snapshot and flattened into visible rows
//! when the fold set or the snapshot moves; a render walks the rows and never
//! the paths. A run of folders that each hold one child folder becomes one row
//! (`src/ui`), so a deep path does not cost a row per segment. Only the paths
//! it is given appear — this is a view of a change, not a repository explorer.

use std::collections::{BTreeMap, HashSet};

/// One visible row of a section's tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Row {
    /// A folder that holds changed files. `path` is the deepest folder the
    /// row's compacted label names, and the key the fold set uses.
    Folder {
        path: String,
        label: String,
        count: usize,
        depth: usize,
        folded: bool,
    },
    /// One changed file, `index` into the section's snapshot vector.
    File {
        index: usize,
        name: String,
        depth: usize,
    },
}

/// The cursor's row: a folder path or a file index. A rail keeps this
/// separate from its selection so folding never moves the diff.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cursor {
    Folder(String),
    File(usize),
}

impl Row {
    /// The cursor key for this row.
    pub fn cursor(&self) -> Cursor {
        match self {
            Row::Folder { path, .. } => Cursor::Folder(path.clone()),
            Row::File { index, .. } => Cursor::File(*index),
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Row::Folder { depth, .. } | Row::File { depth, .. } => *depth,
        }
    }

    /// The folder's path, when this is a folder row.
    pub fn folder_path(&self) -> Option<&str> {
        match self {
            Row::Folder { path, .. } => Some(path),
            Row::File { .. } => None,
        }
    }

    /// The file's index, when this is a file row.
    pub fn file_index(&self) -> Option<usize> {
        match self {
            Row::File { index, .. } => Some(*index),
            Row::Folder { .. } => None,
        }
    }
}

/// A section's tree of changed files.
pub struct Tree {
    root: Node,
}

#[derive(Default)]
struct Node {
    folders: BTreeMap<String, Node>,
    /// (name, index into the section's snapshot vector)
    files: Vec<(String, usize)>,
    /// Changed files at or below this folder.
    count: usize,
}

impl Tree {
    /// Groups `paths` (in snapshot order) into a tree. Every file keeps the
    /// index it came in with, so a row addresses the snapshot without a path
    /// lookup.
    pub fn build<'a>(paths: impl IntoIterator<Item = &'a str>) -> Self {
        let mut root = Node::default();
        for (index, path) in paths.into_iter().enumerate() {
            let mut segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            let Some(name) = segments.pop() else {
                continue;
            };
            let mut node = &mut root;
            node.count += 1;
            for segment in segments {
                node = node.folders.entry(segment.to_string()).or_default();
                node.count += 1;
            }
            node.files.push((name.to_string(), index));
        }
        Self { root }
    }

    pub fn is_empty(&self) -> bool {
        self.root.count == 0
    }

    /// Whether `path` is a folder row of this tree — the deepest folder of a
    /// single-child chain, which is the only path a row names.
    pub fn contains_folder(&self, path: &str) -> bool {
        let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        if segments.is_empty() {
            return false;
        }
        let mut node = &self.root;
        for segment in segments {
            let Some(child) = node.folders.get(segment) else {
                return false;
            };
            node = child;
        }
        node.count > 0 && (!node.files.is_empty() || node.folders.len() != 1)
    }

    /// Every file index at or below `path`, in tree order — the files a
    /// folder row's verbs act on.
    pub fn files_under(&self, path: &str) -> Vec<usize> {
        let mut node = &self.root;
        for segment in path.split('/').filter(|part| !part.is_empty()) {
            let Some(child) = node.folders.get(segment) else {
                return Vec::new();
            };
            node = child;
        }
        let mut files = Vec::with_capacity(node.count);
        node.collect_files(&mut files);
        files
    }

    /// The visible rows: each folder, then its files, with a folded folder's
    /// subtree left out. Folders sort before files, both case-insensitively.
    pub fn rows(&self, folded: &HashSet<String>) -> Vec<Row> {
        let mut rows = Vec::with_capacity(self.root.count * 2);
        self.root.push_children("", 0, folded, &mut rows);
        rows
    }
}

impl Node {
    fn collect_files(&self, out: &mut Vec<usize>) {
        let mut folders: Vec<&String> = self.folders.keys().collect();
        folders.sort_by_key(|name| name.to_lowercase());
        for name in folders {
            self.folders[name].collect_files(out);
        }
        out.extend(self.files.iter().map(|(_, index)| *index));
    }

    fn push_children(
        &self,
        path: &str,
        depth: usize,
        folded: &HashSet<String>,
        rows: &mut Vec<Row>,
    ) {
        let mut folders: Vec<&String> = self.folders.keys().collect();
        folders.sort_by_key(|name| name.to_lowercase());
        for name in folders {
            let child = &self.folders[name];
            child.push_folder(name, path, depth, folded, rows);
        }
        let mut files = self.files.clone();
        files.sort_by_key(|(name, _)| name.to_lowercase());
        for (name, index) in files {
            rows.push(Row::File { index, name, depth });
        }
    }

    fn push_folder(
        &self,
        name: &str,
        parent: &str,
        depth: usize,
        folded: &HashSet<String>,
        rows: &mut Vec<Row>,
    ) {
        let mut label = name.to_string();
        let mut path = join(parent, name);
        let mut node = self;
        // a run of single-child folders is one row; its deepest path is both
        // the label and the fold key
        while node.files.is_empty() && node.folders.len() == 1 {
            let (child_name, child) = node.folders.iter().next().expect("one child folder");
            label.push('/');
            label.push_str(child_name);
            path = join(&path, child_name);
            node = child;
        }
        let is_folded = folded.contains(&path);
        rows.push(Row::Folder {
            count: node.count,
            path: path.clone(),
            label,
            depth,
            folded: is_folded,
        });
        if is_folded {
            return;
        }
        node.push_children(&path, depth + 1, folded, rows);
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(paths: &[&str]) -> Vec<Row> {
        Tree::build(paths.iter().copied()).rows(&HashSet::new())
    }

    fn folder(row: &Row) -> (&str, &str, usize, usize) {
        match row {
            Row::Folder {
                path,
                label,
                count,
                depth,
                ..
            } => (path, label, *count, *depth),
            other => panic!("expected a folder row, got {other:?}"),
        }
    }

    fn file(row: &Row) -> (&str, usize, usize) {
        match row {
            Row::File { name, index, depth } => (name, *index, *depth),
            other => panic!("expected a file row, got {other:?}"),
        }
    }

    #[test]
    fn a_shared_folder_groups_its_files() {
        let rows = rows(&["src/ui/a.rs", "src/ui/b.rs"]);
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(folder(&rows[0]), ("src/ui", "src/ui", 2, 0));
        assert_eq!(file(&rows[1]), ("a.rs", 0, 1));
        assert_eq!(file(&rows[2]), ("b.rs", 1, 1));
    }

    #[test]
    fn a_single_child_chain_is_one_row() {
        let rows = rows(&["a/b/c/d.txt"]);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert_eq!(folder(&rows[0]), ("a/b/c", "a/b/c", 1, 0));
        assert_eq!(file(&rows[1]), ("d.txt", 0, 1));
    }

    #[test]
    fn a_chain_stops_where_a_folder_has_files() {
        let rows = rows(&["src/a.rs", "src/ui/b.rs"]);
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert_eq!(folder(&rows[0]), ("src", "src", 2, 0));
        // folders come before their files, so `ui` sits above `a.rs`
        assert_eq!(folder(&rows[1]), ("src/ui", "ui", 1, 1));
        assert_eq!(file(&rows[2]), ("b.rs", 1, 2));
        assert_eq!(file(&rows[3]), ("a.rs", 0, 1));
    }

    #[test]
    fn folders_come_before_files_and_sort_case_insensitively() {
        let rows = rows(&["b.txt", "A.txt", "Zed/x.txt", "alpha/y.txt"]);
        let order: Vec<String> = rows
            .iter()
            .map(|row| match row {
                Row::Folder { label, .. } => label.clone(),
                Row::File { name, .. } => name.clone(),
            })
            .collect();
        assert_eq!(
            order,
            ["alpha", "y.txt", "Zed", "x.txt", "A.txt", "b.txt"],
            "a folder's own files follow it before the next folder"
        );
    }

    #[test]
    fn folding_keeps_the_folder_row_and_hides_its_subtree() {
        let tree = Tree::build(["src/ui/a.rs", "src/ui/b.rs"]);
        let folded: HashSet<String> = ["src/ui".to_string()].into_iter().collect();
        let rows = tree.rows(&folded);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(folder(&rows[0]).0, "src/ui");
        assert!(matches!(rows[0], Row::Folder { folded: true, .. }));
    }

    #[test]
    fn counts_cover_nested_files() {
        let rows = rows(&["src/a.rs", "src/ui/b.rs", "src/ui/c.rs"]);
        assert_eq!(folder(&rows[0]).2, 3);
        assert_eq!(folder(&rows[1]).2, 2);
    }

    #[test]
    fn a_root_file_has_no_folder() {
        let rows = rows(&["README.md"]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(file(&rows[0]), ("README.md", 0, 0));
    }

    #[test]
    fn contains_folder_names_row_paths_only() {
        let tree = Tree::build(["src/ui/a.rs"]);
        assert!(tree.contains_folder("src/ui"));
        assert!(!tree.contains_folder("src"), "a chain root is not a row");
        assert!(!tree.contains_folder("src/ui/a.rs"));
        assert!(!tree.contains_folder(""));
    }

    #[test]
    fn a_row_names_its_cursor() {
        let rows = rows(&["src/ui/a.rs"]);
        assert_eq!(rows[0].cursor(), Cursor::Folder("src/ui".into()));
        assert_eq!(rows[1].cursor(), Cursor::File(0));
        assert_eq!(rows[0].folder_path(), Some("src/ui"));
        assert_eq!(rows[1].file_index(), Some(0));
        assert_eq!(rows[1].depth(), 1);
    }

    #[test]
    fn files_under_a_folder_row_cover_its_subtree() {
        let tree = Tree::build(["src/a.rs", "src/ui/b.rs", "top.txt"]);
        assert_eq!(
            tree.files_under("src"),
            vec![1, 0],
            "the nested folder's file comes before the folder's own"
        );
        assert_eq!(tree.files_under("src/ui"), vec![1]);
        assert!(tree.files_under("nope").is_empty());
    }

    #[test]
    fn a_file_row_keeps_its_snapshot_index() {
        let tree = Tree::build(["zeta.rs", "alpha.rs"]);
        let rows = tree.rows(&HashSet::new());
        let indices: Vec<usize> = rows
            .iter()
            .map(|row| match row {
                Row::File { index, .. } => *index,
                _ => panic!("only files here"),
            })
            .collect();
        assert_eq!(indices, [1, 0], "sorted by name, indexed by snapshot order");
    }
}
