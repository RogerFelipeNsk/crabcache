//! End-to-end tests over TCP with the official `redis` Rust client and a raw RESP client.

mod common;

use common::{Raw, TestServer, array_items};
use redis::{Commands, RedisResult, Value};
use std::time::{Duration, Instant};

fn info_field(con: &mut redis::Connection, field: &str) -> u64 {
    let info: String = redis::cmd("INFO").query(con).unwrap();
    info.lines()
        .find_map(|l| l.strip_prefix(&format!("{field}:")))
        .unwrap_or_else(|| panic!("INFO has no {field}"))
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn values_are_binary_safe_and_keep_spaces_and_protocol_words() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("spaces", b"hello big world".to_vec()),
        ("binary", vec![0xff, 0x00, 0xfe, b'\r', b'\n', 0x01]),
        ("null-word", b"NULL".to_vec()),
        ("ok-word", b"OK".to_vec()),
        ("empty", Vec::new()),
        ("big", vec![b'x'; 4 * 1024 * 1024]),
    ];
    for (k, v) in &cases {
        let _: () = con.set(*k, v.as_slice()).unwrap();
    }
    for (k, v) in &cases {
        let got: Vec<u8> = con.get(*k).unwrap();
        assert_eq!(&got, v, "key {k}");
    }
    let missing: Option<Vec<u8>> = con.get("nope").unwrap();
    assert_eq!(
        missing, None,
        "missing key must be nil, distinct from the value \"NULL\""
    );
}

#[test]
fn overwrite_returns_latest_value() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    for i in 0..100 {
        let _: () = con.set("k", i).unwrap();
        let got: i64 = con.get("k").unwrap();
        assert_eq!(got, i);
    }
}

#[test]
fn set_options_and_ttl() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let r: Option<String> = redis::cmd("SET")
        .arg("k")
        .arg("v")
        .arg("NX")
        .query(&mut con)
        .unwrap();
    assert_eq!(r.as_deref(), Some("OK"));
    let r: Option<String> = redis::cmd("SET")
        .arg("k")
        .arg("v2")
        .arg("NX")
        .query(&mut con)
        .unwrap();
    assert_eq!(r, None);
    let r: Option<String> = redis::cmd("SET")
        .arg("k")
        .arg("v3")
        .arg("XX")
        .arg("GET")
        .query(&mut con)
        .unwrap();
    assert_eq!(r.as_deref(), Some("v"));
    let _: () = redis::cmd("SET")
        .arg("k")
        .arg("v4")
        .arg("EX")
        .arg(100)
        .query(&mut con)
        .unwrap();
    let ttl: i64 = con.ttl("k").unwrap();
    assert!((99..=100).contains(&ttl), "ttl {ttl}");
    let _: () = redis::cmd("SET")
        .arg("k")
        .arg("v5")
        .arg("KEEPTTL")
        .query(&mut con)
        .unwrap();
    let ttl: i64 = con.ttl("k").unwrap();
    assert!(ttl > 0, "KEEPTTL must keep expiry");
    let _: () = con.set("k", "v6").unwrap();
    assert_eq!(
        con.ttl::<_, i64>("k").unwrap(),
        -1,
        "plain SET clears expiry"
    );
    assert_eq!(con.ttl::<_, i64>("missing").unwrap(), -2);
    // A relative expiry that overflows when added to the current time is an error (Redis on Linux;
    // clang-built Redis accepts it because its check relies on undefined signed overflow).
    for cmd in [
        redis::cmd("SET")
            .arg("k")
            .arg("v")
            .arg("PX")
            .arg(i64::MAX)
            .clone(),
        redis::cmd("PSETEX").arg("k").arg(i64::MAX).arg("v").clone(),
        redis::cmd("SET")
            .arg("k")
            .arg("v")
            .arg("EX")
            .arg(i64::MAX / 1000)
            .clone(),
    ] {
        let e = cmd.query::<()>(&mut con).unwrap_err().to_string();
        assert!(e.contains("invalid expire time"), "{e}");
    }
    let err: RedisResult<()> = redis::cmd("SET")
        .arg("k")
        .arg("v")
        .arg("EX")
        .arg(0)
        .query(&mut con);
    assert!(
        err.unwrap_err()
            .to_string()
            .contains("invalid expire time in 'set' command")
    );
}

