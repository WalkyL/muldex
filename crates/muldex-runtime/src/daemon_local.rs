use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonLockMetadata {
    pub owner_pid: u32,
    pub created_at_ms: u64,
    pub last_heartbeat_ms: u64,
    pub runtime_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStateMetadata {
    pub owner_pid: u32,
    pub status: String,
    pub snapshot_path: String,
    pub session_count: usize,
    pub updated_at_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LocalDaemonError {
    #[error("daemon lock already exists: {0}")]
    LockAlreadyExists(String),
    #[error("daemon lock not held")]
    LockNotHeld,
    #[error("daemon local IO failed: {0}")]
    Io(String),
    #[error("daemon local serialization failed: {0}")]
    Serialization(String),
    #[error("daemon ownership still fresh; force takeover denied")]
    TakeoverDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleOwnershipStatus {
    NoLock,
    Fresh { heartbeat_age_ms: u64 },
    Stale { heartbeat_age_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleOwnershipReport {
    pub status: StaleOwnershipStatus,
    pub lock: Option<DaemonLockMetadata>,
    pub stale_threshold_ms: u64,
}

#[derive(Debug, Clone)]
pub struct LocalDaemonOwnership {
    runtime_root: PathBuf,
    lock_path: PathBuf,
    state_path: PathBuf,
    held_lock: Option<DaemonLockMetadata>,
}

impl LocalDaemonOwnership {
    pub fn new(runtime_root: impl Into<PathBuf>) -> Self {
        let runtime_root = runtime_root.into();
        let lock_path = runtime_root.join("daemon.lock.json");
        let state_path = runtime_root.join("daemon.state.json");
        Self {
            runtime_root,
            lock_path,
            state_path,
            held_lock: None,
        }
    }

    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    pub fn state_path(&self) -> &Path {
        &self.state_path
    }

    pub fn held_lock(&self) -> Option<&DaemonLockMetadata> {
        self.held_lock.as_ref()
    }

    pub fn acquire(
        &mut self,
        owner_pid: u32,
        created_at_ms: u64,
    ) -> Result<DaemonLockMetadata, LocalDaemonError> {
        fs::create_dir_all(&self.runtime_root)
            .map_err(|error| LocalDaemonError::Io(error.to_string()))?;

        if self.lock_path.exists() {
            return Err(LocalDaemonError::LockAlreadyExists(
                self.lock_path.display().to_string(),
            ));
        }

        let metadata = DaemonLockMetadata {
            owner_pid,
            created_at_ms,
            last_heartbeat_ms: created_at_ms,
            runtime_root: self.runtime_root.display().to_string(),
        };
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|error| LocalDaemonError::Serialization(error.to_string()))?;
        fs::write(&self.lock_path, json).map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        self.held_lock = Some(metadata.clone());
        Ok(metadata)
    }

    pub fn release(&mut self) -> Result<(), LocalDaemonError> {
        if self.held_lock.is_none() {
            return Err(LocalDaemonError::LockNotHeld);
        }
        if self.lock_path.exists() {
            fs::remove_file(&self.lock_path)
                .map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        }
        self.held_lock = None;
        Ok(())
    }

    pub fn read_lock(&self) -> Result<DaemonLockMetadata, LocalDaemonError> {
        let json = fs::read_to_string(&self.lock_path)
            .map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|error| LocalDaemonError::Serialization(error.to_string()))
    }

    pub fn refresh_heartbeat(&mut self, now_ms: u64) -> Result<DaemonLockMetadata, LocalDaemonError> {
        let mut metadata = self.read_lock()?;
        metadata.last_heartbeat_ms = now_ms;
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|error| LocalDaemonError::Serialization(error.to_string()))?;
        fs::write(&self.lock_path, json).map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        self.held_lock = Some(metadata.clone());
        Ok(metadata)
    }

    pub fn write_state(&self, metadata: &DaemonStateMetadata) -> Result<(), LocalDaemonError> {
        fs::create_dir_all(&self.runtime_root)
            .map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        let json = serde_json::to_string_pretty(metadata)
            .map_err(|error| LocalDaemonError::Serialization(error.to_string()))?;
        fs::write(&self.state_path, json).map_err(|error| LocalDaemonError::Io(error.to_string()))
    }

    pub fn read_state(&self) -> Result<DaemonStateMetadata, LocalDaemonError> {
        let json = fs::read_to_string(&self.state_path)
            .map_err(|error| LocalDaemonError::Io(error.to_string()))?;
        serde_json::from_str(&json)
            .map_err(|error| LocalDaemonError::Serialization(error.to_string()))
    }

    pub fn classify_stale(
        &self,
        now_ms: u64,
        stale_threshold_ms: u64,
    ) -> Result<StaleOwnershipStatus, LocalDaemonError> {
        if !self.lock_path.exists() {
            return Ok(StaleOwnershipStatus::NoLock);
        }
        let metadata = self.read_lock()?;
        let age = now_ms.saturating_sub(metadata.last_heartbeat_ms);
        if age > stale_threshold_ms {
            Ok(StaleOwnershipStatus::Stale { heartbeat_age_ms: age })
        } else {
            Ok(StaleOwnershipStatus::Fresh { heartbeat_age_ms: age })
        }
    }

    pub fn stale_report(
        &self,
        now_ms: u64,
        stale_threshold_ms: u64,
    ) -> Result<StaleOwnershipReport, LocalDaemonError> {
        let status = self.classify_stale(now_ms, stale_threshold_ms)?;
        let lock = if self.lock_path.exists() {
            Some(self.read_lock()?)
        } else {
            None
        };
        Ok(StaleOwnershipReport {
            status,
            lock,
            stale_threshold_ms,
        })
    }

    pub fn force_takeover(
        &mut self,
        owner_pid: u32,
        now_ms: u64,
        stale_threshold_ms: u64,
    ) -> Result<DaemonLockMetadata, LocalDaemonError> {
        match self.classify_stale(now_ms, stale_threshold_ms)? {
            StaleOwnershipStatus::NoLock => self.acquire(owner_pid, now_ms),
            StaleOwnershipStatus::Fresh { .. } => Err(LocalDaemonError::TakeoverDenied),
            StaleOwnershipStatus::Stale { .. } => {
                let metadata = DaemonLockMetadata {
                    owner_pid,
                    created_at_ms: now_ms,
                    last_heartbeat_ms: now_ms,
                    runtime_root: self.runtime_root.display().to_string(),
                };
                let json = serde_json::to_string_pretty(&metadata)
                    .map_err(|error| LocalDaemonError::Serialization(error.to_string()))?;
                fs::write(&self.lock_path, json)
                    .map_err(|error| LocalDaemonError::Io(error.to_string()))?;
                self.held_lock = Some(metadata.clone());
                Ok(metadata)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn local_ownership_can_acquire_and_release_lock() {
        let root = runtime_root("muldex-local-daemon-lock");
        let mut ownership = LocalDaemonOwnership::new(&root);

        let metadata = ownership.acquire(1234, 77).expect("acquire lock");
        assert_eq!(metadata.owner_pid, 1234);
        assert_eq!(metadata.last_heartbeat_ms, 77);
        assert!(ownership.lock_path().exists());

        ownership.release().expect("release lock");
        assert!(!ownership.lock_path().exists());

        let _ = fs::remove_file(root.join("daemon.state.json"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_rejects_second_acquire_when_lock_exists() {
        let root = runtime_root("muldex-local-daemon-dup");
        let mut first = LocalDaemonOwnership::new(&root);
        let mut second = LocalDaemonOwnership::new(&root);

        first.acquire(1, 1).expect("first acquire");
        let error = second.acquire(2, 2).expect_err("second acquire should fail");

        assert!(matches!(error, LocalDaemonError::LockAlreadyExists(_)));

        first.release().expect("release first lock");
        let _ = fs::remove_file(root.join("daemon.state.json"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_can_write_and_read_state_file() {
        let root = runtime_root("muldex-local-daemon-state");
        let ownership = LocalDaemonOwnership::new(&root);
        let state = DaemonStateMetadata {
            owner_pid: 42,
            status: "Running".to_string(),
            snapshot_path: root.join("host.snapshot.json").display().to_string(),
            session_count: 3,
            updated_at_ms: 99,
        };

        ownership.write_state(&state).expect("write state");
        let restored = ownership.read_state().expect("read state");
        assert_eq!(restored, state);

        let _ = fs::remove_file(root.join("daemon.state.json"));
        let _ = fs::remove_file(root.join("daemon.lock.json"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_can_refresh_heartbeat() {
        let root = runtime_root("muldex-local-daemon-heartbeat");
        let mut ownership = LocalDaemonOwnership::new(&root);
        ownership.acquire(10, 100).expect("acquire lock");

        let updated = ownership.refresh_heartbeat(250).expect("refresh heartbeat");
        assert_eq!(updated.last_heartbeat_ms, 250);

        ownership.release().expect("release lock");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_can_classify_stale_status() {
        let root = runtime_root("muldex-local-daemon-stale");
        let mut ownership = LocalDaemonOwnership::new(&root);
        ownership.acquire(10, 100).expect("acquire lock");

        let fresh = ownership.classify_stale(150, 100).expect("classify fresh");
        let stale = ownership.classify_stale(500, 100).expect("classify stale");

        assert_eq!(fresh, StaleOwnershipStatus::Fresh { heartbeat_age_ms: 50 });
        assert_eq!(stale, StaleOwnershipStatus::Stale { heartbeat_age_ms: 400 });

        ownership.release().expect("release lock");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_can_build_stale_report() {
        let root = runtime_root("muldex-local-daemon-report");
        let mut ownership = LocalDaemonOwnership::new(&root);
        ownership.acquire(10, 100).expect("acquire lock");

        let report = ownership.stale_report(250, 100).expect("stale report");
        assert_eq!(report.stale_threshold_ms, 100);
        assert!(matches!(report.status, StaleOwnershipStatus::Stale { .. }));
        assert!(report.lock.is_some());

        ownership.release().expect("release lock");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_denies_takeover_when_owner_is_fresh() {
        let root = runtime_root("muldex-local-daemon-no-takeover");
        let mut ownership = LocalDaemonOwnership::new(&root);
        ownership.acquire(10, 100).expect("acquire lock");

        let error = ownership
            .force_takeover(20, 150, 100)
            .expect_err("takeover should fail");
        assert_eq!(error, LocalDaemonError::TakeoverDenied);

        ownership.release().expect("release lock");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn local_ownership_allows_takeover_when_owner_is_stale() {
        let root = runtime_root("muldex-local-daemon-takeover");
        let mut ownership = LocalDaemonOwnership::new(&root);
        ownership.acquire(10, 100).expect("acquire lock");

        let updated = ownership
            .force_takeover(20, 500, 100)
            .expect("takeover should succeed");
        assert_eq!(updated.owner_pid, 20);
        assert_eq!(updated.last_heartbeat_ms, 500);

        ownership.release().expect("release lock");
        let _ = fs::remove_dir_all(&root);
    }
}
