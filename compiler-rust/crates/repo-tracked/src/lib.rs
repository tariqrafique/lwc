//! Shared listing of repo source files.
//!
//! Source-scanning gates (Rust tests, scripts, harness) must call this instead
//! of walking the filesystem themselves. In a git work tree that means
//! `git ls-files`. In a git-archive snapshot (official-board; no `.git`) it
//! means every file except fetched/build directories (`.oracle/`, `target/`,
//! `node_modules/`, `.git/`). Fetched inputs are never repo source.

use std::path::{Path, PathBuf};
use std::process::Command;

const SKIP_DIRS: &[&str] = &[".oracle", "target", "node_modules", ".git"];

/// Walk up from `start` to the workspace root (`Cargo.toml` containing `[workspace]`).
/// Works in a git checkout and in a git-archive snapshot that has no `.git`.
pub fn workspace_root(start: &Path) -> PathBuf {
    let mut p = start
        .canonicalize()
        .unwrap_or_else(|e| panic!("repo-tracked: cannot canonicalize {}: {e}", start.display()));
    loop {
        let cargo = p.join("Cargo.toml");
        if cargo.is_file() {
            let text = std::fs::read_to_string(&cargo).unwrap_or_default();
            if text.contains("[workspace]") {
                return p;
            }
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => panic!("repo-tracked: no workspace root above {}", start.display()),
        }
    }
}

fn is_git_work_tree(repo: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .ok()
        .is_some_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "true")
}

fn walk_archived(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("repo-tracked: cannot read {}: {e}", dir.display()));
    for e in entries {
        let e = e.unwrap_or_else(|err| {
            panic!(
                "repo-tracked: cannot read an entry of {}: {err}",
                dir.display()
            )
        });
        let p = e.path();
        let name = e.file_name();
        if p.is_dir() {
            if name.to_str().is_some_and(|n| SKIP_DIRS.contains(&n)) {
                continue;
            }
            walk_archived(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn archived_files(repo: &Path, pathspecs: &[&str]) -> Vec<PathBuf> {
    let specs: Vec<&str> = if pathspecs.is_empty() {
        vec!["."]
    } else {
        pathspecs.to_vec()
    };
    let mut out = Vec::new();
    for spec in specs {
        let base = if spec == "." {
            repo.to_path_buf()
        } else {
            repo.join(spec)
        };
        if base.is_file() {
            out.push(base);
        } else if base.is_dir() {
            walk_archived(&base, &mut out);
        }
    }
    out.sort();
    out
}

/// Every tracked path under `repo`, as absolute paths, in `git ls-files` order.
pub fn tracked_files(repo: &Path) -> Vec<PathBuf> {
    tracked_files_with(repo, &[])
}

/// Tracked paths under `repo` matching optional git pathspecs, as absolute paths.
pub fn tracked_files_with(repo: &Path, pathspecs: &[&str]) -> Vec<PathBuf> {
    if !is_git_work_tree(repo) {
        return archived_files(repo, pathspecs);
    }
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(["ls-files", "-z", "--"]);
    cmd.args(pathspecs);
    let out = cmd.output().unwrap_or_else(|e| {
        panic!(
            "repo-tracked: git ls-files failed under {}: {e}",
            repo.display()
        )
    });
    assert!(
        out.status.success(),
        "repo-tracked: git ls-files exited {:?} under {}\n{}",
        out.status.code(),
        repo.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
        .split(|b| *b == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| {
            let rel = String::from_utf8_lossy(chunk);
            repo.join(rel.as_ref())
        })
        .collect()
}

/// Tracked files matching `pathspecs` whose extension is in `exts` (no leading dot).
pub fn tracked_files_filtered(repo: &Path, pathspecs: &[&str], exts: &[&str]) -> Vec<PathBuf> {
    tracked_files_with(repo, pathspecs)
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e))
        })
        .collect()
}

