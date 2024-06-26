use chrono::prelude::*;
use clap::Parser;
use env_logger::{Builder, Env, Target};
use log::{debug, error, info, warn};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

struct BackupVerifier {
    missing: HashSet<PathBuf>,
    corrupt: HashSet<PathBuf>,
    backup_dir: PathBuf,
    backup_time: chrono::DateTime<chrono::FixedOffset>,
    source_dir: PathBuf,
    id: String,
    excludes: Vec<String>,
    relative_path: bool,
    max_age: Option<humantime::Duration>,
}

impl BackupVerifier {
    fn new(relative_path: bool, max_age: Option<humantime::Duration>) -> BackupVerifier {
        BackupVerifier {
            missing: HashSet::new(),
            corrupt: HashSet::new(),
            backup_dir: PathBuf::new(),
            backup_time: chrono::Local::now().fixed_offset(), // Placeholder, actual value would be set later
            source_dir: PathBuf::new(),
            id: String::new(), // Restic snapshot id
            excludes: Vec::new(),
            relative_path,
            max_age,
        }
    }

    fn excluded(&self, file: &Path) -> bool {
        // TODO: Implement efficient check for exclusion
        // A binary search could be implemented here if `self.excludes` is sorted
        // TODO: Match restic's behavior, not just starts_with? but some globbing + extra magic
        self.excludes
            .iter()
            .any(|exclude| file.starts_with(exclude))
    }

    fn sha256(&self, path: &Path) -> io::Result<[u8; 32]> {
        let mut file = fs::File::open(path)?;
        let mut hasher = Sha256::new();
        io::copy(&mut file, &mut hasher)?;
        let hash = hasher.finalize();
        Ok(hash.into())
    }

    // Verify the source file against the backup
    fn verify_source_file(&mut self, file: &Path) -> io::Result<()> {
        // Relative paths restore right into the temporary directory, but in the snapshot metadata
        // there is an absolute path.
        // Use --relative-path (or -r) to remove the leading path components.
        let relative_file = if self.relative_path {
            file.strip_prefix(self.source_dir.as_path())
                .expect("Could not strip prefix")
        } else {
            // If file is an absolute Path we need to strip the leading slash, otherwise
            // backup_dir.join(file) will return file, instead of the joined paths.
            // See https://doc.rust-lang.org/std/path/struct.Path.html#method.join.
            file.strip_prefix("/").unwrap_or(file)
        };
        let counterpart = self.backup_dir.join(relative_file);

        let file_metadata = fs::metadata(file)?;
        let file_birthtime = file_metadata.created()?;

        if counterpart.is_file() {
            let counterpart_metadata = fs::metadata(&counterpart)?;
            let counterpart_modified = counterpart_metadata.modified()?;
            let file_modified = file_metadata.modified()?;

            // Check if the modified times are the same
            if file_modified == counterpart_modified {
                // Compare file contents
                let file_sha256 = self.sha256(file)?;
                let counterpart_sha256 = self.sha256(&counterpart)?;

                if file_sha256 == counterpart_sha256 {
                    debug!("Same content in backup: {}", file.display());
                } else {
                    warn!(
                        "Same modified timestamp but different content in backup: {}",
                        file.display()
                    );
                    self.corrupt.insert(file.to_path_buf());
                }
            }
        } else if file_birthtime <= self.backup_time.into() {
            debug!("Missing in backup: {}", file.display());
            self.missing.insert(file.to_path_buf());
        } else {
            debug!("Not in backup (too new): {}", file.display());
        }

        Ok(())
    }

