use std::{
    collections::hash_map::DefaultHasher,
    fs::{self, File, OpenOptions, TryLockError},
    hash::{Hash, Hasher},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::model::SavedState;

pub(crate) struct StateStore {
    directory: PathBuf,
    state: SavedState,
    warning: Option<String>,
    _lock: File,
}

impl StateStore {
    pub fn open(directory: &Path) -> Result<Self> {
        let directory = if directory.is_absolute() {
            directory.to_owned()
        } else {
            std::env::current_dir()?.join(directory)
        };
        // The lock lives outside the state directory so read-only recovery still
        // enforces one instance. Never unlink a lock inode while it may be held.
        let lock_directory =
            std::env::temp_dir().join(format!("rmusic-{}", nix::unistd::geteuid()));
        match fs::create_dir(&lock_directory) {
            Ok(()) => fs::set_permissions(&lock_directory, fs::Permissions::from_mode(0o700))?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error).context("Cannot create the instance lock directory"),
        }
        let metadata = fs::symlink_metadata(&lock_directory)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::geteuid().as_raw()
            || metadata.permissions().mode() & 0o077 != 0
        {
            bail!("Instance lock directory must be owned by you and have mode 700");
        }
        let creation_error = fs::create_dir_all(&directory).err();
        let mut hash = DefaultHasher::new();
        directory
            .canonicalize()
            .unwrap_or_else(|_| directory.clone())
            .hash(&mut hash);
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(lock_directory.join(format!("{:016x}.lock", hash.finish())))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                bail!("Another rmusic instance is using this state directory")
            }
            Err(TryLockError::Error(error)) => {
                return Err(error).context("Cannot lock saved state");
            }
        }
        let mut store = Self {
            directory,
            state: SavedState::default(),
            warning: None,
            _lock: lock,
        };
        if let Some(error) = creation_error {
            store.warning = Some(format!("Changes cannot be saved: {error}"));
        }
        match File::open(store.directory.join("state.json")) {
            Ok(file) => {
                let mut bytes = Vec::new();
                let state = file
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .context("Cannot read saved state")
                    .and_then(|_| {
                        if bytes.len() > 8 * 1024 * 1024 {
                            bail!("Saved state exceeds 8 MiB");
                        }
                        let state: SavedState = serde_json::from_slice(&bytes)?;
                        if state.version != 1 || state.volume > 100 {
                            bail!("Unsupported or invalid saved state");
                        }
                        Ok(state)
                    });
                match state {
                    Ok(state) => store.state = state,
                    Err(error) => {
                        store.warning = Some(format!(
                            "Changes cannot be saved: {error}; original preserved. Recover or move state.json aside explicitly."
                        ))
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                store.warning = Some(format!(
                    "Changes cannot be saved: {error}; original preserved"
                ))
            }
        }
        Ok(store)
    }

    pub fn state(&self) -> &SavedState {
        &self.state
    }

    pub fn warning(&self) -> Option<String> {
        self.warning.clone()
    }

    pub fn save(&mut self, state: &SavedState) -> Result<()> {
        if let Some(warning) = &self.warning {
            bail!("{warning}");
        }
        let result = (|| -> Result<()> {
            let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
            serde_json::to_writer_pretty(&mut temporary, state)?;
            temporary.write_all(b"\n")?;
            temporary.as_file().sync_all()?;
            temporary.persist(self.directory.join("state.json"))?;
            File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if let Err(error) = &result {
            self.warning = Some(format!("Changes cannot be saved: {error}"));
        }
        if result.is_ok() {
            self.state = state.clone();
        }
        result
    }
}
