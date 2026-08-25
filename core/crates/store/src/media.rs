//! The content-addressed media library under `data/media/`.
//!
//! Layout (README Part 3): `data/media/{hash[:2]}/{hash}.webp|.ogg`. Writes go
//! to a staging file first, are hashed, then atomically renamed into place, and
//! only afterwards does the `media_files` row get committed (README Part 4
//! §"媒体写入"). Re-storing identical bytes is a no-op, so the same photo
//! fetched for two words costs one file.
//!
//! Nothing here ever deletes: media GC only ever stamps `gc_eligible_at`.

use std::path::{Path, PathBuf};

use morpho_domain::hash::file_hash;
use morpho_domain::types::MediaKind;

use crate::error::{Result, StoreError};

/// Subdirectory of `data/` holding the library.
pub const MEDIA_DIR: &str = "media";
/// Subdirectory of `data/` used for adapter `out_path` staging.
pub const STAGING_DIR: &str = "tmp";

/// A file that has been placed in the library and is ready to be registered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMedia {
    pub file_hash: String,
    pub kind: MediaKind,
    /// Path relative to the data directory, exactly as stored in `media_files`.
    pub rel_path: String,
    pub bytes: i64,
    /// False when the identical bytes were already in the library.
    pub created: bool,
}

/// Handle on `data/media` plus the staging area adapters write into.
#[derive(Debug, Clone)]
pub struct MediaStore {
    data_dir: PathBuf,
}

impl MediaStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `data/media`.
    pub fn root(&self) -> PathBuf {
        self.data_dir.join(MEDIA_DIR)
    }

    /// Relative path of a hash, e.g. `media/ab/abcdef….webp`.
    pub fn rel_path(file_hash: &str, kind: MediaKind) -> String {
        let shard = &file_hash[..2.min(file_hash.len())];
        format!("{MEDIA_DIR}/{shard}/{file_hash}.{}", kind.extension())
    }

    /// Absolute path of a stored file.
    pub fn path_of(&self, file_hash: &str, kind: MediaKind) -> PathBuf {
        self.data_dir.join(Self::rel_path(file_hash, kind))
    }

    /// Create a staging directory an adapter may write its `out_path` into.
    ///
    /// morphod owns this directory: it creates it before spawning and removes
    /// it after handling the result (adapter-protocol.md wave-2 ruling #1).
    pub fn staging(&self, token: &str) -> Result<StagingDir> {
        let path = self.data_dir.join(STAGING_DIR).join(token);
        std::fs::create_dir_all(&path)?;
        Ok(StagingDir { path })
    }

    /// Remove every leftover staging directory. Called by the janitor at boot:
    /// a crash mid-synthesis leaves temporary files and nothing else.
    pub fn clean_staging(&self) -> Result<usize> {
        let root = self.data_dir.join(STAGING_DIR);
        if !root.is_dir() {
            return Ok(0);
        }
        let mut removed = 0;
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            let outcome = if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            match outcome {
                Ok(()) => removed += 1,
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "stale staging entry survived")
                }
            }
        }
        Ok(removed)
    }

    /// Hash `bytes` and place them in the library.
    pub fn put_bytes(&self, bytes: &[u8], kind: MediaKind) -> Result<StoredMedia> {
        let hash = file_hash(bytes);
        let rel_path = Self::rel_path(&hash, kind);
        let target = self.data_dir.join(&rel_path);
        if target.exists() {
            return Ok(StoredMedia {
                file_hash: hash,
                kind,
                rel_path,
                bytes: bytes.len() as i64,
                created: false,
            });
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Stage inside the destination shard so the rename is same-filesystem
        // and therefore atomic.
        let staging = target.with_extension(format!("{}.part", kind.extension()));
        std::fs::write(&staging, bytes)?;
        std::fs::rename(&staging, &target)?;
        Ok(StoredMedia {
            file_hash: hash,
            kind,
            rel_path,
            bytes: bytes.len() as i64,
            created: true,
        })
    }

    /// Move a file an adapter produced into the library.
    pub fn put_file(&self, source: &Path, kind: MediaKind) -> Result<StoredMedia> {
        let bytes = std::fs::read(source).map_err(|err| {
            StoreError::Io(std::io::Error::new(
                err.kind(),
                format!("reading adapter output {}: {err}", source.display()),
            ))
        })?;
        if bytes.is_empty() {
            return Err(StoreError::invalid(format!(
                "adapter produced an empty file at {}",
                source.display()
            )));
        }
        self.put_bytes(&bytes, kind)
    }

    /// True when the library actually holds the bytes for this hash.
    pub fn contains(&self, file_hash: &str, kind: MediaKind) -> bool {
        self.path_of(file_hash, kind).is_file()
    }
}

