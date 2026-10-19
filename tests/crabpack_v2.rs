//! End-to-end integration tests for CrabPack v2:
//! 1. Adaptive compression (Zstd dictionary vs LZ4 block vs Plain skip)
//! 2. Dynamic dictionaries (drift detection, background retraining, version retention, lazy migration, and slot reclamation)
//! 3. Cost-benefit controller (real net memory savings accounting, P95/P99 latency tracking, and decompression telemetry)

mod common;

use common::{TestServer, session_json, wait_for_packed};
use redis::Commands;
use std::time::{Duration, Instant};

fn info_compression(con: &mut redis::Connection) -> String {
    redis::cmd("INFO").arg("compression").query(con).unwrap()
}

fn field_i64(con: &mut redis::Connection, name: &str) -> i64 {
    let info = info_compression(con);
    info.lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{name}:"))
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap_or_else(|| panic!("missing INFO compression field {name} in:\n{info}"))
}

fn field_u64(con: &mut redis::Connection, name: &str) -> u64 {
    field_i64(con, name) as u64
}

fn large_payload(i: u64) -> Vec<u8> {
    format!(
        r#"{{"blob_id":{i},"service":"catalog-render","region":"sa-east-1","blocks":["alpha","beta","gamma","delta"],"payload":"{}"}}"#,
        "0123456789abcdef".repeat(380)
    )
    .into_bytes()
}

fn incompressible_payload(seed: u64, len: usize) -> Vec<u8> {
    let mut s = seed ^ 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 56) as u8
        })
        .collect()
}

fn drifted_xml_payload(i: u64) -> Vec<u8> {
    let warehouses = ["GRU-NORTH", "VCP-CENTRAL", "GIG-COAST", "CNF-INLAND"];
    let carriers = ["EXPRESS-AIR", "FREIGHT-RAIL", "LOCAL-VAN", "CARGO-SEA"];
    format!(
        r#"<shipment manifest_code="SHP-{:08}" origin_facility="{}" carrier_mode="{}" customs_status="CLEARED-DOMESTIC" pallet_weight_grams="{}" insurance_policy="POL-{:012X}" hazmat_class="NONE-STANDARD"/>"#,
        i % 100_000_000,
        warehouses[(i % 4) as usize],
        carriers[((i / 4) % 4) as usize],
        10_000 + (i * 137 % 900_000),
        i.wrapping_mul(0x517C_C1B7_2722_0A95) & 0xFFFF_FFFF_FFFF
    )
    .into_bytes()
}

#[test]
fn adaptive_auto_mode_routes_small_to_zstd_large_to_lz4_and_skips_random() {
    let srv = TestServer::start(&[
        "--compression",
        "--compression-algorithm",
        "auto",
        "--compression-min-idle",
        "0",
        "--maxmemory-policy",
        "allkeys-lfu",
    ]);
    let mut con = srv.client();

    const SMALL_N: u64 = 1200;
    const LARGE_N: u64 = 80;
    const RANDOM_N: u64 = 40;

    let mut pipe = redis::pipe();
    for i in 0..SMALL_N {
        pipe.set(format!("session:{i}"), session_json(i)).ignore();
    }
    for i in 0..LARGE_N {
        pipe.set(format!("blob:{i}"), large_payload(i)).ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();

    let expected_packed = SMALL_N + LARGE_N;
    assert_eq!(
        wait_for_packed(&mut srv.raw(), expected_packed, Duration::from_secs(20)),
        expected_packed
    );

    // Insert unseen high-entropy keys under `session:`; they must be evaluated and skipped by cost-benefit.
    let mut pipe = redis::pipe();
    for i in 0..RANDOM_N {
        pipe.set(
            format!("session:rnd_{i}"),
            incompressible_payload(i + 1, 512),
        )
        .ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    while field_u64(&mut con, "compression_cost_benefit_skips") < RANDOM_N
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(25));
    }

    let zstd_keys = field_u64(&mut con, "compressed_zstd_keys");
    let lz4_keys = field_u64(&mut con, "compressed_lz4_keys");
    let skips = field_u64(&mut con, "compression_cost_benefit_skips");
    assert_eq!(zstd_keys, SMALL_N, "small structured keys must use Zstd");
    assert_eq!(
        lz4_keys, LARGE_N,
        "large keys without dict must use fast LZ4"
    );
    assert_eq!(field_u64(&mut con, "compressed_keys"), expected_packed);
    assert!(
        skips >= RANDOM_N,
        "incompressible random values must be skipped by cost-benefit (got {skips})"
    );
    assert!(
        field_i64(&mut con, "compression_net_memory_saved_bytes") > 0,
        "net memory savings after subtracting dicts and metadata must be positive"
    );

    // Verify byte-for-byte integrity across Zstd, LZ4, and Plain keys.
    for i in (0..SMALL_N).step_by(17) {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("session:{i}")).unwrap(),
            session_json(i)
        );
    }
    for i in 0..LARGE_N {
        let key = format!("blob:{i}");
        let expected = large_payload(i);
        assert_eq!(con.get::<_, Vec<u8>>(&key).unwrap(), expected);
        let slice: Vec<u8> = redis::cmd("GETRANGE")
            .arg(&key)
            .arg(10)
            .arg(99)
            .query(&mut con)
            .unwrap();
        assert_eq!(slice, &expected[10..=99]);
    }
    for i in 0..RANDOM_N {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("session:rnd_{i}")).unwrap(),
            incompressible_payload(i + 1, 512)
        );
    }
}

