//! Which programs have files open, through `lsof`. A folder in use is never moved.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The files open by the developer's programs, with the program's name.
#[derive(Debug, Default)]
pub struct OpenFiles {
    files: Vec<(String, PathBuf)>,
}

impl OpenFiles {
    /// `None` when `lsof` cannot answer: the caller then knows nothing about use.
    pub fn snapshot() -> Option<OpenFiles> {
        let output = Command::new("lsof")
            .args(["-n", "-P", "-Fcn"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        // lsof exits with 1 when some files could not be listed, and still lists the rest.
        if output.stdout.is_empty() {
            return None;
        }
        Some(OpenFiles::parse(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Reads the `-F` output: `p` starts a process, `c` names it, `n` is an open file.
    pub fn parse(text: &str) -> OpenFiles {
        let mut program = String::new();
        let mut files = Vec::new();
        for line in text.lines() {
            match line.split_at_checked(1) {
                Some(("c", name)) => program = name.to_string(),
                Some(("n", name)) if name.starts_with('/') => {
                    files.push((program.clone(), PathBuf::from(name)))
                }
                _ => {}
            }
        }
        OpenFiles { files }
    }

    /// A program with a file open inside `folder`.
    pub fn user_of(&self, folder: &Path) -> Option<&str> {
        self.files
            .iter()
            .find(|(_, file)| file.starts_with(folder))
            .map(|(program, _)| program.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lsof_fields() {
        let open = OpenFiles::parse(
            "p148\ncloginwindow\nfcwd\nn/\np900\nccargo\nftxt\nn/w/app/target/debug/.cargo-lock\nn[pipe]\n",
        );
        assert_eq!(open.user_of(Path::new("/w/app/target")), Some("cargo"));
        assert_eq!(open.user_of(Path::new("/w/other")), None);
    }
}
