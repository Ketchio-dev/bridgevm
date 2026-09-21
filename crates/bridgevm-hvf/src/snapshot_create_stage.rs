//! Internal durable boundaries in snapshot creation.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CreateStage {
    DiskSynced,
    VarsSynced,
    ManifestPublished,
    StagingDirectorySynced,
    SnapshotPublished,
}