#[test]
fn keys_expire_lazily_and_actively() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let _: () = redis::cmd("SET")
        .arg("lazy")
        .arg("v")
        .arg("PX")
        .arg(50)
        .query(&mut con)
        .unwrap();
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(con.get::<_, Option<String>>("lazy").unwrap(), None);

    // Never read again: active expiration must delete them.
    for i in 0..2000 {
        let _: () = redis::cmd("SET")
            .arg(format!("k{i}"))
            .arg("v")
            .arg("PX")
            .arg(100)
            .query(&mut con)
            .unwrap();
    }
    let _: () = con.set("persistent", "v").unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let n: usize = redis::cmd("DBSIZE").query(&mut con).unwrap();
        if n == 1 {
            break;
        }
        assert!(Instant::now() < deadline, "active expiry left {n} keys");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(info_field(&mut con, "expired_keys") >= 2001);
}

#[test]
fn counters_are_atomic_across_concurrent_clients() {
    let srv = TestServer::start(&[]);
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let url = srv.url();
            std::thread::spawn(move || {
                let mut con = redis::Client::open(url).unwrap().get_connection().unwrap();
                for _ in 0..2000 {
                    let _: i64 = con.incr("counter", 1).unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let mut con = srv.client();
    assert_eq!(con.get::<_, i64>("counter").unwrap(), 16_000);
}

#[test]
fn incr_errors() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let _: () = con.set("s", "abc").unwrap();
    let e = con.incr::<_, _, i64>("s", 1).unwrap_err();
    assert!(
        e.to_string()
            .contains("value is not an integer or out of range"),
        "{e}"
    );
    let _: () = con.set("max", i64::MAX).unwrap();
    let e = con.incr::<_, _, i64>("max", 1).unwrap_err();
    assert!(
        e.to_string()
            .contains("increment or decrement would overflow"),
        "{e}"
    );
}

#[test]
fn pipelined_batch_replies_in_order() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let mut pipe = redis::pipe();
    for i in 0..10_000 {
        pipe.set(format!("p{i}"), i).ignore();
    }
    for i in 0..10_000 {
        pipe.get(format!("p{i}"));
    }
    let got: Vec<i64> = pipe.query(&mut con).unwrap();
    assert_eq!(got, (0..10_000).collect::<Vec<_>>());
}

#[test]
fn mset_mget_rename_across_shards() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let pairs: Vec<(String, String)> = (0..100)
        .map(|i| (format!("m{i}"), format!("v{i}")))
        .collect();
    let _: () = con.mset(&pairs).unwrap();
    let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    let vals: Vec<String> = con.mget(&keys).unwrap();
    assert_eq!(
        vals,
        pairs.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>()
    );
    for i in 0..100 {
        let _: () = con.rename(format!("m{i}"), format!("renamed{i}")).unwrap();
    }
    for i in 0..100 {
        assert_eq!(
            con.get::<_, String>(format!("renamed{i}")).unwrap(),
            format!("v{i}")
        );
        assert!(!con.exists::<_, bool>(format!("m{i}")).unwrap());
    }
    let e = con.rename::<_, _, ()>("missing", "x").unwrap_err();
    assert!(e.to_string().contains("no such key"));
}

#[test]
fn keys_and_scan_see_every_key() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    for i in 0..5000 {
        let _: () = con.set(format!("user:{i}"), i).unwrap();
    }
    let _: () = con.set("other", 1).unwrap();
    let keys: Vec<String> = con.keys("user:*").unwrap();
    assert_eq!(keys.len(), 5000);
    let mut scanned: Vec<String> = con
        .scan_match("user:*")
        .unwrap()
        .collect::<RedisResult<_>>()
        .unwrap();
    scanned.sort();
    scanned.dedup();
    assert_eq!(scanned.len(), 5000);
}