#[test]
fn dynamic_dictionary_drift_retrains_v2_migrates_v1_and_reclaims_retired_dict() {
    let srv = TestServer::start(&[
        "--compression",
        "--compression-algorithm",
        "auto",
        "--compression-min-idle",
        "0",
        "--compression-max-bytes-per-tick",
        "2mb",
        "--compression-time-budget-ms",
        "20",
    ]);
    let mut con = srv.client();

    const V1_N: u64 = 1100;
    const V2_N: u64 = 1100;

    let mut pipe = redis::pipe();
    for i in 0..V1_N {
        pipe.set(format!("order:{i}"), session_json(i)).ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();

    assert_eq!(
        wait_for_packed(&mut srv.raw(), V1_N, Duration::from_secs(25)),
        V1_N
    );
    assert_eq!(field_u64(&mut con, "compression_dicts_active"), 1);
    assert_eq!(field_u64(&mut con, "compression_dicts_retired"), 0);

    // Introduce schema drift on the same `order:` prefix.
    let mut pipe = redis::pipe();
    for i in 0..V2_N {
        pipe.set(format!("order:v2_{i}"), drifted_xml_payload(i))
            .ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();

    // Phase 1: Wait for drift detection and v2 dictionary retraining while v1 keys remain compressed on retired v1 dict.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let packed = field_u64(&mut con, "compressed_keys");
        let retrained = field_u64(&mut con, "compression_dicts_retrained");
        let active = field_u64(&mut con, "compression_dicts_active");
        let retired = field_u64(&mut con, "compression_dicts_retired");
        if packed == V1_N + V2_N && retrained >= 1 && active == 1 && retired == 1 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let info_phase1 = info_compression(&mut con);
    assert_eq!(
        field_u64(&mut con, "compressed_keys"),
        V1_N + V2_N,
        "both v1 and v2 keys must be compressed in Phase 1:\n{info_phase1}"
    );
    assert_eq!(
        field_u64(&mut con, "compression_dicts_active"),
        1,
        "expected 1 active dictionary in Phase 1:\n{info_phase1}"
    );
    assert_eq!(
        field_u64(&mut con, "compression_dicts_retired"),
        1,
        "retired v1 dictionary must remain resident while v1 values depend on it:\n{info_phase1}"
    );
    assert!(
        info_phase1.contains("version=1") && info_phase1.contains("version=2"),
        "expected both version=1 (retired) and version=2 (active) in:\n{info_phase1}"
    );

    // Verify both v1 (via retired dict0) and v2 (via active dict1) decode without corruption.
    for i in (0..V1_N).step_by(23) {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("order:{i}")).unwrap(),
            session_json(i)
        );
    }
    for i in (0..V2_N).step_by(23) {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("order:v2_{i}")).unwrap(),
            drifted_xml_payload(i)
        );
    }

    // Phase 2: Migrate the old v1 keys to the v2 schema; once no entries depend on v1 dict,
    // its retired slot must be reclaimed automatically.
    let mut pipe = redis::pipe();
    for i in 0..V1_N {
        pipe.set(format!("order:{i}"), drifted_xml_payload(i + 10_000))
            .ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();

    let deadline = Instant::now() + Duration::from_secs(25);
    while Instant::now() < deadline {
        let packed = field_u64(&mut con, "compressed_keys");
        let reclaimed = field_u64(&mut con, "compression_dicts_reclaimed");
        let retired = field_u64(&mut con, "compression_dicts_retired");
        if packed == V1_N + V2_N && reclaimed >= 1 && retired == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let info_phase2 = info_compression(&mut con);
    assert!(
        field_u64(&mut con, "compression_dict_drifts") >= 1,
        "expected drift detection in:\n{info_phase2}"
    );
    assert!(
        field_u64(&mut con, "compression_dicts_retrained") >= 1,
        "expected dictionary retraining in:\n{info_phase2}"
    );
    assert!(
        field_u64(&mut con, "compression_dicts_reclaimed") >= 1,
        "expected retired dictionary slot reclamation in:\n{info_phase2}"
    );
    assert_eq!(
        field_u64(&mut con, "compression_dicts_retired"),
        0,
        "retired dictionary must be freed once all v1 entries leave it:\n{info_phase2}"
    );
    assert!(
        info_phase2.contains("version=2"),
        "expected active dictionary at version=2 in:\n{info_phase2}"
    );

    // Verify all keys decode cleanly after v1 slot reclamation.
    for i in (0..V1_N).step_by(23) {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("order:{i}")).unwrap(),
            drifted_xml_payload(i + 10_000)
        );
    }
    for i in (0..V2_N).step_by(23) {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("order:v2_{i}")).unwrap(),
            drifted_xml_payload(i)
        );
    }
}

