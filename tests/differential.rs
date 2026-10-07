//! Differential testing against a real Redis: random command sequences are sent to both servers and
//! the raw replies must match byte for byte (after normalizing time-dependent and unordered replies).
//!
//! Inputs avoided on purpose: a relative EX/PX expiry whose `now + n` overflows an i64 in SET, SETEX,
//! PSETEX and GETEX. Redis detects that overflow through signed wraparound, which is undefined
//! behavior in C, so the reply depends on how Redis was compiled (Linux/gcc returns the intended
//! "invalid expire time" error, which CrabCache matches; Homebrew/clang on macOS accepts the command).
//!
//! Redis comes from `CRABCACHE_DIFF_REDIS=host:port` (it will be FLUSHALLed) or a `redis-server`
//! binary on PATH started on a free port. Set `CRABCACHE_REQUIRE_REDIS=1` (as CI does) to fail
//! instead of skipping when neither is available.

mod common;

use common::{Raw, Rng, TestServer, array_items};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct RedisProcess(Option<Child>);

impl Drop for RedisProcess {
    fn drop(&mut self) {
        if let Some(c) = &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn redis_target() -> Option<(SocketAddr, RedisProcess)> {
    if let Ok(addr) = std::env::var("CRABCACHE_DIFF_REDIS") {
        return Some((
            addr.parse()
                .expect("CRABCACHE_DIFF_REDIS must be host:port"),
            RedisProcess(None),
        ));
    }
    let port = TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let child = Command::new("redis-server")
        .args([
            "--port",
            &port.to_string(),
            "--bind",
            "127.0.0.1",
            "--save",
            "",
            "--appendonly",
            "no",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let guard = RedisProcess(Some(child));
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while TcpStream::connect(addr).is_err() {
        if Instant::now() > deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Some((addr, guard))
}

fn connect_redis() -> Option<(SocketAddr, RedisProcess)> {
    let target = redis_target();
    if target.is_none() {
        if std::env::var("CRABCACHE_REQUIRE_REDIS").is_ok() {
            panic!("differential tests require Redis (CRABCACHE_REQUIRE_REDIS is set)");
        }
        eprintln!("SKIPPED: no redis-server on PATH and CRABCACHE_DIFF_REDIS unset");
    }
    target
}

const KEYS: &[&str] = &[
    "k0", "k1", "k2", "k3", "k4", "k5", "k6", "k7", "user:1", "user:2", "a b",
];
const VALUES: &[&[u8]] = &[
    b"",
    b"0",
    b"1",
    b"-1",
    b"42",
    b"abc",
    b"a b",
    b"9223372036854775807",
    b"-9223372036854775808",
    b"01",
    b"+1",
    b" 1",
    b"3.14",
    b"\x00\xff\r\n",
    b"NULL",
    b"OK",
];
const INTS: &[&str] = &[
    "0",
    "1",
    "-1",
    "5",
    "100",
    "-100",
    "abc",
    "",
    "1.5",
    "9223372036854775807",
    "-9223372036854775808",
    "9223372036854775808",
    "01",
];
/// Relative expiries: long enough never to elapse during a run, or invalid. Short TTLs would make
/// replies depend on timing. Absolute ones in the past come from `EXPIREAT`-style "1" below.
const TTLS: &[&str] = &[
    "1000",
    "100000",
    "1000000",
    "0",
    "-5",
    "abc",
    "9223372036854775807",
];
const PATTERNS: &[&str] = &[
    "*", "k*", "k[1-3]", "k?", "user:*", "*:*", "[^k]*", "a\\ b", "nomatch",
];

fn far_future_secs() -> String {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 1_000_000)
        .to_string()
}

/// Builds one random command (as owned argument byte strings).
fn random_command(r: &mut Rng) -> Vec<Vec<u8>> {
    let k = |r: &mut Rng| r.pick(KEYS).as_bytes().to_vec();
    let v = |r: &mut Rng| -> Vec<u8> {
        if r.chance(10) {
            vec![b'x'; 300 + r.below(200) as usize]
        } else {
            r.pick(VALUES).to_vec()
        }
    };
    let i = |r: &mut Rng| r.pick(INTS).as_bytes().to_vec();
    let ttl = |r: &mut Rng| -> Vec<u8> {
        match r.below(10) {
            0 => far_future_secs().into_bytes(),
            1 => (far_future_secs() + "000").into_bytes(),
            _ => r.pick(TTLS).as_bytes().to_vec(),
        }
    };
    // Absolute deadlines: far future, long past ("1"), or invalid.
    let at = |r: &mut Rng| -> Vec<u8> {
        match r.below(4) {
            0 => b"1".to_vec(),
            1 => far_future_secs().into_bytes(),
            _ => r.pick(TTLS).as_bytes().to_vec(),
        }
    };
    let ttl_no_overflow = |r: &mut Rng| -> Vec<u8> {
        r.pick(&["1000", "100000", "1000000", "0", "-5", "abc"])
            .as_bytes()
            .to_vec()
    };
    // Millisecond variants: a relative "1000" ms could elapse mid-run, so scale those up.
    let ttl_ms_no_overflow = |r: &mut Rng| -> Vec<u8> {
        let t = ttl_no_overflow(r);
        if t == b"1000" {
            b"100000000".to_vec()
        } else {
            t
        }
    };
    let s = |x: &str| x.as_bytes().to_vec();

    let mut cmd: Vec<Vec<u8>> = match r.below(40) {
        0..=4 => {
            let mut c = vec![s("SET"), k(r), v(r)];
            for _ in 0..r.below(3) {
                match r.below(8) {
                    0 => c.push(s("NX")),
                    1 => c.push(s("XX")),
                    2 => c.push(s("GET")),
                    3 => c.push(s("KEEPTTL")),
                    4 => c.extend([s("EX"), ttl(r)]),
                    5 => c.extend([s("PX"), ttl_ms_no_overflow(r)]),
                    6 => c.extend([s("EXAT"), at(r)]),
                    _ => c.extend([s("PXAT"), at(r)]),
                }
            }
            c
        }
        5..=7 => vec![s("GET"), k(r)],
        8 => vec![s("GETDEL"), k(r)],
        9 => vec![s("GETSET"), k(r), v(r)],
        10 => vec![s("SETNX"), k(r), v(r)],
        11 => match r.below(2) {
            0 => vec![s("SETEX"), k(r), ttl(r), v(r)],
            _ => vec![s("PSETEX"), k(r), ttl_ms_no_overflow(r), v(r)],
        },
        12 => {
            let mut c = vec![s("GETEX"), k(r)];
            match r.below(6) {
                0 => c.push(s("PERSIST")),
                1 => c.extend([s("EX"), ttl_no_overflow(r)]),
                2 => c.extend([s("PX"), ttl_ms_no_overflow(r)]),
                3 => c.extend([s("EXAT"), at(r)]),
                4 => c.extend([s("PERSIST"), s("EX"), ttl_no_overflow(r)]),
                _ => {}
            }
            c
        }
        13 => {
            let mut c = vec![s("MGET")];
            c.extend((0..1 + r.below(4)).map(|_| k(r)));
            c
        }
        14 | 15 => {
            let mut c = vec![s(r.pick(&["MSET", "MSETNX"]))];
            for _ in 0..1 + r.below(3) {
                c.push(k(r));
                c.push(v(r));
            }
            if r.chance(10) {
                c.pop();
            }
            c
        }
        16..=18 => vec![s(r.pick(&["INCR", "DECR"])), k(r)],
        19 | 20 => vec![s(r.pick(&["INCRBY", "DECRBY"])), k(r), i(r)],
        21 => vec![s("APPEND"), k(r), v(r)],
        22 => vec![s("STRLEN"), k(r)],
        23 => vec![s("GETRANGE"), k(r), i(r), i(r)],
        24 | 25 => {
            let mut c = vec![s(r.pick(&["DEL", "UNLINK", "EXISTS", "TOUCH"]))];
            c.extend((0..1 + r.below(3)).map(|_| k(r)));
            c
        }
        26..=28 => {
            let name = r.pick(&["EXPIRE", "PEXPIRE", "EXPIREAT", "PEXPIREAT"]);
            let when = if name.ends_with("AT") && r.chance(50) {
                if name.starts_with('P') {
                    far_future_secs() + "000"
                } else {
                    far_future_secs()
                }
            } else if name.starts_with('P') {
                r.pick(&["100000000", "0", "-5", "abc", "9223372036854775807"])
                    .to_string()
            } else {
                r.pick(TTLS).to_string()
            };
            let mut c = vec![s(name), k(r), s(&when)];
            // GT/LT compare deadlines. With relative expiries, applying the same TTL twice within one
            // millisecond gives equal deadlines, so the reply would depend on timing; use them only
            // with absolute deadlines.
            let flags: &[&str] = if name.ends_with("AT") {
                &["NX", "XX", "GT", "LT", "BAD"]
            } else {
                &["NX", "XX", "BAD"]
            };
            for _ in 0..r.below(3) {
                c.push(s(r.pick(flags)));
            }
            c
        }
        29 | 30 => vec![
            s(r.pick(&[
                "TTL",
                "PTTL",
                "EXPIRETIME",
                "PEXPIRETIME",
                "PERSIST",
                "TYPE",
            ])),
            k(r),
        ],
        31 => vec![s(r.pick(&["RENAME", "RENAMENX"])), k(r), k(r)],
        32 => vec![s("KEYS"), s(r.pick(PATTERNS))],
        33 => vec![s("DBSIZE")],
        34 => match r.below(4) {
            0 => vec![s("PING")],
            1 => vec![s("PING"), v(r)],
            2 => vec![s("ECHO"), v(r)],
            _ => vec![s("SELECT"), s(r.pick(&["0", "abc", "-1"]))],
        },
        35 => vec![s(r.pick(&["NOSUCHCMD", "foo"])), s("arg1"), s("arg two")],
        36 => vec![s(r.pick(&[
            "GET", "SET", "INCR", "MGET", "EXPIRE", "TTL", "RENAME", "GETRANGE",
        ]))],
        37 if r.chance(5) => vec![s("FLUSHALL")],
        _ => vec![s("SET"), k(r), v(r)],
    };
    // Occasionally lowercase the command name: dispatch is case-insensitive.
    if r.chance(10) {
        cmd[0] = cmd[0].to_ascii_lowercase();
    }
    cmd
}

/// Removes legitimately nondeterministic parts of a reply.
fn normalize(cmd: &[Vec<u8>], reply: Vec<u8>) -> Vec<u8> {
    let name = cmd[0].to_ascii_uppercase();
    match name.as_slice() {
        // Remaining TTLs and deadlines depend on timing: keep only their class.
        b"TTL" | b"PTTL" | b"EXPIRETIME" | b"PEXPIRETIME"
            if reply.starts_with(b":") && !reply.starts_with(b":-") =>
        {
            b":<positive>\r\n".to_vec()
        }
        // KEYS order is hash order.
        b"KEYS" if reply.starts_with(b"*") => {
            let mut items = array_items(&reply);
            items.sort();
            let mut out = format!("*{}\r\n", items.len()).into_bytes();
            for it in items {
                out.extend(it);
            }
            out
        }
        _ => reply,
    }
}

fn show(cmd: &[Vec<u8>]) -> String {
    cmd.iter()
        .map(|a| format!("{:?}", String::from_utf8_lossy(a)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One test running both phases in sequence: an external Redis (`CRABCACHE_DIFF_REDIS`) is shared
/// state, and test runners execute separate tests in parallel.
#[test]
fn commands_match_redis() {
    let Some((redis_addr, _redis)) = connect_redis() else {
        return;
    };
    for resp3 in [false, true] {
        sequential_commands_match(redis_addr, resp3);
        pipelined_batches_match(redis_addr, resp3);
    }
}

/// Connects to both servers, switching both to RESP3 when asked. HELLO replies differ by design
/// (server name, version, client id), so they are not compared.
fn connect_pair(redis_addr: SocketAddr, srv: &TestServer, resp3: bool) -> (Raw, Raw) {
    let mut redis = Raw::connect(redis_addr);
    let mut crab = srv.raw();
    if resp3 {
        for conn in [&mut redis, &mut crab] {
            let hello = conn.cmd(&[b"HELLO", b"3"]);
            assert!(
                hello.starts_with(b"%"),
                "HELLO 3 must reply with a map: {:?}",
                String::from_utf8_lossy(&hello)
            );
        }
    }
    redis.cmd(&[b"FLUSHALL"]);
    (redis, crab)
}

fn sequential_commands_match(redis_addr: SocketAddr, resp3: bool) {
    let srv = TestServer::start(&[]);
    let (mut redis, mut crab) = connect_pair(redis_addr, &srv, resp3);

    let seeds = std::env::var("CRABCACHE_DIFF_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20u64);
    let mut history: Vec<String> = Vec::new();
    for seed in 1..=seeds {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for step in 0..1500 {
            let cmd = random_command(&mut r);
            let args: Vec<&[u8]> = cmd.iter().map(Vec::as_slice).collect();
            let a = normalize(&cmd, redis.cmd(&args));
            let b = normalize(&cmd, crab.cmd(&args));
            history.push(show(&cmd));
            if history.len() > 15 {
                history.remove(0);
            }
            assert!(
                a == b,
                "resp3={resp3} seed {seed} step {step}: {}\n  redis:     {:?}\n  crabcache: {:?}\nlast commands:\n  {}",
                show(&cmd),
                String::from_utf8_lossy(&a),
                String::from_utf8_lossy(&b),
                history.join("\n  ")
            );
        }
    }
}

fn pipelined_batches_match(redis_addr: SocketAddr, resp3: bool) {
    let srv = TestServer::start(&[]);
    let (mut redis, mut crab) = connect_pair(redis_addr, &srv, resp3);

    let mut r = Rng(0xDEAD_BEEF);
    for round in 0..50 {
        let cmds: Vec<Vec<Vec<u8>>> = (0..200).map(|_| random_command(&mut r)).collect();
        let mut wire = Vec::new();
        for c in &cmds {
            let args: Vec<&[u8]> = c.iter().map(Vec::as_slice).collect();
            wire.extend(Raw::encode(&args));
        }
        redis.send(&wire);
        crab.send(&wire);
        for (n, c) in cmds.iter().enumerate() {
            let a = normalize(c, redis.read_reply().unwrap());
            let b = normalize(c, crab.read_reply().unwrap());
            assert!(
                a == b,
                "resp3={resp3} round {round} cmd {n}: {}\n  redis:     {:?}\n  crabcache: {:?}",
                show(c),
                String::from_utf8_lossy(&a),
                String::from_utf8_lossy(&b)
            );
        }
    }
}
