//! Command dispatch. Each command writes its RESP reply into the connection's output buffer.
//!
//! Reply texts and error messages follow Redis 7/8 so existing clients and tools behave the same; the
//! differential test suite checks this against a real Redis.

mod keys;
mod server;
mod strings;

use crate::config::Config;
use crate::protocol::{Args, reply};
use crate::store::{Clock, Db};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// State shared by all connections.
pub struct Shared {
    pub db: Db,
    pub config: Config,
    pub stats: Stats,
}

impl Shared {
    pub fn new(config: Config) -> Self {
        let db = Db::new(
            config.shard_count(),
            config.maxmemory,
            config.maxmemory_policy,
            config.maxmemory_samples,
        );
        Self {
            db,
            config,
            stats: Stats::default(),
        }
    }
}

#[derive(Default)]
pub struct Stats {
    pub connections_received: AtomicU64,
    pub connected_clients: AtomicU64,
    pub rejected_connections: AtomicU64,
    pub commands_processed: AtomicU64,
    pub next_client_id: AtomicU64,
}

/// Per-connection state.
pub struct Session {
    pub id: u64,
    pub addr: String,
    pub authenticated: bool,
    pub name: Option<Vec<u8>>,
    /// Protocol selected with HELLO 3; RESP2 otherwise.
    pub resp3: bool,
    /// Set by QUIT; the connection closes after flushing replies.
    pub close: bool,
    rng: u64,
}

impl Session {
    pub fn new(shared: &Shared, addr: String) -> Self {
        let id = shared.stats.next_client_id.fetch_add(1, Relaxed) + 1;
        Self {
            id,
            addr,
            authenticated: shared.config.requirepass.is_none(),
            name: None,
            resp3: false,
            close: false,
            rng: 0x2545_F491_4F6C_DD1D ^ id,
        }
    }

    fn next_rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }
}

/// Everything a command handler needs.
pub struct Ctx<'a> {
    pub shared: &'a Shared,
    pub db: &'a Db,
    pub session: &'a mut Session,
    pub clock: Clock,
    pub out: &'a mut Vec<u8>,
}

/// Arity as in the Redis command table: `n` exact, `-n` at least n (both counting the command name).
fn arity_ok(argc: usize, arity: i32) -> bool {
    if arity >= 0 {
        argc == arity as usize
    } else {
        argc >= (-arity) as usize
    }
}

