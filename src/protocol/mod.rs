//! Protocol definitions and parsing

pub mod binary;
pub mod commands;
pub mod parser;
pub mod pipeline;
pub mod serializer;

pub use binary::BinaryProtocol;
pub use commands::{Command, Response};
pub use parser::ProtocolParser;
pub use pipeline::{
    PipelineBatch, PipelineBuilder, PipelineProcessor, PipelineProtocol, PipelineResponseBatch,
    PipelineStats,
};
pub use serializer::ProtocolSerializer;
