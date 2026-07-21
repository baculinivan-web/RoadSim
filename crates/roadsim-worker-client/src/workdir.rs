use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const RUN_DIRECTORY_SCHEMA_VERSION: u32 = 1;
const RUN_DIRECTORY_PREFIX: &str = "run-";
const STATE_FILE_PREFIX: &str = "state-";
const STATE_FILE_SUFFIX: &str = ".json";
const MAX_MARKER_BYTES: u64 = 16 * 1024;
const MAX_CONFIGURED_RUN_DIRECTORIES: usize = 1_024;
const MAX_CONFIGURED_STATE_ENTRIES: usize = 64;
const OWNER_DROPPED_DIAGNOSTIC: &str = "worker.run.owner_dropped";
const RECOVERED_DIAGNOSTIC: &str = "worker.run.recovered_after_interruption";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunDirectoryLimits {
    max_retained_runs: usize,
    max_state_entries_per_run: usize,
}

impl RunDirectoryLimits {
    pub fn new(
        max_retained_runs: usize,
        max_state_entries_per_run: usize,
    ) -> Result<Self, RunDirectoryError> {
        if max_retained_runs == 0
            || max_retained_runs > MAX_CONFIGURED_RUN_DIRECTORIES
            || !(4..=MAX_CONFIGURED_STATE_ENTRIES).contains(&max_state_entries_per_run)
        {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::InvalidLimits,
                None,
            ));
        }
        Ok(Self {
            max_retained_runs,
            max_state_entries_per_run,
        })
    }

    #[must_use]
    pub const fn max_retained_runs(self) -> usize {
        self.max_retained_runs
    }

    #[must_use]
    pub const fn max_state_entries_per_run(self) -> usize {
        self.max_state_entries_per_run
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Starting,
    Running,
    Completed,
    Cancelled,
    Failed,
    Incomplete,
}

impl RunState {
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Incomplete
        )
    }

    const fn may_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Starting, Self::Running)
                | (
                    Self::Starting | Self::Running,
                    Self::Cancelled | Self::Failed | Self::Incomplete
                )
                | (Self::Running, Self::Completed)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct RunStateMarker {
    schema_version: u32,
    session_id: u64,
    creation_sequence: u64,
    transition_sequence: u32,
    state: RunState,
    diagnostic_code: Option<String>,
}

impl RunStateMarker {
    fn starting(session_id: u64, creation_sequence: u64) -> Self {
        Self {
            schema_version: RUN_DIRECTORY_SCHEMA_VERSION,
            session_id,
            creation_sequence,
            transition_sequence: 0,
            state: RunState::Starting,
            diagnostic_code: None,
        }
    }

