use super::collector::StatsResponse;

pub struct Dashboard;

impl Dashboard {
    pub fn generate_html(stats: &StatsResponse) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CrabCache Dashboard</title>
    <style>
        body {{
            font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
        }}
        .header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 20px;
            border-radius: 10px;
            margin-bottom: 20px;
            text-align: center;
        }}
        .metrics-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
            gap: 20px;
            margin-bottom: 20px;
        }}
        .metric-card {{
            background: white;
            padding: 20px;
            border-radius: 10px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }}
        .metric-title {{
            font-size: 18px;
            font-weight: bold;
            color: #333;
            margin-bottom: 10px;
        }}
        .metric-value {{
            font-size: 24px;
            font-weight: bold;
            color: #667eea;
        }}
        .metric-unit {{
            font-size: 14px;
            color: #666;
        }}
        .latency-grid {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
            gap: 10px;
            margin-top: 10px;
        }}
        .latency-item {{
            text-align: center;
            padding: 10px;
            background: #f8f9fa;
            border-radius: 5px;
        }}
        .shard-table {{
            width: 100%;
            border-collapse: collapse;
            background: white;
            border-radius: 10px;
            overflow: hidden;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }}
        .shard-table th, .shard-table td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #eee;
        }}
        .shard-table th {{
            background-color: #667eea;
            color: white;
            font-weight: bold;
        }}
        .shard-table tr:hover {{
            background-color: #f8f9fa;
        }}
        .status-good {{ color: #28a745; }}
        .status-warning {{ color: #ffc107; }}
        .status-error {{ color: #dc3545; }}
        .refresh-info {{
            text-align: center;
            color: #666;
            margin-top: 20px;
            font-size: 14px;
        }}
    </style>
    <script>
        // Auto-refresh every 5 seconds
        setTimeout(function() {{
            window.location.reload();
        }}, 5000);
    </script>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>🦀 CrabCache Dashboard</h1>
            <p>High-Performance Rust Cache Server</p>
        </div>
        
        <div class="metrics-grid">
            <div class="metric-card">
                <div class="metric-title">Performance</div>
                <div class="metric-value">{:.0} <span class="metric-unit">ops/sec</span></div>
                <div style="margin-top: 10px;">
                    <div>Total Operations: <strong>{}</strong></div>
                    <div>Uptime: <strong>{} seconds</strong></div>
                </div>
            </div>
            
            <div class="metric-card">
                <div class="metric-title">Cache Efficiency</div>
                <div class="metric-value">{:.1}% <span class="metric-unit">hit ratio</span></div>
                <div style="margin-top: 10px;">
                    <div class="status-good">Excellent cache performance</div>
                </div>
            </div>
            
            <div class="metric-card">
                <div class="metric-title">Memory Usage</div>
                <div class="metric-value">{:.1} <span class="metric-unit">MB</span></div>
                <div style="margin-top: 10px;">
                    <div>Total Items: <strong>{}</strong></div>
                </div>
            </div>
            
            <div class="metric-card">
                <div class="metric-title">Connections</div>
                <div class="metric-value">{} <span class="metric-unit">active</span></div>
                <div style="margin-top: 10px;">
                    <div>Total: <strong>{}</strong></div>
                </div>
            </div>
        </div>
        
        <div class="metric-card">
            <div class="metric-title">Latency Metrics</div>
            <div class="latency-grid">
                <div class="latency-item">
                    <div><strong>P50</strong></div>
                    <div>{:.3}ms</div>
                </div>
                <div class="latency-item">
                    <div><strong>P95</strong></div>
                    <div>{:.3}ms</div>
                </div>
                <div class="latency-item">
                    <div><strong>P99</strong></div>
                    <div>{:.3}ms</div>
                </div>
                <div class="latency-item">
                    <div><strong>P99.9</strong></div>
                    <div>{:.3}ms</div>
                </div>
                <div class="latency-item">
                    <div><strong>Mean</strong></div>
                    <div>{:.3}ms</div>
                </div>
                <div class="latency-item">
                    <div><strong>Max</strong></div>
                    <div>{:.3}ms</div>
                </div>
            </div>
        </div>
        
        <div class="metric-card">
            <div class="metric-title">Shard Statistics</div>
            <table class="shard-table">
                <thead>
                    <tr>
                        <th>Shard</th>
                        <th>Operations</th>
                        <th>Hit Ratio</th>
                        <th>Items</th>
                        <th>Memory</th>
                        <th>Evictions</th>
                    </tr>
                </thead>
                <tbody>
                    {}
                </tbody>
            </table>
        </div>
        
        <div class="refresh-info">
            🔄 Auto-refreshing every 5 seconds | Last updated: {}
        </div>
    </div>
</body>
</html>"#,
            stats.global.operations_per_second,
            stats.global.total_operations,
            stats.global.uptime_seconds,
            stats.global.cache_hit_ratio * 100.0,
            stats.global.memory_used_bytes as f64 / 1024.0 / 1024.0,
            stats.shards.iter().map(|s| s.items).sum::<u64>(),
            stats.global.active_connections,
            stats.global.total_connections,
            stats.latency.p50_ms,
            stats.latency.p95_ms,
            stats.latency.p99_ms,
            stats.latency.p99_9_ms,
            stats.latency.mean_ms,
            stats.latency.max_ms,
            Self::generate_shard_rows(&stats.shards),
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
        )
    }
    
    fn generate_shard_rows(shards: &[super::collector::SerializableShardMetrics]) -> String {
        shards
            .iter()
            .enumerate()
            .map(|(id, shard)| {
                let hit_ratio = if shard.operations > 0 {
                    (shard.hits as f64 / shard.operations as f64) * 100.0
                } else {
                    0.0
                };
                
                let hit_ratio_class = if hit_ratio >= 90.0 {
                    "status-good"
                } else if hit_ratio >= 70.0 {
                    "status-warning"
                } else {
                    "status-error"
                };
                
                format!(
                    r#"<tr>
                        <td><strong>Shard {}</strong></td>
                        <td>{}</td>
                        <td><span class="{}">{:.1}%</span></td>
                        <td>{}</td>
                        <td>{:.1} KB</td>
                        <td>{}</td>
                    </tr>"#,
                    id,
                    shard.operations,
                    hit_ratio_class,
                    hit_ratio,
                    shard.items,
                    shard.memory_bytes as f64 / 1024.0,
                    shard.evictions
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}