pub fn execute(
    shared: &Shared,
    session: &mut Session,
    args: &Args,
    clock: Clock,
    out: &mut Vec<u8>,
) {
    let name = args.get(0);
    let mut upper = [0u8; 24];
    let cmd: &[u8] = if name.len() <= upper.len() {
        for (d, s) in upper.iter_mut().zip(name) {
            *d = s.to_ascii_uppercase();
        }
        &upper[..name.len()]
    } else {
        b""
    };

    let mut ctx = Ctx {
        shared,
        db: &shared.db,
        session,
        clock,
        out,
    };
    let c = &mut ctx;

    // (handler, arity, lowercase name for errors)
    let (handler, arity, lname): (fn(&mut Ctx, &Args), i32, &str) = match cmd {
        b"GET" => (strings::get, 2, "get"),
        b"SET" => (strings::set, -3, "set"),
        b"SETNX" => (strings::setnx, 3, "setnx"),
        b"SETEX" => (strings::setex, 4, "setex"),
        b"PSETEX" => (strings::psetex, 4, "psetex"),
        b"GETSET" => (strings::getset, 3, "getset"),
        b"GETDEL" => (strings::getdel, 2, "getdel"),
        b"GETEX" => (strings::getex, -2, "getex"),
        b"MGET" => (strings::mget, -2, "mget"),
        b"MSET" => (strings::mset, -3, "mset"),
        b"MSETNX" => (strings::msetnx, -3, "msetnx"),
        b"INCR" => (strings::incr, 2, "incr"),
        b"DECR" => (strings::decr, 2, "decr"),
        b"INCRBY" => (strings::incrby, 3, "incrby"),
        b"DECRBY" => (strings::decrby, 3, "decrby"),
        b"APPEND" => (strings::append, 3, "append"),
        b"STRLEN" => (strings::strlen, 2, "strlen"),
        b"GETRANGE" => (strings::getrange, 4, "getrange"),
        b"SUBSTR" => (strings::getrange, 4, "substr"),
        b"DEL" => (keys::del, -2, "del"),
        b"UNLINK" => (keys::del, -2, "unlink"),
        b"EXISTS" => (keys::exists, -2, "exists"),
        b"TOUCH" => (keys::touch, -2, "touch"),
        b"EXPIRE" => (keys::expire, -3, "expire"),
        b"PEXPIRE" => (keys::pexpire, -3, "pexpire"),
        b"EXPIREAT" => (keys::expireat, -3, "expireat"),
        b"PEXPIREAT" => (keys::pexpireat, -3, "pexpireat"),
        b"TTL" => (keys::ttl, 2, "ttl"),
        b"PTTL" => (keys::pttl, 2, "pttl"),
        b"EXPIRETIME" => (keys::expiretime, 2, "expiretime"),
        b"PEXPIRETIME" => (keys::pexpiretime, 2, "pexpiretime"),
        b"PERSIST" => (keys::persist, 2, "persist"),
        b"TYPE" => (keys::type_, 2, "type"),
        b"KEYS" => (keys::keys, 2, "keys"),
        b"SCAN" => (keys::scan, -2, "scan"),
        b"RANDOMKEY" => (keys::randomkey, 1, "randomkey"),
        b"RENAME" => (keys::rename, 3, "rename"),
        b"RENAMENX" => (keys::renamenx, 3, "renamenx"),
        b"DBSIZE" => (keys::dbsize, 1, "dbsize"),
        b"FLUSHDB" => (keys::flush, -1, "flushdb"),
        b"FLUSHALL" => (keys::flush, -1, "flushall"),
        b"PING" => (server::ping, -1, "ping"),
        b"ECHO" => (server::echo, 2, "echo"),
        b"SELECT" => (server::select, 2, "select"),
        b"QUIT" => (server::quit, -1, "quit"),
        b"RESET" => (server::reset, 1, "reset"),
        b"AUTH" => (server::auth, -2, "auth"),
        b"HELLO" => (server::hello, -1, "hello"),
        b"CLIENT" => (server::client, -2, "client"),
        b"COMMAND" => (server::command, -1, "command"),
        b"CONFIG" => (server::config, -2, "config"),
        b"INFO" => (server::info, -1, "info"),
        b"TIME" => (server::time, 1, "time"),
        b"MEMORY" => (server::memory, -2, "memory"),
        _ => {
            unknown_command(c.out, args);
            return;
        }
    };
    if !arity_ok(args.len(), arity) {
        reply::wrong_arity(c.out, lname);
        return;
    }
    // Same order as Redis: unknown command and arity errors come before NOAUTH.
    if !c.session.authenticated && !matches!(cmd, b"AUTH" | b"HELLO" | b"QUIT" | b"RESET") {
        reply::error(c.out, "NOAUTH Authentication required.");
        return;
    }
    handler(c, args);
}

/// `ERR unknown command 'x', with args beginning with: 'a' 'b' `, built like Redis does.
fn unknown_command(out: &mut Vec<u8>, args: &Args) {
    fn clip(b: &[u8], max: usize) -> String {
        String::from_utf8_lossy(&b[..b.len().min(max)])
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect()
    }
    let mut rest = String::new();
    for a in args.iter().skip(1) {
        if rest.len() >= 128 {
            break;
        }
        rest.push_str(&format!("'{}' ", clip(a, 128 - rest.len())));
    }
    reply::error(
        out,
        &format!(
            "ERR unknown command '{}', with args beginning with: {}",
            clip(args.get(0), 128),
            rest
        ),
    );
}