    fn validate(&self) -> Result<(), RunDirectoryError> {
        if self.schema_version != RUN_DIRECTORY_SCHEMA_VERSION
            || !diagnostic_is_valid(self.diagnostic_code.as_deref())
            || (matches!(self.state, RunState::Failed | RunState::Incomplete)
                != self.diagnostic_code.is_some())
        {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(self.session_id),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunDirectoryErrorCode {
    InvalidLimits,
    RootInvalid,
    Io,
    CapacityExceeded,
    RunAlreadyExists,
    InvalidTransition,
    MarkerCorrupt,
    UnsafeEntry,
    StateJournalFull,
}

impl RunDirectoryErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidLimits => "worker.workdir.limits_invalid",
            Self::RootInvalid => "worker.workdir.root_invalid",
            Self::Io => "worker.workdir.io",
            Self::CapacityExceeded => "worker.workdir.capacity_exceeded",
            Self::RunAlreadyExists => "worker.workdir.run_exists",
            Self::InvalidTransition => "worker.workdir.transition_invalid",
            Self::MarkerCorrupt => "worker.workdir.marker_corrupt",
            Self::UnsafeEntry => "worker.workdir.entry_unsafe",
            Self::StateJournalFull => "worker.workdir.state_journal_full",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunDirectoryError {
    code: RunDirectoryErrorCode,
    session_id: Option<u64>,
}

impl RunDirectoryError {
    const fn new(code: RunDirectoryErrorCode, session_id: Option<u64>) -> Self {
        Self { code, session_id }
    }

    #[must_use]
    pub const fn code(self) -> RunDirectoryErrorCode {
        self.code
    }

    #[must_use]
    pub const fn session_id(self) -> Option<u64> {
        self.session_id
    }
}

impl fmt::Display for RunDirectoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for RunDirectoryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunDirectoryRecord {
    session_id: u64,
    creation_sequence: u64,
    state: RunState,
    recovered_to_incomplete: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RunDirectoryIdentity {
    creation_sequence: u64,
    session_id: u64,
}

impl RunDirectoryIdentity {
    #[must_use]
    pub const fn new(creation_sequence: u64, session_id: u64) -> Self {
        Self {
            creation_sequence,
            session_id,
        }
    }

    #[must_use]
    pub const fn creation_sequence(self) -> u64 {
        self.creation_sequence
    }

    #[must_use]
    pub const fn session_id(self) -> u64 {
        self.session_id
    }
}

impl RunDirectoryRecord {
    #[must_use]
    pub const fn session_id(&self) -> u64 {
        self.session_id
    }

    #[must_use]
    pub const fn creation_sequence(&self) -> u64 {
        self.creation_sequence
    }

    #[must_use]
    pub const fn state(&self) -> RunState {
        self.state
    }

    #[must_use]
    pub const fn recovered_to_incomplete(&self) -> bool {
        self.recovered_to_incomplete
    }

    #[must_use]
    pub const fn identity(&self) -> RunDirectoryIdentity {
        RunDirectoryIdentity::new(self.creation_sequence, self.session_id)
    }
}

#[derive(Clone)]
pub struct RunDirectoryManager {
    root: PathBuf,
    limits: RunDirectoryLimits,
}

impl RunDirectoryManager {
    pub fn new(
        root: impl Into<PathBuf>,
        limits: RunDirectoryLimits,
    ) -> Result<Self, RunDirectoryError> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::RootInvalid,
                None,
            ));
        }
        fs::create_dir_all(&root)
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, None))?;
        let metadata = fs::symlink_metadata(&root)
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, None))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::RootInvalid,
                None,
            ));
        }
        Ok(Self { root, limits })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn create_run(
        &self,
        session_id: u64,
        creation_sequence: u64,
    ) -> Result<RunDirectory, RunDirectoryError> {
        self.ensure_capacity()?;
        let path = self
            .root
            .join(run_directory_name(creation_sequence, session_id));
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(RunDirectoryError::new(
                    RunDirectoryErrorCode::RunAlreadyExists,
                    Some(session_id),
                ));
            }
            Err(_) => {
                return Err(RunDirectoryError::new(
                    RunDirectoryErrorCode::Io,
                    Some(session_id),
                ));
            }
        }
        let marker = RunStateMarker::starting(session_id, creation_sequence);
        if let Err(error) = write_marker(&path, &marker) {
            let _ = fs::remove_dir(&path);
            return Err(error);
        }
        Ok(RunDirectory {
            path,
            marker,
            max_state_entries: self.limits.max_state_entries_per_run,
            armed: true,
        })
    }

    pub fn recover(&self) -> Result<Vec<RunDirectoryRecord>, RunDirectoryError> {
        let mut records = self.scan_records()?;
        for record in &mut records {
            if matches!(record.marker.state, RunState::Starting | RunState::Running) {
                let next_sequence = record
                    .marker
                    .transition_sequence
                    .checked_add(1)
                    .ok_or_else(|| {
                        RunDirectoryError::new(
                            RunDirectoryErrorCode::StateJournalFull,
                            Some(record.marker.session_id),
                        )
                    })?;
                if next_sequence as usize >= self.limits.max_state_entries_per_run {
                    return Err(RunDirectoryError::new(
                        RunDirectoryErrorCode::StateJournalFull,
                        Some(record.marker.session_id),
                    ));
                }
                let next = RunStateMarker {
                    transition_sequence: next_sequence,
                    state: RunState::Incomplete,
                    diagnostic_code: Some(RECOVERED_DIAGNOSTIC.to_owned()),
                    ..record.marker.clone()
                };
                write_marker(&record.path, &next)?;
                record.marker = next;
                record.recovered_to_incomplete = true;
            }
        }
        records.sort_by_key(|record| (record.marker.creation_sequence, record.marker.session_id));
        Ok(records
            .into_iter()
            .map(|record| RunDirectoryRecord {
                session_id: record.marker.session_id,
                creation_sequence: record.marker.creation_sequence,
                state: record.marker.state,
                recovered_to_incomplete: record.recovered_to_incomplete,
            })
            .collect())
    }

    pub fn cleanup_completed(&self) -> Result<Vec<RunDirectoryIdentity>, RunDirectoryError> {
        let mut records = self.scan_records()?;
        records.retain(|record| {
            matches!(
                record.marker.state,
                RunState::Completed | RunState::Cancelled
            )
        });
        records.sort_by_key(|record| (record.marker.creation_sequence, record.marker.session_id));
        let mut removed = Vec::with_capacity(records.len());
        for record in records {
            remove_owned_run_directory(
                &self.root,
                &record.path,
                record.marker.creation_sequence,
                record.marker.session_id,
            )?;
            removed.push(RunDirectoryIdentity::new(
                record.marker.creation_sequence,
                record.marker.session_id,
            ));
        }
        Ok(removed)
    }

    fn ensure_capacity(&self) -> Result<(), RunDirectoryError> {
        let mut records = self.scan_records()?;
        let removals_needed = records
            .len()
            .saturating_add(1)
            .saturating_sub(self.limits.max_retained_runs);
        if removals_needed == 0 {
            return Ok(());
        }
        records.sort_by_key(|record| (record.marker.creation_sequence, record.marker.session_id));
        let candidates: Vec<_> = records
            .into_iter()
            .filter(|record| {
                matches!(
                    record.marker.state,
                    RunState::Completed | RunState::Cancelled
                )
            })
            .take(removals_needed)
            .collect();
        if candidates.len() != removals_needed {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::CapacityExceeded,
                None,
            ));
        }
        for candidate in candidates {
            remove_owned_run_directory(
                &self.root,
                &candidate.path,
                candidate.marker.creation_sequence,
                candidate.marker.session_id,
            )?;
        }
        Ok(())
    }

    fn scan_records(&self) -> Result<Vec<InternalRecord>, RunDirectoryError> {
        let mut records = Vec::new();
        let entries = fs::read_dir(&self.root)
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, None))?;
        for entry in entries {
            let entry =
                entry.map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, None))?;
            let name = entry.file_name();
            let Some((creation_sequence, session_id)) =
                parse_run_directory_name(&name.to_string_lossy())
            else {
                return Err(RunDirectoryError::new(
                    RunDirectoryErrorCode::UnsafeEntry,
                    None,
                ));
            };
            let file_type = entry
                .file_type()
                .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
            if file_type.is_symlink() || !file_type.is_dir() {
                return Err(RunDirectoryError::new(
                    RunDirectoryErrorCode::UnsafeEntry,
                    Some(session_id),
                ));
            }
            let path = entry.path();
            let marker = load_marker_journal(
                &path,
                creation_sequence,
                session_id,
                self.limits.max_state_entries_per_run,
            )?;
            records.push(InternalRecord {
                path,
                marker,
                recovered_to_incomplete: false,
            });
        }
        Ok(records)
    }
}