#[test]
fn cost_benefit_config_validation_and_latency_telemetry() {
    let srv = TestServer::start(&["--compression", "--compression-min-idle", "0"]);
    let mut con = srv.client();

    // Invalid algorithm or latency target must fail atomically without mutating other settings.
    let bad_algo: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-min-idle")
        .arg(15)
        .arg("compression-algorithm")
        .arg("brotli")
        .query(&mut con);
    assert!(bad_algo.is_err());
    assert_eq!(field_u64(&mut con, "compression_min_idle"), 0);

    let bad_lat: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-latency-target-us")
        .arg(0)
        .query(&mut con);
    assert!(bad_lat.is_err());

    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-algorithm")
        .arg("lz4")
        .arg("compression-latency-target-us")
        .arg(250)
        .query(&mut con)
        .unwrap();
    assert_eq!(field_u64(&mut con, "compression_latency_target_us"), 250);

    for i in 0..120 {
        let _: () = con.set(format!("item:{i}"), large_payload(i)).unwrap();
    }
    assert_eq!(
        wait_for_packed(&mut srv.raw(), 120, Duration::from_secs(10)),
        120
    );

    // Execute enough reads to trigger sampled request latency observations (1 in 64 commands)
    // and decompression telemetry.
    for round in 0..3 {
        let mut pipe = redis::pipe();
        for i in 0..120 {
            pipe.get(format!("item:{}", (i + round) % 120));
        }
        let replies: Vec<Vec<u8>> = pipe.query(&mut con).unwrap();
        assert_eq!(replies.len(), 120);
    }

    assert!(field_u64(&mut con, "compression_decompress_calls") >= 360);
    assert!(field_u64(&mut con, "compression_observed_p95_us") >= 1);
    assert!(
        field_u64(&mut con, "compression_observed_p99_us")
            >= field_u64(&mut con, "compression_observed_p95_us")
    );
    assert!(field_i64(&mut con, "compression_net_memory_saved_bytes") > 0);
}
