use super::collector::StatsResponse;
use std::fmt::Write;

pub struct PrometheusExporter;

impl PrometheusExporter {
    pub fn export_metrics(stats: &StatsResponse) -> String {
        let mut output = String::new();

        // Global metrics
        Self::write_help_and_type(
            &mut output,
            "crabcache_uptime_seconds",
            "gauge",
            "Uptime in seconds",
        );
        writeln!(
            output,
            "crabcache_uptime_seconds {}",
            stats.global.uptime_seconds
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_operations_total",
            "counter",
            "Total operations processed",
        );
        writeln!(
            output,
            "crabcache_operations_total {}",
            stats.global.total_operations
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_operations_per_second",
            "gauge",
            "Operations per second",
        );
        writeln!(
            output,
            "crabcache_operations_per_second {:.2}",
            stats.global.operations_per_second
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_memory_used_bytes",
            "gauge",
            "Memory used in bytes",
        );
        writeln!(
            output,
            "crabcache_memory_used_bytes {}",
            stats.global.memory_used_bytes
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_cache_hit_ratio",
            "gauge",
            "Cache hit ratio (0-1)",
        );
        writeln!(
            output,
            "crabcache_cache_hit_ratio {:.4}",
            stats.global.cache_hit_ratio
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_connections_total",
            "counter",
            "Total connections",
        );
        writeln!(
            output,
            "crabcache_connections_total {}",
            stats.global.total_connections
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_connections_active",
            "gauge",
            "Active connections",
        );
        writeln!(
            output,
            "crabcache_connections_active {}",
            stats.global.active_connections
        )
        .unwrap();

        // Latency metrics
        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_p50_milliseconds",
            "gauge",
            "P50 latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_p50_milliseconds {:.3}",
            stats.latency.p50_ms
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_p95_milliseconds",
            "gauge",
            "P95 latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_p95_milliseconds {:.3}",
            stats.latency.p95_ms
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_p99_milliseconds",
            "gauge",
            "P99 latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_p99_milliseconds {:.3}",
            stats.latency.p99_ms
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_p99_9_milliseconds",
            "gauge",
            "P99.9 latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_p99_9_milliseconds {:.3}",
            stats.latency.p99_9_ms
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_mean_milliseconds",
            "gauge",
            "Mean latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_mean_milliseconds {:.3}",
            stats.latency.mean_ms
        )
        .unwrap();

        Self::write_help_and_type(
            &mut output,
            "crabcache_latency_max_milliseconds",
            "gauge",
            "Max latency in milliseconds",
        );
        writeln!(
            output,
            "crabcache_latency_max_milliseconds {:.3}",
            stats.latency.max_ms
        )
        .unwrap();

        // Per-shard metrics
        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_operations_total",
            "counter",
            "Operations per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_operations_total{{shard=\"{}\"}} {}",
                shard_id, shard.operations
            )
            .unwrap();
        }

        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_hits_total",
            "counter",
            "Cache hits per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_hits_total{{shard=\"{}\"}} {}",
                shard_id, shard.hits
            )
            .unwrap();
        }

        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_misses_total",
            "counter",
            "Cache misses per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_misses_total{{shard=\"{}\"}} {}",
                shard_id, shard.misses
            )
            .unwrap();
        }

        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_evictions_total",
            "counter",
            "Evictions per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_evictions_total{{shard=\"{}\"}} {}",
                shard_id, shard.evictions
            )
            .unwrap();
        }

        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_memory_bytes",
            "gauge",
            "Memory used per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_memory_bytes{{shard=\"{}\"}} {}",
                shard_id, shard.memory_bytes
            )
            .unwrap();
        }

        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_items",
            "gauge",
            "Items count per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_items{{shard=\"{}\"}} {}",
                shard_id, shard.items
            )
            .unwrap();
        }

        // Operations by type per shard
        Self::write_help_and_type(
            &mut output,
            "crabcache_shard_operations_by_type_total",
            "counter",
            "Operations by type per shard",
        );
        for (shard_id, shard) in stats.shards.iter().enumerate() {
            writeln!(
                output,
                "crabcache_shard_operations_by_type_total{{shard=\"{}\",operation=\"get\"}} {}",
                shard_id, shard.get_ops
            )
            .unwrap();
            writeln!(
                output,
                "crabcache_shard_operations_by_type_total{{shard=\"{}\",operation=\"put\"}} {}",
                shard_id, shard.put_ops
            )
            .unwrap();
            writeln!(
                output,
                "crabcache_shard_operations_by_type_total{{shard=\"{}\",operation=\"del\"}} {}",
                shard_id, shard.del_ops
            )
            .unwrap();
            writeln!(
                output,
                "crabcache_shard_operations_by_type_total{{shard=\"{}\",operation=\"expire\"}} {}",
                shard_id, shard.expire_ops
            )
            .unwrap();
        }

        output
    }

    fn write_help_and_type(output: &mut String, metric_name: &str, metric_type: &str, help: &str) {
        writeln!(output, "# HELP {} {}", metric_name, help).unwrap();
        writeln!(output, "# TYPE {} {}", metric_name, metric_type).unwrap();
    }
}