pub struct RunDirectory {
    path: PathBuf,
    marker: RunStateMarker,
    max_state_entries: usize,
    armed: bool,
}

impl RunDirectory {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn session_id(&self) -> u64 {
        self.marker.session_id
    }

    #[must_use]
    pub const fn state(&self) -> RunState {
        self.marker.state
    }

    #[must_use]
    pub const fn identity(&self) -> RunDirectoryIdentity {
        RunDirectoryIdentity::new(self.marker.creation_sequence, self.marker.session_id)
    }

    pub fn mark_running(&mut self) -> Result<(), RunDirectoryError> {
        self.transition(RunState::Running, None)
    }

    pub fn mark_completed(&mut self) -> Result<(), RunDirectoryError> {
        self.transition(RunState::Completed, None)
    }

    pub fn mark_cancelled(&mut self) -> Result<(), RunDirectoryError> {
        self.transition(RunState::Cancelled, None)
    }

    pub fn mark_failed(&mut self, diagnostic_code: &str) -> Result<(), RunDirectoryError> {
        self.transition(RunState::Failed, Some(diagnostic_code))
    }

    pub fn mark_incomplete(&mut self, diagnostic_code: &str) -> Result<(), RunDirectoryError> {
        self.transition(RunState::Incomplete, Some(diagnostic_code))
    }