#[test]
fn requirepass_gates_commands() {
    let srv = TestServer::start(&["--requirepass", "s3cret"]);
    let mut raw = srv.raw();
    assert_eq!(
        raw.cmd(&[b"GET", b"k"]),
        b"-NOAUTH Authentication required.\r\n"
    );
    assert!(raw.cmd(&[b"AUTH", b"wrong"]).starts_with(b"-WRONGPASS"));
    assert_eq!(raw.cmd(&[b"AUTH", b"s3cret"]), b"+OK\r\n");
    assert_eq!(raw.cmd(&[b"SET", b"k", b"v"]), b"+OK\r\n");

    let mut con = redis::Client::open(format!("redis://:s3cret@{}/", srv.addr))
        .unwrap()
        .get_connection()
        .unwrap();
    assert_eq!(con.get::<_, String>("k").unwrap(), "v");
}

#[test]
fn maxmemory_with_eviction_stays_under_limit() {
    let srv = TestServer::start(&["--maxmemory", "2mb", "--maxmemory-policy", "allkeys-lru"]);
    let mut con = srv.client();
    let value = vec![b'x'; 1000];
    for i in 0..20_000 {
        let _: () = con.set(format!("key{i}"), value.as_slice()).unwrap();
    }
    std::thread::sleep(Duration::from_millis(300));
    let used = info_field(&mut con, "used_memory");
    assert!(used <= 2 * 1024 * 1024, "used_memory {used}");
    assert!(info_field(&mut con, "evicted_keys") > 10_000);
    let n: usize = redis::cmd("DBSIZE").query(&mut con).unwrap();
    assert!(n > 1000, "kept {n} keys");
}

#[test]
fn maxmemory_noeviction_rejects_writes_but_allows_reads() {
    let srv = TestServer::start(&["--maxmemory", "1mb"]);
    let mut con = srv.client();
    let value = vec![b'x'; 1000];
    let mut rejected = None;
    for i in 0..5000 {
        if let Err(e) = con.set::<_, _, ()>(format!("key{i}"), value.as_slice()) {
            rejected = Some(e.to_string());
            break;
        }
    }
    let msg = rejected.expect("writes must be rejected over maxmemory");
    assert!(
        msg.contains("OOM") && msg.contains("command not allowed when used memory"),
        "{msg}"
    );
    assert_eq!(con.get::<_, Vec<u8>>("key0").unwrap(), value);
    let _: () = con.del("key0").unwrap();
}

#[test]
fn config_set_and_get() {
    let srv = TestServer::start(&[]);
    let mut con = srv.client();
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory")
        .arg("10mb")
        .arg("maxmemory-policy")
        .arg("allkeys-lfu")
        .query(&mut con)
        .unwrap();
    let v: Vec<String> = redis::cmd("CONFIG")
        .arg("GET")
        .arg("maxmemory*")
        .query(&mut con)
        .unwrap();
    assert_eq!(
        v[..4],
        ["maxmemory", "10485760", "maxmemory-policy", "allkeys-lfu"]
    );
    let e: RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg("maxmemory-policy")
        .arg("bogus")
        .query(&mut con);
    assert!(e.is_err());
}

#[test]
fn protocol_error_replies_and_closes() {
    let srv = TestServer::start(&[]);
    let mut raw = srv.raw();
    raw.send(b"*1\r\n:oops\r\n");
    let r = raw.read_reply().unwrap();
    assert!(
        r.starts_with(b"-ERR Protocol error: expected '$', got ':'"),
        "{}",
        String::from_utf8_lossy(&r)
    );
    assert!(raw.is_closed());
    // The server keeps serving others.
    assert_eq!(srv.raw().cmd(&[b"PING"]), b"+PONG\r\n");
}

#[test]
fn newline_flood_is_rejected_fast_without_spinning() {
    let srv = TestServer::start(&[]);
    let mut raw = srv.raw();
    let started = Instant::now();
    // 64 KB + 1 of inline data without a newline exceeds the inline limit.
    let chunk = vec![b'A'; 16 * 1024];
    for _ in 0..5 {
        raw.send(&chunk);
    }
    let r = raw.read_reply().unwrap();
    assert_eq!(r, b"-ERR Protocol error: too big inline request\r\n");
    assert!(raw.is_closed());
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(srv.raw().cmd(&[b"PING"]), b"+PONG\r\n");
}

