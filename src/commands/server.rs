//! Connection and server commands: PING, AUTH, HELLO, CLIENT, COMMAND, CONFIG, INFO, ...

use super::Ctx;
use crate::protocol::{Args, reply};
use crate::store::Policy;
use crate::store::compress::CompressionAlgo;
use crate::util::{eq_ic, glob_match, parse_i64, parse_memory};
use std::fmt::Write as _;
use std::sync::atomic::Ordering::Relaxed;

pub fn ping(c: &mut Ctx, args: &Args) {
    match args.len() {
        1 => reply::simple(c.out, b"PONG"),
        2 => reply::bulk(c.out, args.get(1)),
        _ => reply::wrong_arity(c.out, "ping"),
    }
}

pub fn echo(c: &mut Ctx, args: &Args) {
    reply::bulk(c.out, args.get(1));
}

pub fn select(c: &mut Ctx, args: &Args) {
    match parse_i64(args.get(1)) {
        Some(0) => reply::ok(c.out),
        Some(_) => reply::error(c.out, "ERR DB index is out of range"),
        None => reply::not_integer(c.out),
    }
}

pub fn quit(c: &mut Ctx, _args: &Args) {
    c.session.close = true;
    reply::ok(c.out);
}

pub fn reset(c: &mut Ctx, _args: &Args) {
    c.session.name = None;
    c.session.resp3 = false;
    c.session.authenticated = c.shared.config.requirepass.is_none();
    reply::simple(c.out, b"RESET");
}

/// Constant-time comparison so password checks do not leak a matching prefix through timing.
fn secure_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Checks credentials for the single `default` user.
fn check_password(c: &Ctx, user: Option<&[u8]>, pass: &[u8]) -> bool {
    let Some(expected) = &c.shared.config.requirepass else {
        return false;
    };
    user.is_none_or(|u| u == b"default") && secure_eq(pass, expected.as_bytes())
}

pub fn auth(c: &mut Ctx, args: &Args) {
    if args.len() > 3 {
        return reply::syntax_error(c.out);
    }
    if c.shared.config.requirepass.is_none() {
        return reply::error(
            c.out,
            "ERR AUTH <password> called without any password configured for the default user. Are you sure your configuration is correct?",
        );
    }
    let (user, pass) = if args.len() == 3 {
        (Some(args.get(1)), args.get(2))
    } else {
        (None, args.get(1))
    };
    if check_password(c, user, pass) {
        c.session.authenticated = true;
        reply::ok(c.out);
    } else {
        reply::error(
            c.out,
            "WRONGPASS invalid username-password pair or user is disabled.",
        );
    }
}

pub fn hello(c: &mut Ctx, args: &Args) {
    let mut j = 1;
    let mut resp3 = c.session.resp3;
    if args.len() > 1 {
        match parse_i64(args.get(1)) {
            Some(v @ 2..=3) => resp3 = v == 3,
            Some(_) => return reply::error(c.out, "NOPROTO unsupported protocol version"),
            None => {
                return reply::error(
                    c.out,
                    "ERR Protocol version is not an integer or out of range",
                );
            }
        }
        j = 2;
    }
    let mut name = None;
    while j < args.len() {
        let a = args.get(j);
        if eq_ic(a, "AUTH") && j + 2 < args.len() {
            if c.shared.config.requirepass.is_none()
                || !check_password(c, Some(args.get(j + 1)), args.get(j + 2))
            {
                return reply::error(
                    c.out,
                    "WRONGPASS invalid username-password pair or user is disabled.",
                );
            }
            c.session.authenticated = true;
            j += 3;
        } else if eq_ic(a, "SETNAME") && j + 1 < args.len() {
            name = Some(args.get(j + 1).to_vec());
            j += 2;
        } else {
            return reply::error(
                c.out,
                &format!(
                    "ERR Syntax error in HELLO option '{}'",
                    String::from_utf8_lossy(a)
                ),
            );
        }
    }
    if !c.session.authenticated {
        return reply::error(
            c.out,
            "NOAUTH HELLO must be called with the client already authenticated, otherwise the HELLO <proto> AUTH <user> <pass> option can be used to authenticate the client and select the RESP protocol version at the same time",
        );
    }
    if name.is_some() {
        c.session.name = name;
    }
    c.session.resp3 = resp3;
    reply::map(c.out, 7, resp3);
    for (k, v) in [("server", "crabcache"), ("version", crate::VERSION)] {
        reply::bulk(c.out, k.as_bytes());
        reply::bulk(c.out, v.as_bytes());
    }
    reply::bulk(c.out, b"proto");
    reply::int(c.out, if resp3 { 3 } else { 2 });
    reply::bulk(c.out, b"id");
    reply::int(c.out, c.session.id as i64);
    for (k, v) in [("mode", "standalone"), ("role", "master")] {
        reply::bulk(c.out, k.as_bytes());
        reply::bulk(c.out, v.as_bytes());
    }
    reply::bulk(c.out, b"modules");
    reply::array(c.out, 0);
}

