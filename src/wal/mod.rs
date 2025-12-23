//! Write-Ahead Log implementation for durability

pub mod entry;
pub mod writer;
pub mod reader;

pub use entry::{WALEntry, Operation, SegmentHeader};
pub use writer::{WALWriter, WALConfig, SyncPolicy, WALError};
pub use reader::{WALReader, RecoveryStats, WALReplayTarget};