#[test]
fn query_buffer_limit_closes_hoarding_clients() {
    let srv = TestServer::start(&["--client-query-buffer-limit", "1mb"]);
    let mut raw = srv.raw();
    // Announce a 4 MB bulk and send part of it: over the 1 MB limit the client is dropped.
    raw.send(b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$4194304\r\n");
    let chunk = vec![b'x'; 256 * 1024];
    let mut closed = false;
    for _ in 0..16 {
        if !raw.try_send(&chunk) {
            closed = true;
            break;
        }
    }
    closed = closed || raw.is_closed();
    assert!(
        closed,
        "client over the query buffer limit must be disconnected"
    );
    assert_eq!(srv.raw().cmd(&[b"PING"]), b"+PONG\r\n");
}

#[test]
fn inline_commands_work() {
    let srv = TestServer::start(&[]);
    let mut raw = srv.raw();
    raw.send(b"SET \"a key\" 'it''s'\r\n");
    assert_eq!(
        raw.read_reply().unwrap(),
        b"-ERR Protocol error: unbalanced quotes in request\r\n"
    );
    let mut raw = srv.raw();
    raw.send(b"SET k \"line\\nbreak\"\r\nGET k\r\nPING\r\n");
    assert_eq!(raw.read_reply().unwrap(), b"+OK\r\n");
    assert_eq!(raw.read_reply().unwrap(), b"$10\r\nline\nbreak\r\n");
    assert_eq!(raw.read_reply().unwrap(), b"+PONG\r\n");
}

#[test]
fn unknown_command_and_arity_messages_match_redis() {
    let srv = TestServer::start(&[]);
    let mut raw = srv.raw();
    assert_eq!(
        raw.cmd(&[b"FOO", b"a", b"b"]),
        b"-ERR unknown command 'FOO', with args beginning with: 'a' 'b' \r\n"
    );
    assert_eq!(
        raw.cmd(&[b"GET"]),
        b"-ERR wrong number of arguments for 'get' command\r\n"
    );
    let items = array_items(&raw.cmd(&[b"HELLO"]));
    assert_eq!(items.len(), 14);
    assert_eq!(
        raw.cmd(&[b"HELLO", b"4"]),
        b"-NOPROTO unsupported protocol version\r\n"
    );
}

#[test]
fn resp3_after_hello_3() {
    let srv = TestServer::start(&[]);
    let mut raw = srv.raw();
    let hello = raw.cmd(&[b"HELLO", b"3"]);
    assert!(
        hello.starts_with(b"%7\r\n"),
        "{}",
        String::from_utf8_lossy(&hello)
    );
    assert!(hello.windows(13).any(|w| w == b"$5\r\nproto\r\n:3"));
    // Null is `_` in RESP3; maps and verbatim strings replace arrays and bulk text.
    assert_eq!(raw.cmd(&[b"GET", b"missing"]), b"_\r\n");
    assert!(
        raw.cmd(&[b"CONFIG", b"GET", b"maxmemory"])
            .starts_with(b"%1\r\n")
    );
    assert!(raw.cmd(&[b"INFO", b"server"]).starts_with(b"="));
    // HELLO 2 and RESET go back to RESP2.
    raw.cmd(&[b"HELLO", b"2"]);
    assert_eq!(raw.cmd(&[b"GET", b"missing"]), b"$-1\r\n");

    // A client library negotiating RESP3 (redis-py 6+ does this by default).
    let url = format!("redis://{}/?protocol=resp3", srv.addr);
    let mut con = redis::Client::open(url).unwrap().get_connection().unwrap();
    let _: () = con.set_ex("k", "v", 60).unwrap();
    assert_eq!(
        con.get::<_, Option<String>>("k").unwrap().as_deref(),
        Some("v")
    );
    assert_eq!(con.get::<_, Option<String>>("nope").unwrap(), None);
    assert_eq!(con.ttl::<_, i64>("k").unwrap(), 60);
}

#[test]
fn many_connections_are_served() {
    let srv = TestServer::start(&[]);
    let mut conns: Vec<Raw> = (0..500).map(|_| srv.raw()).collect();
    for (i, c) in conns.iter_mut().enumerate() {
        let k = format!("c{i}");
        assert_eq!(c.cmd(&[b"SET", k.as_bytes(), b"v"]), b"+OK\r\n");
    }
    let mut con = srv.client();
    assert_eq!(redis::cmd("DBSIZE").query::<usize>(&mut con).unwrap(), 500);
    assert_eq!(info_field(&mut con, "connected_clients"), 501);
    let _: Value = redis::cmd("PING").query(&mut con).unwrap();
}

#[test]
fn crabpack_compresses_idle_values_transparently() {
    use common::{session_json, wait_for_packed};
    let srv = TestServer::start(&["--compression", "--compression-min-idle", "0"]);
    let mut con = srv.client();
    const N: u64 = 3000;
    let mut pipe = redis::pipe();
    for i in 0..N {
        pipe.set(format!("session:{i}"), session_json(i)).ignore();
    }
    let _: () = pipe.query(&mut con).unwrap();
    let used_before = info_field(&mut con, "used_memory");

    let mut raw = srv.raw();
    let packed = wait_for_packed(&mut raw, N * 9 / 10, Duration::from_secs(30));
    assert!(
        packed >= N * 9 / 10,
        "only {packed} of {N} values were compressed"
    );
    assert!(info_field(&mut con, "compression_dicts") >= 1);
    let used_after = info_field(&mut con, "used_memory");
    assert!(
        used_after * 2 < used_before,
        "used_memory {used_before} -> {used_after}"
    );

    // Every value reads back byte for byte, one by one and in bulk.
    for i in 0..N {
        let v: Vec<u8> = con.get(format!("session:{i}")).unwrap();
        assert_eq!(v, session_json(i), "session:{i}");
    }
    let keys: Vec<String> = (0..100).map(|i| format!("session:{i}")).collect();
    let vals: Vec<Vec<u8>> = con.mget(&keys).unwrap();
    assert_eq!(vals, (0..100).map(session_json).collect::<Vec<_>>());

    // Commands that read or rewrite packed values.
    let v = session_json(1);
    assert_eq!(con.strlen::<_, usize>("session:1").unwrap(), v.len());
    let part: Vec<u8> = con.getrange("session:1", 2, 20).unwrap();
    assert_eq!(part, v[2..=20]);
    assert_eq!(
        con.append::<_, _, usize>("session:1", "!").unwrap(),
        v.len() + 1
    );
    let mut appended = v.clone();
    appended.push(b'!');
    assert_eq!(con.get::<_, Vec<u8>>("session:1").unwrap(), appended);
    let _: () = con.rename("session:2", "renamed:2").unwrap();
    assert_eq!(con.get::<_, Vec<u8>>("renamed:2").unwrap(), session_json(2));
    let _: bool = con.expire("session:3", 1000).unwrap();
    assert_eq!(con.get::<_, Vec<u8>>("session:3").unwrap(), session_json(3));
    let old: Vec<u8> = redis::cmd("GETSET")
        .arg("session:4")
        .arg("x")
        .query(&mut con)
        .unwrap();
    assert_eq!(old, session_json(4));
    let old: Vec<u8> = redis::cmd("GETDEL")
        .arg("session:5")
        .query(&mut con)
        .unwrap();
    assert_eq!(old, session_json(5));
    let old: Vec<u8> = redis::cmd("SET")
        .arg("session:6")
        .arg("y")
        .arg("GET")
        .query(&mut con)
        .unwrap();
    assert_eq!(old, session_json(6));
    let e = con.incr::<_, _, i64>("session:7", 1).unwrap_err();
    assert!(e.to_string().contains("not an integer"), "{e}");

    // Turning compression off keeps packed values readable.
    let _: () = redis::cmd("CONFIG")
        .arg("SET")
        .arg("compression")
        .arg("no")
        .query(&mut con)
        .unwrap();
    assert_eq!(con.get::<_, Vec<u8>>("session:9").unwrap(), session_json(9));
}
