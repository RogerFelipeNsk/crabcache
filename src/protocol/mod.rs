//! Protocol definitions and parsing

pub mod advanced_pipeline;
pub mod binary;
pub mod commands;
pub mod parser;
pub mod pipeline;
pub mod serializer;
pub mod simd_parser;
pub mod zero_copy_buffer;

// Phase 8.1 - Protobuf Native Support
pub mod protobuf;

// Phase 8.2 - TOON Protocol Support (Tiny Optimized Object Notation)
pub mod toon;

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

// Phase 8.1 - Protobuf exports
pub use protobuf::{
    ProtocolNegotiator, ProtocolType, NegotiationResult,
    ProtobufParser, ProtobufSerializer, ProtobufZeroCopy,
    SchemaRegistry, ProtobufBufferPool,
    ProtobufConfig, ProtobufMetrics, ProtobufError, ProtobufResult,
    PROTOBUF_MAGIC, PROTOBUF_VERSION, MAX_PROTOBUF_MESSAGE_SIZE,
};

// Phase 8.2 - TOON Protocol exports
pub use toon::{
    ToonPacket, ToonType, ToonFlags, StringInterner,
    encoder::ToonEncoder,
    decoder::ToonDecoder,
    TOON_MAGIC, TOON_VERSION,
};