    fn transition(
        &mut self,
        state: RunState,
        diagnostic_code: Option<&str>,
    ) -> Result<(), RunDirectoryError> {
        if !self.marker.state.may_transition_to(state)
            || !diagnostic_is_valid(diagnostic_code)
            || (matches!(state, RunState::Failed | RunState::Incomplete)
                && diagnostic_code.is_none())
        {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::InvalidTransition,
                Some(self.marker.session_id),
            ));
        }
        let next_sequence = self
            .marker
            .transition_sequence
            .checked_add(1)
            .ok_or_else(|| {
                RunDirectoryError::new(
                    RunDirectoryErrorCode::StateJournalFull,
                    Some(self.marker.session_id),
                )
            })?;
        if next_sequence as usize >= self.max_state_entries {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::StateJournalFull,
                Some(self.marker.session_id),
            ));
        }
        let next = RunStateMarker {
            transition_sequence: next_sequence,
            state,
            diagnostic_code: diagnostic_code.map(str::to_owned),
            ..self.marker.clone()
        };
        write_marker(&self.path, &next)?;
        self.marker = next;
        if state.is_terminal() {
            self.armed = false;
        }
        Ok(())
    }
}

impl Drop for RunDirectory {
    fn drop(&mut self) {
        if self.armed && !self.marker.state.is_terminal() {
            let _ = self.transition(RunState::Incomplete, Some(OWNER_DROPPED_DIAGNOSTIC));
        }
    }
}

#[derive(Clone)]
struct InternalRecord {
    path: PathBuf,
    marker: RunStateMarker,
    recovered_to_incomplete: bool,
}

fn diagnostic_is_valid(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= 128
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
    })
}

fn run_directory_name(creation_sequence: u64, session_id: u64) -> String {
    format!("{RUN_DIRECTORY_PREFIX}{creation_sequence:016x}-{session_id:016x}")
}

fn parse_run_directory_name(name: &str) -> Option<(u64, u64)> {
    let (creation_sequence, session_id) =
        name.strip_prefix(RUN_DIRECTORY_PREFIX)?.split_once('-')?;
    if !is_lower_hex(creation_sequence, 16) || !is_lower_hex(session_id, 16) {
        return None;
    }
    Some((
        u64::from_str_radix(creation_sequence, 16).ok()?,
        u64::from_str_radix(session_id, 16).ok()?,
    ))
}

fn state_file_name(sequence: u32) -> String {
    format!("{STATE_FILE_PREFIX}{sequence:08x}{STATE_FILE_SUFFIX}")
}

fn parse_state_file_name(name: &str) -> Option<u32> {
    let hexadecimal = name
        .strip_prefix(STATE_FILE_PREFIX)?
        .strip_suffix(STATE_FILE_SUFFIX)?;
    if hexadecimal.len() != 8
        || !hexadecimal
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    u32::from_str_radix(hexadecimal, 16).ok()
}

