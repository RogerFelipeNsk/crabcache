//! Regression coverage for CrabPack scheduling, clocks and owned read snapshots.
mod common;

use common::{TestServer, session_json, wait_for_packed};
use redis::Commands;
use std::time::Duration;

fn field(con: &mut redis::Connection, name: &str) -> u64 {
    let info: String = redis::cmd("INFO").arg("compression").query(con).unwrap();
    info.lines()
        .find_map(|l| {
            l.strip_prefix(&format!("{name}:"))
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap()
}

fn load_sessions(con: &mut redis::Connection, n: u64) {
    let mut pipe = redis::pipe();
    for i in 0..n {
        pipe.cmd("SET")
            .arg(format!("session:{i}"))
            .arg(session_json(i))
            .arg("EX")
            .arg(30)
            .ignore();
    }
    let _: () = pipe.query(con).unwrap();
}

#[test]
fn lfu_five_second_idle_compacts_before_first_minute_and_preserves_ttl() {
    let srv = TestServer::start(&[
        "--compression",
        "--maxmemory-policy",
        "allkeys-lfu",
        "--compression-min-idle",
        "5",
    ]);
    let mut con = srv.client();
    const N: u64 = 2000;
    load_sessions(&mut con, N);
    assert_eq!(
        wait_for_packed(&mut srv.raw(), N, Duration::from_secs(15)),
        N
    );
    assert_eq!(field(&mut con, "compression_access_clock_bytes"), N * 4);
    assert!(field(&mut con, "compression_prepared_dict_bytes") > 16384);
    for i in 0..N {
        let got: Vec<u8> = con.get(format!("session:{i}")).unwrap();
        assert_eq!(got, session_json(i));
    }
    let ttl: i64 = redis::cmd("TTL").arg("session:0").query(&mut con).unwrap();
    assert!((1..=30).contains(&ttl));
}

#[test]
fn large_get_mget_range_and_getdel_snapshots_survive_concurrent_writes() {
    let srv = TestServer::start(&[
        "--compression",
        "--compression-min-idle",
        "0",
        "--compression-max-bytes-per-tick",
        "1mb",
    ]);
    let mut con = srv.client();
    const N: u64 = 2000;
    load_sessions(&mut con, N);
    let a = vec![b'a'; 128 * 1024];
    let b = vec![b'b'; 128 * 1024];
    let _: () = con.set("session:large", &a).unwrap();
    assert_eq!(
        wait_for_packed(&mut srv.raw(), N + 1, Duration::from_secs(20)),
        N + 1
    );
    let addr = srv.addr;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let mut w = redis::Client::open(format!("redis://{addr}/"))
                .unwrap()
                .get_connection()
                .unwrap();
            for i in 0..100 {
                let _: () = w
                    .set("session:large", if i % 2 == 0 { &a } else { &b })
                    .unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        for _ in 0..4 {
            scope.spawn(|| {
                let mut r = redis::Client::open(format!("redis://{addr}/"))
                    .unwrap()
                    .get_connection()
                    .unwrap();
                for i in 0..100 {
                    let value: Vec<u8> = r.get("session:large").unwrap();
                    assert!(value == a || value == b, "torn GET");
                    if i % 10 == 0 {
                        let values: Vec<Vec<u8>> = redis::cmd("MGET")
                            .arg("session:large")
                            .arg("session:large")
                            .query(&mut r)
                            .unwrap();
                        assert!(values.iter().all(|v| v == &a || v == &b), "torn MGET");
                        let range: Vec<u8> = redis::cmd("GETRANGE")
                            .arg("session:large")
                            .arg(65530)
                            .arg(65600)
                            .query(&mut r)
                            .unwrap();
                        assert!(
                            range == vec![b'a'; 71] || range == vec![b'b'; 71],
                            "torn GETRANGE"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            });
        }
    });
    let value: Vec<u8> = redis::cmd("GETDEL")
        .arg("session:large")
        .query(&mut con)
        .unwrap();
    assert!(value == a || value == b);
    assert_eq!(
        con.get::<_, Option<Vec<u8>>>("session:large").unwrap(),
        None
    );
}

#[test]
fn packing_budgets_are_configurable_and_invalid_multi_updates_are_atomic() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let result: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-min-idle")
        .arg(11)
        .arg("compression-max-bytes-per-tick")
        .arg(0)
        .query(&mut con);
    assert!(result.is_err());
    assert_eq!(field(&mut con, "compression_min_idle"), 60);
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-max-bytes-per-tick")
        .arg("128kb")
        .arg("compression-time-budget-ms")
        .arg(3)
        .query(&mut con)
        .unwrap();
    assert_eq!(
        field(&mut con, "compression_max_bytes_per_tick"),
        128 * 1024
    );
    assert_eq!(field(&mut con, "compression_time_budget_ms"), 3);
    let result: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-time-budget-ms")
        .arg(1001)
        .query(&mut con);
    assert!(result.is_err());
}

#[test]
fn compression_access_clocks_are_counted_under_maxmemory_and_cleared_on_flush() {
    let srv = TestServer::start(&[
        "--compression",
        "--maxmemory-policy",
        "allkeys-lfu",
        "--maxmemory",
        "256kb",
    ]);
    let mut con = srv.client();
    for i in 0..2000 {
        let _: () = con.set(format!("k{i}"), vec![b'x'; 1000]).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    let info: String = redis::cmd("INFO").query(&mut con).unwrap();
    let used: u64 = info
        .lines()
        .find_map(|l| {
            l.strip_prefix("used_memory:")
                .map(|v| v.trim().parse().unwrap())
        })
        .unwrap();
    assert!(used <= 256 * 1024, "used memory {used}");
    let n: u64 = redis::cmd("DBSIZE").query(&mut con).unwrap();
    assert_eq!(field(&mut con, "compression_access_clock_bytes"), n * 4);
    let _: () = redis::cmd("FLUSHALL").query(&mut con).unwrap();
    assert_eq!(field(&mut con, "compression_access_clock_bytes"), 0);
}

#[test]
fn large_values_train_from_bounded_fragments_without_two_sample_rejection() {
    let srv = TestServer::start(&["--compression", "--compression-min-idle", "0"]);
    let mut con = srv.client();
    const N: u64 = 24;
    let mut pipe = redis::pipe();
    for i in 0..N {
        // More than 64 KiB per value: the old byte threshold starts training
        // after only a few samples, then rejects that batch for having <10.
        pipe.set(format!("session:{i}"), session_json(i).repeat(300))
            .ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();
    assert_eq!(
        wait_for_packed(&mut srv.raw(), N, Duration::from_secs(15)),
        N
    );
    for i in 0..N {
        assert_eq!(
            con.get::<_, Vec<u8>>(format!("session:{i}")).unwrap(),
            session_json(i).repeat(300)
        );
    }
    assert_eq!(field(&mut con, "compression_dicts_rejected"), 0);
    assert_eq!(field(&mut con, "compression_training_jobs"), 0);
    assert_eq!(field(&mut con, "compression_training_bytes"), 0);
}

#[test]
fn adaptive_settings_validate_atomically_and_report_in_info() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let result: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-min-ttl")
        .arg(9)
        .arg("compression-adaptive-dicts")
        .arg("invalid")
        .query(&mut con);
    assert!(result.is_err());
    assert_eq!(field(&mut con, "compression_min_ttl"), 2);
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-adaptive-dicts")
        .arg("no")
        .arg("compression-adaptive-maintenance")
        .arg("no")
        .arg("compression-min-ttl")
        .arg(7)
        .arg("compression-hot-maxmemory")
        .arg("1mb")
        .query(&mut con)
        .unwrap();
    assert_eq!(field(&mut con, "compression_min_ttl"), 7);
    assert_eq!(field(&mut con, "compression_hot_maxmemory"), 1024 * 1024);
    let settings: Vec<String> = redis::cmd("CONFIG")
        .arg("GET")
        .arg("compression-adaptive-dicts")
        .query(&mut con)
        .unwrap();
    assert_eq!(settings, ["compression-adaptive-dicts", "no"]);
}

#[test]
fn hot_lfu_promotion_preserves_binary_ttl_and_pipeline_replies() {
    let srv = TestServer::start(&[
        "--compression",
        "--compression-min-idle",
        "0",
        "--maxmemory-policy",
        "allkeys-lfu",
        "--compression-hot-maxmemory",
        "1mb",
    ]);
    let mut con = srv.client();
    load_sessions(&mut con, 2000);
    assert_eq!(
        wait_for_packed(&mut srv.raw(), 2000, Duration::from_secs(20)),
        2000
    );
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression-min-idle")
        .arg(60)
        .query(&mut con)
        .unwrap();
    let original = session_json(0);
    for _ in 0..1000 {
        assert_eq!(con.get::<_, Vec<u8>>("session:0").unwrap(), original);
        if field(&mut con, "compression_hot_promotions") != 0 {
            break;
        }
    }
    assert_eq!(field(&mut con, "compression_hot_promotions"), 1);
    assert!(field(&mut con, "compression_hot_resident_bytes") <= 1024 * 1024);
    let ttl: i64 = redis::cmd("TTL").arg("session:0").query(&mut con).unwrap();
    assert!((1..=30).contains(&ttl));
    let responses: (Vec<u8>, Vec<u8>, Vec<u8>) = redis::pipe()
        .cmd("GET")
        .arg("session:1")
        .cmd("GET")
        .arg("session:0")
        .cmd("GETDEL")
        .arg("session:2")
        .query(&mut con)
        .unwrap();
    assert_eq!(responses, (session_json(1), original, session_json(2)));
    let _: () = con
        .set("session:0", b"replacement\0\xff".as_slice())
        .unwrap();
    assert_eq!(field(&mut con, "compression_hot_resident_bytes"), 0);
    assert_eq!(
        con.get::<_, Vec<u8>>("session:0").unwrap(),
        b"replacement\0\xff"
    );
}

#[test]
fn short_lived_values_are_skipped_after_a_dictionary_is_ready() {
    let srv = TestServer::start(&[
        "--compression",
        "--compression-min-idle",
        "0",
        "--compression-min-ttl",
        "2",
    ]);
    let mut con = srv.client();
    load_sessions(&mut con, 2000);
    assert_eq!(
        wait_for_packed(&mut srv.raw(), 2000, Duration::from_secs(20)),
        2000
    );
    let value = session_json(99999);
    let _: () = redis::cmd("SET")
        .arg("session:short")
        .arg(&value)
        .arg("PX")
        .arg(1500)
        .query(&mut con)
        .unwrap();
    let start = std::time::Instant::now();
    while field(&mut con, "compression_short_ttl_skips") == 0
        && start.elapsed() < Duration::from_secs(1)
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(field(&mut con, "compression_short_ttl_skips") > 0);
    assert_eq!(con.get::<_, Vec<u8>>("session:short").unwrap(), value);
    assert_eq!(field(&mut con, "compressed_keys"), 2000);
}