/// Repo-relative path with forward slashes.
pub fn rel_slash(repo: &Path, file: &Path) -> String {
    file.strip_prefix(repo)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("repo-tracked-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            git(&p, &["init", "-q"]);
            git(&p, &["config", "user.email", "repo-tracked@test"]);
            git(&p, &["config", "user.name", "repo-tracked"]);
            fs::write(p.join(".gitignore"), ".oracle/\ntarget/\nnode_modules/\n").unwrap();
            fs::write(p.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
            git(&p, &["add", ".gitignore", "Cargo.toml"]);
            git(&p, &["commit", "-q", "-m", "init"]);
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn git(repo: &Path, args: &[&str]) {
        let st = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(st.success(), "git {args:?} failed in {}", repo.display());
    }

    #[test]
    fn lists_tracked_and_ignores_untracked_oracle() {
        let t = Scratch::new("oracle");
        fs::create_dir_all(t.path().join("crates")).unwrap();
        fs::write(t.path().join("crates/ok.rs"), "fn ok() {}\n").unwrap();
        git(t.path(), &["add", "crates/ok.rs"]);
        fs::create_dir_all(t.path().join(".oracle/lwc")).unwrap();
        fs::write(
            t.path().join(".oracle/lwc/plant.rs"),
            concat!(
                "const P = \"",
                "/work",
                "space/evil\";
"
            ),
        )
        .unwrap();
        fs::write(t.path().join("crates/untracked.rs"), "fn sneak() {}\n").unwrap();
        let files = tracked_files(t.path());
        let rels: Vec<String> = files.iter().map(|f| rel_slash(t.path(), f)).collect();
        assert!(rels.contains(&"crates/ok.rs".into()), "{rels:?}");
        assert!(rels.contains(&"Cargo.toml".into()), "{rels:?}");
        assert!(!rels.iter().any(|r| r.contains(".oracle")), "{rels:?}");
        assert!(
            !rels.iter().any(|r| r.ends_with("untracked.rs")),
            "{rels:?}"
        );
    }

    #[test]
    fn pathspec_and_extension_filters() {
        let t = Scratch::new("filter");
        fs::create_dir_all(t.path().join("crates/a")).unwrap();
        fs::create_dir_all(t.path().join("scripts")).unwrap();
        fs::write(t.path().join("crates/a/lib.rs"), "").unwrap();
        fs::write(t.path().join("crates/a/notes.md"), "").unwrap();
        fs::write(t.path().join("scripts/x.sh"), "").unwrap();
        git(t.path(), &["add", "crates", "scripts"]);
        let rs = tracked_files_filtered(t.path(), &["crates"], &["rs"]);
        let rels: Vec<String> = rs.iter().map(|f| rel_slash(t.path(), f)).collect();
        assert_eq!(rels, vec!["crates/a/lib.rs".to_string()]);
    }

    #[test]
    fn workspace_root_finds_archive_workspace() {
        let p =
            std::env::temp_dir().join(format!("repo-tracked-archive-root-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("crates/nested")).unwrap();
        fs::write(p.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        let found = workspace_root(&p.join("crates/nested"));
        assert_eq!(found, p.canonicalize().unwrap());
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn archive_snapshot_skips_oracle() {
        let p = std::env::temp_dir().join(format!("repo-tracked-archive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(p.join("crates")).unwrap();
        fs::create_dir_all(p.join(".oracle/lwc")).unwrap();
        fs::write(p.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        fs::write(p.join("crates/ok.rs"), "fn ok() {}\n").unwrap();
        fs::write(
            p.join(".oracle/lwc/plant.rs"),
            concat!("const P = \"", "/work", "space/evil\";\n"),
        )
        .unwrap();
        fs::write(p.join("crates/untracked.rs"), "fn sneak() {}\n").unwrap();
        let rels: Vec<String> = tracked_files(&p).iter().map(|f| rel_slash(&p, f)).collect();
        assert!(rels.contains(&"crates/ok.rs".into()), "{rels:?}");
        assert!(rels.contains(&"crates/untracked.rs".into()), "{rels:?}");
        assert!(!rels.iter().any(|r| r.contains(".oracle")), "{rels:?}");
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn workspace_root_finds_git_top() {
        let t = Scratch::new("root");
        fs::create_dir_all(t.path().join("crates/nested")).unwrap();
        let found = workspace_root(&t.path().join("crates/nested"));
        assert_eq!(found, t.path().canonicalize().unwrap());
    }
}