fn is_lower_hex(value: &str, expected_length: usize) -> bool {
    value.len() == expected_length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn write_marker(path: &Path, marker: &RunStateMarker) -> Result<(), RunDirectoryError> {
    marker.validate()?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(marker.session_id)))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RunDirectoryError::new(
            RunDirectoryErrorCode::UnsafeEntry,
            Some(marker.session_id),
        ));
    }
    let encoded = serde_json::to_vec(marker).map_err(|_| {
        RunDirectoryError::new(
            RunDirectoryErrorCode::MarkerCorrupt,
            Some(marker.session_id),
        )
    })?;
    if encoded.len() as u64 > MAX_MARKER_BYTES {
        return Err(RunDirectoryError::new(
            RunDirectoryErrorCode::MarkerCorrupt,
            Some(marker.session_id),
        ));
    }
    let marker_path = path.join(state_file_name(marker.transition_sequence));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker_path)
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(marker.session_id)))?;
    file.write_all(&encoded)
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(marker.session_id)))
}

fn load_marker_journal(
    path: &Path,
    creation_sequence: u64,
    session_id: u64,
    max_entries: usize,
) -> Result<RunStateMarker, RunDirectoryError> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?
    {
        let entry = entry
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
        let file_type = entry
            .file_type()
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
        if file_type.is_symlink() {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::UnsafeEntry,
                Some(session_id),
            ));
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(sequence) = parse_state_file_name(&name) {
            if !file_type.is_file() {
                return Err(RunDirectoryError::new(
                    RunDirectoryErrorCode::UnsafeEntry,
                    Some(session_id),
                ));
            }
            entries.push((sequence, entry.path()));
        } else if name.starts_with(STATE_FILE_PREFIX) {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(session_id),
            ));
        }
    }
    entries.sort_by_key(|(sequence, _)| *sequence);
    if entries.is_empty() || entries.len() > max_entries {
        return Err(RunDirectoryError::new(
            RunDirectoryErrorCode::MarkerCorrupt,
            Some(session_id),
        ));
    }
    let mut previous: Option<RunStateMarker> = None;
    for (index, (file_sequence, marker_path)) in entries.into_iter().enumerate() {
        if file_sequence as usize != index {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(session_id),
            ));
        }
        let metadata = fs::symlink_metadata(&marker_path)
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
        if metadata.len() == 0 || metadata.len() > MAX_MARKER_BYTES {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(session_id),
            ));
        }
        let mut encoded = Vec::with_capacity(metadata.len() as usize);
        File::open(marker_path)
            .and_then(|file| file.take(MAX_MARKER_BYTES + 1).read_to_end(&mut encoded))
            .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
        let marker: RunStateMarker = serde_json::from_slice(&encoded).map_err(|_| {
            RunDirectoryError::new(RunDirectoryErrorCode::MarkerCorrupt, Some(session_id))
        })?;
        marker.validate()?;
        if marker.session_id != session_id
            || marker.creation_sequence != creation_sequence
            || marker.transition_sequence != file_sequence
            || (file_sequence == 0 && marker.state != RunState::Starting)
        {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(session_id),
            ));
        }
        if let Some(previous_marker) = &previous
            && (marker.creation_sequence != previous_marker.creation_sequence
                || !previous_marker.state.may_transition_to(marker.state))
        {
            return Err(RunDirectoryError::new(
                RunDirectoryErrorCode::MarkerCorrupt,
                Some(session_id),
            ));
        }
        previous = Some(marker);
    }
    previous.ok_or_else(|| {
        RunDirectoryError::new(RunDirectoryErrorCode::MarkerCorrupt, Some(session_id))
    })
}