/// A morphod-owned scratch directory handed to one adapter invocation.
#[derive(Debug)]
pub struct StagingDir {
    path: PathBuf,
}

impl StagingDir {
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path to hand to an adapter as `out_path`.
    pub fn out_path(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.path.display(), error = %err, "failed to clean staging dir");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, MediaStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = MediaStore::new(dir.path());
        (dir, store)
    }

    #[test]
    fn rel_path_shards_by_hash_prefix() {
        let hash = "ab12".to_string() + &"0".repeat(60);
        assert_eq!(
            MediaStore::rel_path(&hash, MediaKind::Image),
            format!("media/ab/{hash}.webp")
        );
        assert_eq!(
            MediaStore::rel_path(&hash, MediaKind::Audio),
            format!("media/ab/{hash}.ogg")
        );
    }

    #[test]
    fn put_bytes_is_content_addressed_and_idempotent() {
        let (_dir, store) = store();
        let first = store.put_bytes(b"RIFFfake-webp", MediaKind::Image).unwrap();
        assert!(first.created);
        assert_eq!(
            first.file_hash,
            morpho_domain::hash::file_hash(b"RIFFfake-webp")
        );
        assert!(store.contains(&first.file_hash, MediaKind::Image));

        let second = store.put_bytes(b"RIFFfake-webp", MediaKind::Image).unwrap();
        assert!(!second.created, "identical bytes must dedupe");
        assert_eq!(first.file_hash, second.file_hash);
        assert_eq!(first.rel_path, second.rel_path);
    }

    #[test]
    fn different_bytes_get_different_files() {
        let (_dir, store) = store();
        let a = store.put_bytes(b"one", MediaKind::Audio).unwrap();
        let b = store.put_bytes(b"two", MediaKind::Audio).unwrap();
        assert_ne!(a.file_hash, b.file_hash);
        assert_ne!(a.rel_path, b.rel_path);
    }

    #[test]
    fn put_file_moves_adapter_output_into_the_library() {
        let (dir, store) = store();
        let source = dir.path().join("adapter-out.ogg");
        std::fs::write(&source, b"OggS...").unwrap();
        let stored = store.put_file(&source, MediaKind::Audio).unwrap();
        assert!(stored.created);
        let read_back = std::fs::read(store.path_of(&stored.file_hash, MediaKind::Audio)).unwrap();
        assert_eq!(read_back, b"OggS...");
    }

    #[test]
    fn empty_adapter_output_is_rejected() {
        let (dir, store) = store();
        let source = dir.path().join("empty.ogg");
        std::fs::write(&source, b"").unwrap();
        let err = store.put_file(&source, MediaKind::Audio).unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err}");
    }

    #[test]
    fn no_partial_files_survive_a_successful_write() {
        let (dir, store) = store();
        let stored = store.put_bytes(b"payload", MediaKind::Image).unwrap();
        let shard = dir.path().join("media").join(&stored.file_hash[..2]);
        let names: Vec<String> = std::fs::read_dir(&shard)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(!names[0].ends_with(".part"));
    }

    #[test]
    fn staging_dirs_clean_themselves_up() {
        let (dir, store) = store();
        let path = {
            let staging = store.staging("job-1").unwrap();
            let out = staging.out_path("x.ogg");
            std::fs::write(&out, b"scratch").unwrap();
            assert!(out.exists());
            staging.path().to_path_buf()
        };
        assert!(!path.exists(), "staging dir must vanish on drop");
        assert!(dir.path().join("tmp").exists());
    }

    #[test]
    fn janitor_removes_leftovers_from_a_crash() {
        let (dir, store) = store();
        let leftover = dir.path().join("tmp").join("crashed-job");
        std::fs::create_dir_all(&leftover).unwrap();
        std::fs::write(leftover.join("half.ogg"), b"truncated").unwrap();
        assert_eq!(store.clean_staging().unwrap(), 1);
        assert!(!leftover.exists());
        // Clean twice: an empty staging root is not an error.
        assert_eq!(store.clean_staging().unwrap(), 0);
    }
}
