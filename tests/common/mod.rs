//! Test helpers: an in-process server on an ephemeral port, and a raw RESP client that returns reply
//! bytes exactly as received.

#![allow(dead_code)]

use clap::Parser;
use crabcache::config::Config;
use crabcache::server::Server;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct TestServer {
    pub addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    /// Starts a server with default test config plus extra CLI-style flags, e.g. `&["--maxmemory", "1mb"]`.
    pub fn start(flags: &[&str]) -> Self {
        let mut argv = vec![
            "crabcache",
            "--port",
            "0",
            "--threads",
            "2",
            "--shards",
            "16",
        ];
        argv.extend_from_slice(flags);
        let config = Config::parse_from(argv);
        let (addr_tx, addr_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let server = Server::bind(config).await.expect("bind");
                addr_tx.send(server.local_addr().unwrap()).unwrap();
                server
                    .run(async {
                        let _ = stop_rx.await;
                    })
                    .await;
            });
        });
        let addr = addr_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("server start");
        Self {
            addr,
            shutdown: Some(stop_tx),
            thread: Some(thread),
        }
    }

    pub fn url(&self) -> String {
        format!("redis://{}/", self.addr)
    }

    pub fn client(&self) -> redis::Connection {
        redis::Client::open(self.url())
            .unwrap()
            .get_connection()
            .unwrap()
    }

    pub fn raw(&self) -> Raw {
        Raw::connect(self.addr)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Minimal RESP client that keeps reply bytes verbatim (for byte-exact comparisons).
pub struct Raw {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl Raw {
    pub fn connect(addr: SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Self {
            stream,
            buf: Vec::new(),
        }
    }

    pub fn encode(args: &[&[u8]]) -> Vec<u8> {
        let mut out = format!("*{}\r\n", args.len()).into_bytes();
        for a in args {
            out.extend_from_slice(format!("${}\r\n", a.len()).as_bytes());
            out.extend_from_slice(a);
            out.extend_from_slice(b"\r\n");
        }
        out
    }

    pub fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).unwrap();
    }

    /// Like `send`, but reports a closed connection instead of panicking.
    pub fn try_send(&mut self, bytes: &[u8]) -> bool {
        self.stream.write_all(bytes).is_ok()
    }

    pub fn cmd(&mut self, args: &[&[u8]]) -> Vec<u8> {
        self.send(&Self::encode(args));
        self.read_reply().expect("reply")
    }

    /// Reads one complete reply. Returns None if the server closed the connection.
    pub fn read_reply(&mut self) -> Option<Vec<u8>> {
        loop {
            if let Some(n) = reply_len(&self.buf, 0) {
                return Some(self.buf.drain(..n).collect());
            }
            let mut chunk = [0u8; 64 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
    }

    /// True if the server closed the connection within the read timeout. Linux answers with a reset
    /// instead of EOF when the server closes while unread request bytes are still queued.
    pub fn is_closed(&mut self) -> bool {
        use std::io::ErrorKind::{ConnectionAborted, ConnectionReset};
        let mut b = [0u8; 1];
        match self.stream.read(&mut b) {
            Ok(0) => true,
            Err(e) => matches!(e.kind(), ConnectionReset | ConnectionAborted),
            Ok(_) => false,
        }
    }
}

/// Length of the complete RESP2/RESP3 value starting at `pos`, or None if incomplete.
pub fn reply_len(buf: &[u8], pos: usize) -> Option<usize> {
    let line_end = pos + buf.get(pos..)?.windows(2).position(|w| w == b"\r\n")?;
    let header = std::str::from_utf8(&buf[pos + 1..line_end]).ok()?;
    let after = line_end + 2 - pos;
    let elements = |n: i64| -> Option<usize> {
        let mut total = after;
        for _ in 0..n.max(0) {
            total += reply_len(buf, pos + total)?;
        }
        Some(total)
    };
    match buf[pos] {
        // simple string, error, integer; RESP3 null, boolean, double, big number
        b'+' | b'-' | b':' | b'_' | b'#' | b',' | b'(' => Some(after),
        // bulk string; RESP3 verbatim string and blob error
        b'$' | b'=' | b'!' => {
            let n: i64 = header.parse().ok()?;
            if n < 0 {
                return Some(after);
            }
            let total = after + n as usize + 2;
            (buf.len() - pos >= total).then_some(total)
        }
        // array; RESP3 set and push
        b'*' | b'~' | b'>' => elements(header.parse().ok()?),
        // RESP3 map
        b'%' => elements(2 * header.parse::<i64>().ok()?),
        _ => panic!("bad reply byte {:?}", buf[pos] as char),
    }
}

/// Splits a raw array reply into its raw elements.
pub fn array_items(reply: &[u8]) -> Vec<Vec<u8>> {
    assert_eq!(
        reply[0],
        b'*',
        "not an array: {}",
        String::from_utf8_lossy(reply)
    );
    let first = reply.windows(2).position(|w| w == b"\r\n").unwrap() + 2;
    let n: usize = std::str::from_utf8(&reply[1..first - 2])
        .unwrap()
        .parse()
        .unwrap();
    let mut items = Vec::with_capacity(n);
    let mut pos = first;
    for _ in 0..n {
        let len = reply_len(reply, pos).unwrap();
        items.push(reply[pos..pos + len].to_vec());
        pos += len;
    }
    items
}

/// Deterministic PRNG for reproducible random tests.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    pub fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }

    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }
}

/// Deterministic JSON values with shared structure, like sessions in a real cache: compressible with a
/// trained dictionary, not by plain compression.
pub fn session_json(i: u64) -> Vec<u8> {
    let mut r = Rng(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let names = [
        "Ana", "Bruno", "Carla", "Diego", "Elisa", "Felipe", "Gabriela", "Hugo",
    ];
    let themes = ["dark", "light", "system"];
    let roles = ["user", "admin", "editor", "support"];
    format!(
        r#"{{"user_id":{},"name":"{} {}","email":"user{}@example.com","roles":["{}"],"locale":"pt-BR","theme":"{}","last_seen":"2026-{:02}-{:02}T{:02}:{:02}:00Z","cart":[{{"sku":"SKU-{}","qty":{}}}],"csrf":"{:016x}"}}"#,
        r.below(10_000_000),
        r.pick(&names),
        r.pick(&names),
        r.below(100_000),
        r.pick(&roles),
        r.pick(&themes),
        1 + r.below(12),
        1 + r.below(28),
        r.below(24),
        r.below(60),
        1000 + r.below(90_000),
        1 + r.below(4),
        r.next()
    )
    .into_bytes()
}

/// Polls `INFO compression` until at least `min` keys are compressed or the timeout expires.
pub fn wait_for_packed(raw: &mut Raw, min: u64, timeout: std::time::Duration) -> u64 {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let info = raw.cmd(&[b"INFO", b"compression"]);
        let text = String::from_utf8_lossy(&info);
        let packed: u64 = text
            .lines()
            .find_map(|l| l.strip_prefix("compressed_keys:"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        if packed >= min || std::time::Instant::now() > deadline {
            return packed;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