fn remove_owned_run_directory(
    root: &Path,
    path: &Path,
    creation_sequence: u64,
    session_id: u64,
) -> Result<(), RunDirectoryError> {
    if path.parent() != Some(root)
        || path.file_name().and_then(|name| name.to_str())
            != Some(&run_directory_name(creation_sequence, session_id))
    {
        return Err(RunDirectoryError::new(
            RunDirectoryErrorCode::UnsafeEntry,
            Some(session_id),
        ));
    }
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RunDirectoryError::new(
            RunDirectoryErrorCode::UnsafeEntry,
            Some(session_id),
        ));
    }
    fs::remove_dir_all(path)
        .map_err(|_| RunDirectoryError::new(RunDirectoryErrorCode::Io, Some(session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "roadsim-worker-client-test-{}-{sequence}",
                std::process::id()
            ));
            if path.exists() {
                fs::remove_dir_all(&path).unwrap();
            }
            Self(path)
        }

        fn manager(&self, retained: usize) -> RunDirectoryManager {
            RunDirectoryManager::new(&self.0, RunDirectoryLimits::new(retained, 8).unwrap())
                .unwrap()
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            if self.0.exists() {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }
    }

    #[test]
    fn journal_enforces_lifecycle_and_persists_completed_state() {
        let root = TestRoot::new();
        let manager = root.manager(4);
        let mut run = manager.create_run(42, 100).unwrap();
        assert_eq!(run.state(), RunState::Starting);

        let error = run.mark_completed().unwrap_err();
        assert_eq!(error.code(), RunDirectoryErrorCode::InvalidTransition);
        assert_eq!(run.state(), RunState::Starting);

        run.mark_running().unwrap();
        run.mark_completed().unwrap();
        drop(run);

        let records = manager.recover().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].session_id(), 42);
        assert_eq!(records[0].creation_sequence(), 100);
        assert_eq!(records[0].state(), RunState::Completed);
        assert!(!records[0].recovered_to_incomplete());
    }

    #[test]
    fn repeated_protocol_session_uses_distinct_creation_identity() {
        let root = TestRoot::new();
        let manager = root.manager(4);
        let first = manager.create_run(5, 1).unwrap();
        let first_path = first.path().to_owned();
        drop(first);
        let second = manager.create_run(5, 2).unwrap();
        assert_ne!(first_path, second.path());
        assert_eq!(second.identity(), RunDirectoryIdentity::new(2, 5));
    }

    #[test]
    fn recovery_converts_interrupted_running_state_to_incomplete_once() {
        let root = TestRoot::new();
        let manager = root.manager(4);
        let mut run = manager.create_run(7, 1).unwrap();
        run.mark_running().unwrap();
        std::mem::forget(run);

        let recovered = manager.recover().unwrap();
        assert_eq!(recovered[0].state(), RunState::Incomplete);
        assert!(recovered[0].recovered_to_incomplete());

        let second_scan = manager.recover().unwrap();
        assert_eq!(second_scan[0].state(), RunState::Incomplete);
        assert!(!second_scan[0].recovered_to_incomplete());
    }

    #[test]
    fn capacity_prunes_oldest_success_but_preserves_failed_and_incomplete_runs() {
        let root = TestRoot::new();
        let manager = root.manager(2);
        let mut completed = manager.create_run(1, 10).unwrap();
        let completed_path = completed.path().to_owned();
        completed.mark_running().unwrap();
        completed.mark_completed().unwrap();
        drop(completed);

        let mut failed = manager.create_run(2, 20).unwrap();
        failed.mark_failed("worker.test.failed").unwrap();
        drop(failed);

        let incomplete = manager.create_run(3, 30).unwrap();
        assert!(!completed_path.exists());
        drop(incomplete);

        let error = match manager.create_run(4, 40) {
            Ok(_) => panic!("failed and incomplete runs must block capacity"),
            Err(error) => error,
        };
        assert_eq!(error.code(), RunDirectoryErrorCode::CapacityExceeded);
        let records = manager.recover().unwrap();
        assert_eq!(
            records
                .iter()
                .map(RunDirectoryRecord::state)
                .collect::<Vec<_>>(),
            [RunState::Failed, RunState::Incomplete]
        );
    }

    #[test]
    fn explicit_cleanup_removes_only_successful_or_cancelled_runs() {
        let root = TestRoot::new();
        let manager = root.manager(4);
        let mut completed = manager.create_run(1, 1).unwrap();
        completed.mark_running().unwrap();
        completed.mark_completed().unwrap();
        let completed_path = completed.path().to_owned();
        drop(completed);

        let mut cancelled = manager.create_run(2, 2).unwrap();
        cancelled.mark_cancelled().unwrap();
        let cancelled_path = cancelled.path().to_owned();
        drop(cancelled);

        let mut failed = manager.create_run(3, 3).unwrap();
        failed.mark_failed("worker.test.failed").unwrap();
        let failed_path = failed.path().to_owned();
        drop(failed);

        assert_eq!(
            manager.cleanup_completed().unwrap(),
            [
                RunDirectoryIdentity::new(1, 1),
                RunDirectoryIdentity::new(2, 2)
            ]
        );
        assert!(!completed_path.exists());
        assert!(!cancelled_path.exists());
        assert!(failed_path.exists());
    }

    #[test]
    fn lowering_retention_prunes_enough_old_successes_before_create() {
        let root = TestRoot::new();
        let initial = root.manager(4);
        for session_id in 1..=3 {
            let mut run = initial.create_run(session_id, session_id).unwrap();
            run.mark_running().unwrap();
            run.mark_completed().unwrap();
        }

        let reduced = root.manager(2);
        let fourth = reduced.create_run(4, 4).unwrap();
        drop(fourth);
        let records = reduced.recover().unwrap();
        assert_eq!(
            records
                .iter()
                .map(RunDirectoryRecord::session_id)
                .collect::<Vec<_>>(),
            [3, 4]
        );
    }

    #[test]
    fn unknown_root_entry_and_broken_journal_fail_closed() {
        let root = TestRoot::new();
        let manager = root.manager(4);
        fs::write(manager.root().join("unexpected.txt"), b"not owned").unwrap();
        let error = manager.recover().unwrap_err();
        assert_eq!(error.code(), RunDirectoryErrorCode::UnsafeEntry);
        fs::remove_file(manager.root().join("unexpected.txt")).unwrap();

        let run = manager.create_run(9, 1).unwrap();
        let path = run.path().to_owned();
        std::mem::forget(run);
        fs::remove_file(path.join(state_file_name(0))).unwrap();
        let error = manager.recover().unwrap_err();
        assert_eq!(error.code(), RunDirectoryErrorCode::MarkerCorrupt);
        assert_eq!(error.session_id(), Some(9));
    }

    #[test]
    fn published_marker_schema_is_valid_json() {
        let schema = include_str!("../../../schemas/worker-runtime/run-state-v1.schema.json");
        let parsed: serde_json::Value = serde_json::from_str(schema).unwrap();
        assert_eq!(
            parsed["$id"],
            "https://roadsim.dev/schemas/worker-runtime/run-state-v1.schema.json"
        );
    }

    #[cfg(unix)]
    #[test]
    fn state_write_rejects_replaced_run_directory_symlink() {
        use std::os::unix::fs::symlink;

        let root = TestRoot::new();
        let manager = root.manager(4);
        let mut run = manager.create_run(11, 1).unwrap();
        let run_path = run.path().to_owned();
        let outside_root = TestRoot::new();
        fs::create_dir(&outside_root.0).unwrap();
        let outside = &outside_root.0;
        fs::remove_dir_all(&run_path).unwrap();
        symlink(outside, &run_path).unwrap();

        let error = run.mark_running().unwrap_err();
        assert_eq!(error.code(), RunDirectoryErrorCode::UnsafeEntry);
        assert!(fs::read_dir(outside).unwrap().next().is_none());
        drop(run);
    }
}
