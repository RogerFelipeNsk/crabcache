//! Protocol definitions and parsing

pub mod advanced_pipeline;
pub mod binary;
pub mod commands;
pub mod parser;
pub mod pipeline;
pub mod serializer;
pub mod simd_parser;
pub mod zero_copy_buffer;

pub use advanced_pipeline::{
    AdaptiveBatchSizer, AdvancedPipelineConfig, AdvancedPipelineMetrics, AdvancedPipelineProcessor,
    CommandAffinityAnalyzer, ParallelBatchParser,
};
pub use binary::BinaryProtocol;
pub use commands::{Command, Response};
pub use parser::ProtocolParser;
pub use pipeline::{
    PipelineBatch, PipelineBuilder, PipelineProcessor, PipelineProtocol, PipelineResponseBatch,
    PipelineStats,
};
pub use serializer::ProtocolSerializer;
pub use simd_parser::SIMDParser;
pub use zero_copy_buffer::{ZeroCopyBufferPool, ZeroCopyConfig, ZeroCopySerializer};