    fn load_excludes(&self, excludes_file: PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
        let file_contents = fs::read_to_string(excludes_file);
        match file_contents {
            Ok(contents) => Ok(contents.lines().map(String::from).collect::<Vec<String>>()),
            // If the file doesn't exist, return an empty list because no exclude file means no excludes
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn main(&mut self) -> Result<(), Box<dyn Error>> {
        let snapshot_info = Command::new("restic")
            .args(["snapshots", "--json", "--latest", "1"]) // Get metadata for the latest 1 snapshot
            .output()?;

        if snapshot_info.stdout.is_empty() {
            return Err(
                "Couldn't find any snapshots. Did you set RESTIC_REPOSITORY and RESTIC_PASSWORD? Is restic installed?"
                    .into(),
            );
        }

        let snapshot: Value = serde_json::from_slice(&snapshot_info.stdout)?;
        let snapshot = snapshot.get(0).ok_or("No snapshot data available")?;

        self.backup_time = snapshot["time"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .ok_or("Invalid snapshot time")?;
        self.id = snapshot["id"]
            .as_str()
            .map(String::from)
            .ok_or("Invalid snapshot id")?;
        self.source_dir = snapshot["paths"][0]
            .as_str()
            .map(PathBuf::from)
            .ok_or("Invalid source directory")?;

        if !self.source_dir.is_dir() {
            return Err(format!("Couldn't find source directory {:?}", self.source_dir).into());
        }

        // Check if the backup is too old
        if let Some(max_age) = self.max_age {
            let now = chrono::Local::now().fixed_offset();
            let seconds_since_backup = (now - self.backup_time).num_seconds();
            if seconds_since_backup > max_age.as_secs().try_into()? {
                return Err("Backup is too old".into());
            }
        }

        // Load excludes from ~/.backup_exclude
        let home_dir = dirs::home_dir().ok_or("Could not find home directory")?;
        let excludes_file = home_dir.join(".backup_exclude");
        self.excludes = self.load_excludes(excludes_file)?;

        // Log some information about the snapshot
        Command::new("restic")
            .arg("stats")
            .arg(&self.id)
            .status()
            .expect("Failed to execute restic stats");

        let temp_dir = tempfile::TempDir::with_prefix("bacify-")?;
        self.backup_dir = temp_dir.path().to_owned();
        self.restore()?;
        self.verify()?;

        self.verdict()
    }

    fn restore(&self) -> Result<(), Box<dyn Error>> {
        Command::new("restic")
            .args([
                "restore",
                &self.id,
                "--target",
                self.backup_dir.to_str().ok_or("Invalid backup directory")?,
            ])
            .status()?;
        Ok(())
    }

    fn verify(&mut self) -> io::Result<()> {
        for entry in WalkDir::new(&self.source_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let file_path = entry.path();
            if self.excluded(file_path) {
                continue;
            }

            self.verify_source_file(file_path)?;
        }
        Ok(())
    }

    fn verdict(&self) -> Result<(), Box<dyn Error>> {
        let mut result = Ok(());

        if !self.missing.is_empty() {
            warn!("Missing files that should be in the backup, the backup was created after the files were:");
            for file in &self.missing {
                warn!("{}", file.display());
            }
            result = Err("Verification failed".into());
        }

        if !self.corrupt.is_empty() {
            warn!("Changed files found that have the same modified time:");
            for file in &self.corrupt {
                warn!("{}", file.display());
            }
            result = Err("Verification failed".into());
        }
        result
    }
}

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    relative_path: bool,

    #[arg(short, long)]
    max_age: Option<humantime::Duration>,
}

fn main() {
    // Set the default logging level to info, if not set via LOG_LEVEL
    Builder::from_env(Env::default().filter_or("LOG_LEVEL", "info"))
        .target(Target::Stdout)
        .init();

    let args = Args::parse();

    // We want to see some output during restore, needs at least restic version 0.16.0
    if std::env::var_os("RESTIC_PROGRESS_FPS").is_none() {
        std::env::set_var("RESTIC_PROGRESS_FPS", "0.5");
    }

    let mut verifier = BackupVerifier::new(args.relative_path, args.max_age);
    match verifier.main() {
        Err(e) => {
            error!("Error: {}", e);
            std::process::exit(1)
        }
        Ok(()) => info!("Verification succeeded."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_load_excludes_with_invalid_utf8() -> io::Result<()> {
        let temp_dir = tempfile::TempDir::with_prefix("bacify-test-")?;
        let exclude_file_path = temp_dir.path().join(".backup_exclude");
        let mut file = File::create(&exclude_file_path)?;
        file.write_all(&[0xff, 0xfe, 0xfd])?; // Invalid UTF-8 sequence

        let verifier = BackupVerifier::new(true, None);

        let result = verifier.load_excludes(exclude_file_path);
        assert!(result.is_err());
        // TODO: How can I test that the correct error is returned?

        Ok(())
    }

    #[test]
    fn test_load_excludes_without_file() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempfile::TempDir::with_prefix("bacify-test-")?;
        let exclude_file_path = temp_dir.path().join("nonexistent_file");

        let verifier = BackupVerifier::new(true, None);

        let result = verifier.load_excludes(exclude_file_path)?;
        assert!(result.is_empty());

        Ok(())
    }

    #[test]
    fn test_excluded_exact_match() {
        let mut verifier = BackupVerifier::new(false, None);
        verifier.excludes.push("/home/user/exclude_this".into());
        assert!(verifier.excluded(Path::new("/home/user/exclude_this")));
    }

    #[test]
    fn test_excluded_starts_with_match() {
        let mut verifier = BackupVerifier::new(false, None);
        verifier.excludes.push("/home/user/exclude".into());
        assert!(verifier.excluded(Path::new("/home/user/exclude/subdir")));
    }

    #[test]
    fn test_not_excluded_no_match() {
        let mut verifier = BackupVerifier::new(false, None);
        verifier.excludes.push("/home/user/exclude".into());
        assert!(!verifier.excluded(Path::new("/home/user/include")));
    }

    #[test]
    fn test_not_excluded_partial_match() {
        let mut verifier = BackupVerifier::new(false, None);
        verifier.excludes.push("/home/user/exclude".into());
        assert!(!verifier.excluded(Path::new("/home/user/exclude_this")));
    }
}