fn client_info_line(c: &Ctx) -> String {
    let name = c
        .session
        .name
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    format!(
        "id={} addr={} name={} db=0\n",
        c.session.id, c.session.addr, name
    )
}

pub fn client(c: &mut Ctx, args: &Args) {
    let sub = args.get(1).to_ascii_uppercase();
    match (sub.as_slice(), args.len()) {
        (b"ID", 2) => reply::int(c.out, c.session.id as i64),
        (b"GETNAME", 2) => match &c.session.name {
            Some(n) => reply::bulk(c.out, n),
            None => reply::null(c.out, c.session.resp3),
        },
        (b"SETNAME", 3) => {
            let n = args.get(2);
            if n.iter().any(|&b| b <= b' ' || b > b'~') {
                return reply::error(
                    c.out,
                    "ERR Client names cannot contain spaces, newlines or special characters.",
                );
            }
            c.session.name = (!n.is_empty()).then(|| n.to_vec());
            reply::ok(c.out);
        }
        (b"SETINFO", 4) | (b"NO-EVICT", 3) | (b"NO-TOUCH", 3) => reply::ok(c.out),
        (b"INFO", 2) | (b"LIST", _) => {
            let line = client_info_line(c);
            reply::verbatim_text(c.out, line.as_bytes(), c.session.resp3);
        }
        _ => reply::error(
            c.out,
            &format!(
                "ERR unknown subcommand or wrong number of arguments for '{}'. Try CLIENT HELP.",
                String::from_utf8_lossy(args.get(1))
            ),
        ),
    }
}

/// Command introspection is not implemented; empty answers keep redis-cli and client libraries happy.
pub fn command(c: &mut Ctx, args: &Args) {
    if args.len() == 1 {
        return reply::array(c.out, 0);
    }
    match args.get(1).to_ascii_uppercase().as_slice() {
        b"COUNT" => reply::int(c.out, 0),
        b"DOCS" => reply::map(c.out, 0, c.session.resp3),
        b"INFO" | b"LIST" => reply::array(c.out, 0),
        _ => reply::error(
            c.out,
            &format!(
                "ERR unknown subcommand '{}'. Try COMMAND HELP.",
                String::from_utf8_lossy(args.get(1))
            ),
        ),
    }
}

fn config_params(c: &Ctx) -> Vec<(&'static str, String)> {
    let cfg = &c.shared.config;
    vec![
        ("maxmemory", c.db.maxmemory().to_string()),
        ("maxmemory-policy", c.db.policy().name().to_string()),
        ("maxmemory-samples", c.db.samples().to_string()),
        ("maxclients", cfg.maxclients.to_string()),
        (
            "compression",
            if c.db.codec.enabled() { "yes" } else { "no" }.to_string(),
        ),
        (
            "compression-algorithm",
            c.db.codec.algorithm().name().to_string(),
        ),
        (
            "compression-min-idle",
            c.db.codec.min_idle_secs().to_string(),
        ),
        ("compression-min-size", c.db.codec.min_size().to_string()),
        (
            "compression-max-bytes-per-tick",
            c.db.codec.pack_max_bytes().to_string(),
        ),
        (
            "compression-time-budget-ms",
            c.db.codec.pack_budget_ms().to_string(),
        ),
        (
            "compression-adaptive-dicts",
            if c.db.codec.adaptive_dicts() {
                "yes"
            } else {
                "no"
            }
            .into(),
        ),
        (
            "compression-adaptive-maintenance",
            if c.db.codec.adaptive_maintenance() {
                "yes"
            } else {
                "no"
            }
            .into(),
        ),
        ("compression-min-ttl", c.db.codec.min_ttl_secs().to_string()),
        (
            "compression-hot-maxmemory",
            c.db.codec.hot_max_bytes().to_string(),
        ),
        (
            "compression-latency-target-us",
            c.db.codec.latency_target_us().to_string(),
        ),
        ("port", cfg.port.to_string()),
        ("bind", cfg.bind.clone()),
        ("databases", "1".to_string()),
        ("save", String::new()),
        ("appendonly", "no".to_string()),
        ("proto-max-bulk-len", cfg.proto_max_bulk_len.to_string()),
        (
            "client-query-buffer-limit",
            cfg.client_query_buffer_limit.to_string(),
        ),
    ]
}

pub fn config(c: &mut Ctx, args: &Args) {
    let sub = args.get(1).to_ascii_uppercase();
    match sub.as_slice() {
        b"GET" if args.len() >= 3 => {
            let params = config_params(c);
            let matched: Vec<_> = params
                .iter()
                .filter(|(k, _)| {
                    args.iter()
                        .skip(2)
                        .any(|p| glob_match(p, k.as_bytes(), true))
                })
                .collect();
            reply::map(c.out, matched.len(), c.session.resp3);
            for (k, v) in matched {
                reply::bulk(c.out, k.as_bytes());
                reply::bulk(c.out, v.as_bytes());
            }
        }
        b"SET" if args.len() >= 4 && args.len() % 2 == 0 => {
            enum Update {
                MaxMemory(u64),
                Policy(Policy),
                Samples(usize),
                Compression(bool),
                CompressionAlgorithm(CompressionAlgo),
                CompressionMinIdle(u64),
                CompressionMinSize(usize),
                CompressionMaxBytes(u64),
                CompressionBudgetMs(u64),
                AdaptiveDicts(bool),
                AdaptiveMaintenance(bool),
                MinTtl(u64),
                HotMemory(u64),
                LatencyTargetUs(u64),
            }
            // Validate everything before applying anything, like Redis.
            let mut updates = Vec::new();
            for j in (2..args.len()).step_by(2) {
                let (k, v) = (args.get(j), args.get(j + 1));
                let bad = || {
                    format!(
                        "ERR CONFIG SET failed (possibly related to argument '{}') - argument couldn't be parsed into an integer",
                        String::from_utf8_lossy(k)
                    )
                };
                match k.to_ascii_lowercase().as_slice() {
                    b"maxmemory" => match parse_memory(v) {
                        Some(m) => updates.push(Update::MaxMemory(m)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"maxmemory-policy" => match Policy::parse(v) {
                        Some(p) => updates.push(Update::Policy(p)),
                        None => {
                            return reply::error(
                                c.out,
                                "ERR CONFIG SET failed (possibly related to argument 'maxmemory-policy') - argument(s) must be one of the following: noeviction, allkeys-lru, allkeys-lfu, allkeys-random",
                            );
                        }
                    },
                    b"compression" => match v.to_ascii_lowercase().as_slice() {
                        b"yes" => updates.push(Update::Compression(true)),
                        b"no" => updates.push(Update::Compression(false)),
                        _ => {
                            return reply::error(
                                c.out,
                                "ERR CONFIG SET failed (possibly related to argument 'compression') - argument must be 'yes' or 'no'",
                            );
                        }
                    },
                    b"compression-algorithm" => match CompressionAlgo::parse(v) {
                        Some(algo) => updates.push(Update::CompressionAlgorithm(algo)),
                        None => {
                            return reply::error(
                                c.out,
                                "ERR CONFIG SET failed (possibly related to argument 'compression-algorithm') - argument(s) must be one of the following: auto, zstd, lz4",
                            );
                        }
                    },
                    b"compression-min-idle" => match parse_i64(v).filter(|&n| n >= 0) {
                        Some(n) => updates.push(Update::CompressionMinIdle(n as u64)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"compression-min-size" => match parse_i64(v).filter(|&n| n >= 0) {
                        Some(n) => updates.push(Update::CompressionMinSize(n as usize)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"compression-max-bytes-per-tick" => match parse_memory(v).filter(|&n| n > 0) {
                        Some(n) => updates.push(Update::CompressionMaxBytes(n)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"compression-time-budget-ms" => {
                        match parse_i64(v).filter(|&n| (1..=1000).contains(&n)) {
                            Some(n) => updates.push(Update::CompressionBudgetMs(n as u64)),
                            None => return reply::error(c.out, &bad()),
                        }
                    }
                    b"compression-adaptive-dicts" | b"compression-adaptive-maintenance" => {
                        let on = match v.to_ascii_lowercase().as_slice() {
                            b"yes" => true,
                            b"no" => false,
                            _ => return reply::error(c.out, &bad()),
                        };
                        updates.push(if eq_ic(k, "compression-adaptive-dicts") {
                            Update::AdaptiveDicts(on)
                        } else {
                            Update::AdaptiveMaintenance(on)
                        });
                    }
                    b"compression-min-ttl" => match parse_i64(v).filter(|&n| n >= 0) {
                        Some(n) => updates.push(Update::MinTtl(n as u64)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"compression-hot-maxmemory" => match parse_memory(v) {
                        Some(n) => updates.push(Update::HotMemory(n)),
                        None => return reply::error(c.out, &bad()),
                    },
                    b"compression-latency-target-us" => {
                        match parse_i64(v).filter(|&n| (1..=1_000_000).contains(&n)) {
                            Some(n) => updates.push(Update::LatencyTargetUs(n as u64)),
                            None => return reply::error(c.out, &bad()),
                        }
                    }
                    b"maxmemory-samples" => match parse_i64(v).filter(|&n| (1..=64).contains(&n)) {
                        Some(n) => updates.push(Update::Samples(n as usize)),
                        None => return reply::error(c.out, &bad()),
                    },
                    _ => {
                        return reply::error(
                            c.out,
                            &format!(
                                "ERR Unknown option or number of arguments for CONFIG SET - '{}'",
                                String::from_utf8_lossy(k)
                            ),
                        );
                    }
                }
            }
            for u in updates {
                match u {
                    Update::MaxMemory(m) => c.db.set_maxmemory(m),
                    Update::Policy(p) => c.db.set_policy(p),
                    Update::Samples(n) => c.db.set_samples(n),
                    Update::Compression(on) => c.db.codec.set_enabled(on),
                    Update::CompressionAlgorithm(algo) => c.db.codec.set_algorithm(algo),
                    Update::CompressionMinIdle(n) => c.db.codec.set_min_idle_secs(n),
                    Update::CompressionMinSize(n) => c.db.codec.set_min_size(n),
                    Update::CompressionMaxBytes(n) => c.db.codec.set_pack_max_bytes(n),
                    Update::CompressionBudgetMs(n) => c.db.codec.set_pack_budget_ms(n),
                    Update::AdaptiveDicts(on) => c.db.codec.set_adaptive_dicts(on),
                    Update::AdaptiveMaintenance(on) => c.db.codec.set_adaptive_maintenance(on),
                    Update::MinTtl(n) => c.db.codec.set_min_ttl_secs(n),
                    Update::HotMemory(n) => c.db.codec.set_hot_max_bytes(n),
                    Update::LatencyTargetUs(n) => c.db.codec.set_latency_target_us(n),
                }
            }
            reply::ok(c.out);
        }
        b"RESETSTAT" if args.len() == 2 => {
            c.db.reset_stats();
            c.shared.stats.commands_processed.store(0, Relaxed);
            c.shared.stats.connections_received.store(0, Relaxed);
            c.shared.stats.rejected_connections.store(0, Relaxed);
            reply::ok(c.out);
        }
        b"REWRITE" if args.len() == 2 => {
            reply::error(c.out, "ERR The server is running without a config file")
        }
        _ => reply::error(
            c.out,
            &format!(
                "ERR unknown subcommand or wrong number of arguments for '{}'. Try CONFIG HELP.",
                String::from_utf8_lossy(args.get(1))
            ),
        ),
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes}B")
    } else {
        format!("{v:.2}{}", UNITS[u])
    }
}

pub fn info(c: &mut Ctx, args: &Args) {
    let wanted: Vec<String> = args
        .iter()
        .skip(1)
        .map(|a| String::from_utf8_lossy(a).to_ascii_lowercase())
        .collect();
    let show = |s: &str| {
        wanted.is_empty()
            || wanted
                .iter()
                .any(|w| w == s || w == "all" || w == "everything" || w == "default")
    };
    let st = &c.shared.stats;
    let cfg = &c.shared.config;
    let mut s = String::new();
    if show("server") {
        let up = c.db.uptime_secs();
        let _ = write!(
            s,
            "# Server\r\nredis_version:7.4.0\r\ncrabcache_version:{}\r\nredis_mode:standalone\r\nos:{} {}\r\narch_bits:64\r\nprocess_id:{}\r\ntcp_port:{}\r\nuptime_in_seconds:{up}\r\nuptime_in_days:{}\r\nio_threads_active:{}\r\n\r\n",
            crate::VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::process::id(),
            cfg.port,
            up / 86400,
            cfg.worker_threads(),
        );
    }
    if show("clients") {
        let _ = write!(
            s,
            "# Clients\r\nconnected_clients:{}\r\nmaxclients:{}\r\n\r\n",
            st.connected_clients.load(Relaxed),
            cfg.maxclients
        );
    }
    let (keys, ttl_keys, stats) = c.db.summary();
    if show("memory") {
        let used = c.db.used_memory();
        let max = c.db.maxmemory();
        let _ = write!(
            s,
            "# Memory\r\nused_memory:{used}\r\nused_memory_human:{}\r\nmaxmemory:{max}\r\nmaxmemory_human:{}\r\nmaxmemory_policy:{}\r\n\r\n",
            human(used),
            human(max),
            c.db.policy().name()
        );
    }
    if show("stats") {
        let _ = write!(
            s,
            "# Stats\r\ntotal_connections_received:{}\r\ntotal_commands_processed:{}\r\nrejected_connections:{}\r\nexpired_keys:{}\r\nevicted_keys:{}\r\nkeyspace_hits:{}\r\nkeyspace_misses:{}\r\n\r\n",
            st.connections_received.load(Relaxed),
            st.commands_processed.load(Relaxed),
            st.rejected_connections.load(Relaxed),
            stats.expired,
            stats.evicted,
            stats.hits,
            stats.misses,
        );
    }
    if show("compression") {
        let codec = &c.db.codec;
        let pack = c.db.pack_summary();
        let dicts = codec.dicts();
        let active_dicts = codec.active_dicts().len();
        let retired_dicts = codec.retired_dicts().len();
        let (p95_us, p99_us) = codec.latency_percentiles_us();
        let ratio = if pack.stored_bytes > 0 {
            pack.original_bytes as f64 / pack.stored_bytes as f64
        } else {
            1.0
        };
        let _ = write!(
            s,
            "# Compression\r\ncompression:{}\r\ncompression_algorithm:{}\r\ncompression_min_idle:{}\r\ncompression_min_size:{}\r\ncompression_dicts:{}\r\ncompression_dicts_active:{active_dicts}\r\ncompression_dicts_retired:{retired_dicts}\r\ncompression_dicts_retrained:{}\r\ncompression_dicts_reclaimed:{}\r\ncompression_dict_drifts:{}\r\ncompression_dicts_rejected:{}\r\ncompressed_keys:{}\r\ncompressed_zstd_keys:{}\r\ncompressed_lz4_keys:{}\r\ncompressed_original_bytes:{}\r\ncompressed_stored_bytes:{}\r\ncompression_net_memory_saved_bytes:{}\r\ncompression_ratio:{:.2}\r\n",
            if codec.enabled() { "yes" } else { "no" },
            codec.algorithm().name(),
            codec.min_idle_secs(),
            codec.min_size(),
            dicts.len(),
            codec.stats.dicts_retrained.load(Relaxed),
            codec.dicts_reclaimed(),
            codec.stats.dict_drifts.load(Relaxed),
            codec.stats.dicts_rejected.load(Relaxed),
            pack.keys,
            pack.zstd_keys,
            pack.lz4_keys,
            pack.original_bytes,
            pack.stored_bytes,
            c.db.net_compression_savings_bytes(),
            ratio,
        );
        let (sampler_prefixes, sample_bytes) = codec.sampler_memory();
        let prepared_bytes: usize = dicts.iter().map(|d| d.prepared_bytes).sum();
        let _ = write!(
            s,
            "compression_max_bytes_per_tick:{}\r\ncompression_time_budget_ms:{}\r\ncompression_access_clock_bytes:{}\r\ncompression_sampler_prefixes:{}\r\ncompression_sample_bytes:{}\r\ncompression_prepared_dict_bytes:{}\r\ncompression_training_jobs:{}\r\ncompression_training_bytes:{}\r\ncompression_training_us:{}\r\ncompression_pack_us:{}\r\ncompression_pack_input_bytes:{}\r\ncompression_candidates_discarded:{}\r\ncompression_oversized_skips:{}\r\ncompression_cost_benefit_skips:{}\r\ncompression_decompress_calls:{}\r\ncompression_decompress_us:{}\r\n",
            codec.pack_max_bytes(),
            codec.pack_budget_ms(),
            c.db.access_clock_bytes(),
            sampler_prefixes,
            sample_bytes,
            prepared_bytes,
            codec.stats.training_jobs.load(Relaxed),
            codec.stats.training_bytes.load(Relaxed),
            codec.stats.training_micros.load(Relaxed),
            codec.stats.pack_micros.load(Relaxed),
            codec.stats.pack_input_bytes.load(Relaxed),
            codec.stats.candidates_discarded.load(Relaxed),
            codec.stats.oversized_skips.load(Relaxed),
            codec.stats.cost_benefit_skips.load(Relaxed),
            codec.stats.decompress_calls.load(Relaxed),
            codec.stats.decompress_micros.load(Relaxed),
        );
        let _ = write!(
            s,
            "compression_adaptive_dicts:{}\r\ncompression_adaptive_maintenance:{}\r\ncompression_min_ttl:{}\r\ncompression_hot_maxmemory:{}\r\ncompression_hot_resident_bytes:{}\r\ncompression_hot_promotions:{}\r\ncompression_short_ttl_skips:{}\r\ncompression_maintenance_backoffs:{}\r\ncompression_latency_target_us:{}\r\ncompression_observed_p95_us:{p95_us}\r\ncompression_observed_p99_us:{p99_us}\r\n",
            if codec.adaptive_dicts() { "yes" } else { "no" },
            if codec.adaptive_maintenance() {
                "yes"
            } else {
                "no"
            },
            codec.min_ttl_secs(),
            codec.hot_max_bytes(),
            c.db.hot_resident_bytes(),
            codec.stats.hot_promotions.load(Relaxed),
            codec.stats.short_ttl_skips.load(Relaxed),
            codec.stats.maintenance_backoffs.load(Relaxed),
            codec.latency_target_us(),
        );
        for d in &dicts {
            let _ = write!(
                s,
                "dict{}:prefix={},trained_ratio={:.2},raw_bytes={},level={},prepared_bytes={},version={},active_keys={},retired={},net_saved_bytes={}\r\n",
                d.id,
                String::from_utf8_lossy(&d.prefix),
                d.trained_ratio,
                d.raw_bytes,
                d.level,
                d.prepared_bytes,
                d.version,
                d.active_key_count(),
                if d.is_retired() { 1 } else { 0 },
                d.net_saved_bytes(),
            );
        }
        s.push_str("\r\n");
    }
    if show("keyspace") {
        s.push_str("# Keyspace\r\n");
        if keys > 0 {
            let _ = write!(s, "db0:keys={keys},expires={ttl_keys},avg_ttl=0\r\n");
        }
    }
    reply::verbatim_text(c.out, s.as_bytes(), c.session.resp3);
}

pub fn time(c: &mut Ctx, _args: &Args) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    reply::array(c.out, 2);
    reply::bulk_int(c.out, now.as_secs() as i64);
    reply::bulk_int(c.out, now.subsec_micros() as i64);
}

pub fn memory(c: &mut Ctx, args: &Args) {
    if eq_ic(args.get(1), "USAGE") && (args.len() == 3 || args.len() == 5) {
        let key = args.get(2);
        let (mut g, h) = c.db.lock_key(key);
        match g.lookup(h, key, c.clock.ms) {
            Some(i) => reply::int(
                c.out,
                (g.entry(i).mem_usage() + if g.tracks_access() { 4 } else { 0 }) as i64,
            ),
            None => reply::null(c.out, c.session.resp3),
        }
    } else {
        reply::error(
            c.out,
            &format!(
                "ERR unknown subcommand or wrong number of arguments for '{}'. Try MEMORY HELP.",
                String::from_utf8_lossy(args.get(1))
            ),
        );
    }
}
