//! String commands: GET/SET family, counters, MGET/MSET, APPEND, GETRANGE.

use super::Ctx;
use crate::protocol::{Args, reply};
use crate::store::{Clock, Db, Entry, ShardGuard, entry::ENTRY_OVERHEAD};
use crate::util::{eq_ic, parse_i64};

const MAX_STRING: usize = 512 * 1024 * 1024;

const CORRUPT: &str = "ERR stored value could not be decompressed";

/// Writes the value of `e` as a bulk reply, decompressing it if it is stored packed.
fn reply_value(db: &Db, out: &mut Vec<u8>, e: &Entry) {
    if db.codec.with_value(e, |v| reply::bulk(out, v)).is_err() {
        reply::error(out, CORRUPT);
    }
}

fn oom(out: &mut Vec<u8>) {
    reply::error(
        out,
        "OOM command not allowed when used memory > 'maxmemory'.",
    );
}

/// Stores a prebuilt entry, overwriting any existing value. `keep_ttl` keeps the current expiry.
fn store(g: &mut ShardGuard, h: u64, mut e: Entry, keep_ttl: bool, clock: Clock) {
    match g.lookup(h, e.key(), clock.ms) {
        Some(i) => {
            if keep_ttl {
                e.set_expire_at(g.entry(i).expire_at());
            }
            e.meta = g.entry(i).meta;
            g.replace(i, h, e);
            let p = g.policy();
            g.touch(i, p, clock);
        }
        None => {
            e.meta = g.policy().initial_meta(clock);
            g.insert(h, e);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ExpireUnit {
    Ex,
    Px,
    ExAt,
    PxAt,
}

impl ExpireUnit {
    fn parse(a: &[u8]) -> Option<Self> {
        Some(match a.to_ascii_uppercase().as_slice() {
            b"EX" => Self::Ex,
            b"PX" => Self::Px,
            b"EXAT" => Self::ExAt,
            b"PXAT" => Self::PxAt,
            _ => return None,
        })
    }
}

/// Expiry derived from an EX/PX/EXAT/PXAT argument.
#[derive(Clone, Copy)]
enum Deadline {
    At(u64),
    /// Already in the past: the write succeeds and the key is deleted immediately.
    Past,
}

/// Parses an EX/PX/EXAT/PXAT argument with Redis semantics, replying on error. A relative deadline that
/// overflows when added to the current time is rejected as an invalid expire time. Redis detects that
/// case through signed overflow, which is undefined behavior in C: Linux (gcc) builds reject it as
/// intended, while clang builds such as Homebrew's on macOS drop the check and accept the command.
fn deadline(
    unit: ExpireUnit,
    arg: &[u8],
    now: u64,
    cmd: &str,
    out: &mut Vec<u8>,
) -> Option<Deadline> {
    let Some(n) = parse_i64(arg) else {
        reply::not_integer(out);
        return None;
    };
    let secs = matches!(unit, ExpireUnit::Ex | ExpireUnit::ExAt);
    if n <= 0 || (secs && n > i64::MAX / 1000) {
        reply::invalid_expire(out, cmd);
        return None;
    }
    let ms = if secs { n * 1000 } else { n };
    let at = match unit {
        ExpireUnit::Ex | ExpireUnit::Px => match ms.checked_add(now as i64) {
            Some(at) => at,
            None => {
                reply::invalid_expire(out, cmd);
                return None;
            }
        },
        _ => ms,
    };
    Some(if at <= now as i64 {
        Deadline::Past
    } else {
        Deadline::At(at as u64)
    })
}

/// Writes `e`, or deletes the key when the requested deadline is already past.
fn put(
    g: &mut ShardGuard,
    h: u64,
    e: Entry,
    keep_ttl: bool,
    deadline: Option<Deadline>,
    clock: Clock,
) {
    if let Some(Deadline::Past) = deadline {
        if let Some(i) = g.lookup(h, e.key(), clock.ms) {
            g.remove_at(i, h);
        }
    } else {
        store(g, h, e, keep_ttl, clock);
    }
}

fn expire_ms(d: Option<Deadline>) -> u64 {
    match d {
        Some(Deadline::At(at)) => at,
        _ => 0,
    }
}

pub fn get(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    match g.lookup(h, key, c.clock.ms) {
        Some(i) => {
            g.stats.hits += 1;
            let p = g.policy();
            g.touch(i, p, c.clock);
            reply_value(c.db, c.out, g.entry(i));
        }
        None => {
            g.stats.misses += 1;
            reply::null(c.out, c.session.resp3);
        }
    }
}

pub fn set(c: &mut Ctx, args: &Args) {
    let (mut nx, mut xx, mut get, mut keep_ttl) = (false, false, false, false);
    let mut expire: Option<(ExpireUnit, &[u8])> = None;
    let mut j = 3;
    while j < args.len() {
        let a = args.get(j);
        if eq_ic(a, "NX") && !xx {
            nx = true;
        } else if eq_ic(a, "XX") && !nx {
            xx = true;
        } else if eq_ic(a, "GET") {
            get = true;
        } else if eq_ic(a, "KEEPTTL") && expire.is_none() {
            keep_ttl = true;
        } else if let Some(unit) = ExpireUnit::parse(a).filter(|&u| {
            !keep_ttl && expire.is_none_or(|(prev, _)| prev == u) && j + 1 < args.len()
        }) {
            expire = Some((unit, args.get(j + 1)));
            j += 1;
        } else {
            reply::syntax_error(c.out);
            return;
        }
        j += 1;
    }
    let dl = match expire {
        Some((unit, arg)) => match deadline(unit, arg, c.clock.ms, "set", c.out) {
            Some(d) => Some(d),
            None => return,
        },
        None => None,
    };

    let (key, value) = (args.get(1), args.get(2));
    let e = Entry::new(key, value, expire_ms(dl), 0);
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(e.mem_usage(), c.clock).is_err() {
        oom(c.out);
        return;
    }
    if nx || xx || get {
        let existing = g.lookup(h, key, c.clock.ms);
        if get {
            match existing {
                Some(i) => reply_value(c.db, c.out, g.entry(i)),
                None => reply::null(c.out, c.session.resp3),
            }
        }
        if (nx && existing.is_some()) || (xx && existing.is_none()) {
            if !get {
                reply::null(c.out, c.session.resp3);
            }
            return;
        }
    }
    put(&mut g, h, e, keep_ttl, dl, c.clock);
    if !get {
        reply::ok(c.out);
    }
}

pub fn setnx(c: &mut Ctx, args: &Args) {
    let (key, value) = (args.get(1), args.get(2));
    let e = Entry::new(key, value, 0, 0);
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(e.mem_usage(), c.clock).is_err() {
        return oom(c.out);
    }
    if g.lookup(h, key, c.clock.ms).is_some() {
        reply::int(c.out, 0);
    } else {
        store(&mut g, h, e, false, c.clock);
        reply::int(c.out, 1);
    }
}

fn setex_generic(c: &mut Ctx, args: &Args, unit: ExpireUnit, cmd: &str) {
    let Some(dl) = deadline(unit, args.get(2), c.clock.ms, cmd, c.out) else {
        return;
    };
    let key = args.get(1);
    let e = Entry::new(key, args.get(3), expire_ms(Some(dl)), 0);
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(e.mem_usage(), c.clock).is_err() {
        return oom(c.out);
    }
    put(&mut g, h, e, false, Some(dl), c.clock);
    reply::ok(c.out);
}

pub fn setex(c: &mut Ctx, args: &Args) {
    setex_generic(c, args, ExpireUnit::Ex, "setex");
}

pub fn psetex(c: &mut Ctx, args: &Args) {
    setex_generic(c, args, ExpireUnit::Px, "psetex");
}

pub fn getset(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let e = Entry::new(key, args.get(2), 0, 0);
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(e.mem_usage(), c.clock).is_err() {
        return oom(c.out);
    }
    match g.lookup(h, key, c.clock.ms) {
        Some(i) => reply_value(c.db, c.out, g.entry(i)),
        None => reply::null(c.out, c.session.resp3),
    }
    store(&mut g, h, e, false, c.clock);
}

pub fn getdel(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    match g.lookup(h, key, c.clock.ms) {
        Some(i) => {
            g.stats.hits += 1;
            let e = g.remove_at(i, h);
            reply_value(c.db, c.out, &e);
        }
        None => {
            g.stats.misses += 1;
            reply::null(c.out, c.session.resp3);
        }
    }
}

pub fn getex(c: &mut Ctx, args: &Args) {
    let mut expire: Option<(ExpireUnit, &[u8])> = None;
    let mut persist = false;
    let mut j = 2;
    while j < args.len() {
        let a = args.get(j);
        if eq_ic(a, "PERSIST") && expire.is_none() {
            persist = true;
        } else if let Some(unit) = ExpireUnit::parse(a)
            .filter(|&u| !persist && expire.is_none_or(|(prev, _)| prev == u) && j + 1 < args.len())
        {
            expire = Some((unit, args.get(j + 1)));
            j += 1;
        } else {
            reply::syntax_error(c.out);
            return;
        }
        j += 1;
    }
    // Redis order: syntax errors, then a missing key replies nil, then the expiry is validated.
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    let Some(i) = g.lookup(h, key, c.clock.ms) else {
        g.stats.misses += 1;
        return reply::null(c.out, c.session.resp3);
    };
    let at = match expire {
        Some((unit, arg)) => match deadline(unit, arg, c.clock.ms, "getex", c.out) {
            Some(at) => Some(at),
            None => return,
        },
        None => None,
    };
    g.stats.hits += 1;
    reply_value(c.db, c.out, g.entry(i));
    match at {
        Some(Deadline::Past) => {
            g.remove_at(i, h);
        }
        Some(Deadline::At(at)) => g.set_expire(i, h, at),
        None if persist => g.set_expire(i, h, 0),
        None => {}
    }
}

pub fn mget(c: &mut Ctx, args: &Args) {
    reply::array(c.out, args.len() - 1);
    for key in args.iter().skip(1) {
        let (mut g, h) = c.db.lock_key(key);
        match g.lookup(h, key, c.clock.ms) {
            Some(i) => {
                g.stats.hits += 1;
                let p = g.policy();
                g.touch(i, p, c.clock);
                reply_value(c.db, c.out, g.entry(i));
            }
            None => {
                g.stats.misses += 1;
                reply::null(c.out, c.session.resp3);
            }
        }
    }
}

/// Locks every shard touched by the key/value pairs (ascending order) and runs `f` atomically.
fn with_pairs_locked(
    c: &mut Ctx,
    args: &Args,
    f: impl FnOnce(&mut Ctx, &mut [ShardGuard], &[(usize, u64)]),
) {
    let db = c.db;
    // (index into guards, hash) per pair
    let hashed: Vec<(usize, u64)> = (1..args.len())
        .step_by(2)
        .map(|j| {
            let h = db.hash(args.get(j));
            (db.shard_index(h), h)
        })
        .collect();
    let mut shards: Vec<usize> = hashed.iter().map(|&(s, _)| s).collect();
    shards.sort_unstable();
    shards.dedup();
    let mut guards = db.lock_many(&shards);
    let located: Vec<(usize, u64)> = hashed
        .iter()
        .map(|&(s, h)| (shards.binary_search(&s).unwrap(), h))
        .collect();
    f(c, &mut guards, &located);
}

pub fn mset(c: &mut Ctx, args: &Args) {
    if args.len() % 2 == 0 {
        return reply::wrong_arity(c.out, "mset");
    }
    let incoming: usize =
        args.iter().skip(1).map(<[u8]>::len).sum::<usize>() + ENTRY_OVERHEAD * (args.len() / 2);
    with_pairs_locked(c, args, |c, guards, located| {
        if guards[0].admit(incoming, c.clock).is_err() {
            return oom(c.out);
        }
        for (n, &(gi, h)) in located.iter().enumerate() {
            let (k, v) = (args.get(1 + 2 * n), args.get(2 + 2 * n));
            store(&mut guards[gi], h, Entry::new(k, v, 0, 0), false, c.clock);
        }
        reply::ok(c.out);
    });
}

pub fn msetnx(c: &mut Ctx, args: &Args) {
    if args.len() % 2 == 0 {
        return reply::wrong_arity(c.out, "msetnx");
    }
    let incoming: usize =
        args.iter().skip(1).map(<[u8]>::len).sum::<usize>() + ENTRY_OVERHEAD * (args.len() / 2);
    with_pairs_locked(c, args, |c, guards, located| {
        if guards[0].admit(incoming, c.clock).is_err() {
            return oom(c.out);
        }
        let any_exists = located.iter().enumerate().any(|(n, &(gi, h))| {
            guards[gi]
                .lookup(h, args.get(1 + 2 * n), c.clock.ms)
                .is_some()
        });
        if any_exists {
            return reply::int(c.out, 0);
        }
        for (n, &(gi, h)) in located.iter().enumerate() {
            let (k, v) = (args.get(1 + 2 * n), args.get(2 + 2 * n));
            store(&mut guards[gi], h, Entry::new(k, v, 0, 0), false, c.clock);
        }
        reply::int(c.out, 1);
    });
}

fn incr_by(c: &mut Ctx, key: &[u8], delta: i64) {
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(key.len() + 20 + ENTRY_OVERHEAD, c.clock).is_err() {
        return oom(c.out);
    }
    let mut buf = itoa::Buffer::new();
    match g.lookup(h, key, c.clock.ms) {
        Some(i) => {
            let cur = match c.db.codec.with_value(g.entry(i), parse_i64) {
                Ok(Some(cur)) => cur,
                Ok(None) => return reply::not_integer(c.out),
                Err(_) => return reply::error(c.out, CORRUPT),
            };
            let Some(new) = cur.checked_add(delta) else {
                return reply::error(c.out, "ERR increment or decrement would overflow");
            };
            g.set_value(i, buf.format(new).as_bytes());
            reply::int(c.out, new);
        }
        None => {
            let meta = g.policy().initial_meta(c.clock);
            g.insert(h, Entry::new(key, buf.format(delta).as_bytes(), 0, meta));
            reply::int(c.out, delta);
        }
    }
}

pub fn incr(c: &mut Ctx, args: &Args) {
    incr_by(c, args.get(1), 1);
}

pub fn decr(c: &mut Ctx, args: &Args) {
    incr_by(c, args.get(1), -1);
}

pub fn incrby(c: &mut Ctx, args: &Args) {
    match parse_i64(args.get(2)) {
        Some(d) => incr_by(c, args.get(1), d),
        None => reply::not_integer(c.out),
    }
}

pub fn decrby(c: &mut Ctx, args: &Args) {
    match parse_i64(args.get(2)) {
        Some(i64::MIN) => reply::error(c.out, "ERR decrement would overflow"),
        Some(d) => incr_by(c, args.get(1), -d),
        None => reply::not_integer(c.out),
    }
}

pub fn append(c: &mut Ctx, args: &Args) {
    let (key, add) = (args.get(1), args.get(2));
    let (mut g, h) = c.db.lock_key(key);
    if g.admit(key.len() + add.len() + ENTRY_OVERHEAD, c.clock)
        .is_err()
    {
        return oom(c.out);
    }
    match g.lookup(h, key, c.clock.ms) {
        Some(i) => {
            let old_len = g.entry(i).value_len();
            if old_len + add.len() > MAX_STRING {
                return reply::error(
                    c.out,
                    "ERR string exceeds maximum allowed size (proto-max-bulk-len)",
                );
            }
            let joined = c.db.codec.with_value(g.entry(i), |old| {
                let mut v = Vec::with_capacity(old.len() + add.len());
                v.extend_from_slice(old);
                v.extend_from_slice(add);
                v
            });
            let Ok(v) = joined else {
                return reply::error(c.out, CORRUPT);
            };
            g.set_value(i, &v);
            reply::int(c.out, v.len() as i64);
        }
        None => {
            let meta = g.policy().initial_meta(c.clock);
            g.insert(h, Entry::new(key, add, 0, meta));
            reply::int(c.out, add.len() as i64);
        }
    }
}

pub fn strlen(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    let n = g
        .lookup(h, key, c.clock.ms)
        .map_or(0, |i| g.entry(i).value_len());
    reply::int(c.out, n as i64);
}

pub fn getrange(c: &mut Ctx, args: &Args) {
    let (Some(mut start), Some(mut end)) = (parse_i64(args.get(2)), parse_i64(args.get(3))) else {
        return reply::not_integer(c.out);
    };
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    let Some(i) = g.lookup(h, key, c.clock.ms) else {
        return reply::bulk(c.out, b"");
    };
    let len = g.entry(i).value_len() as i64;
    if start < 0 && end < 0 && start > end {
        return reply::bulk(c.out, b"");
    }
    if start < 0 {
        start += len;
    }
    if end < 0 {
        end += len;
    }
    start = start.max(0);
    end = end.max(0);
    if end >= len {
        end = len - 1;
    }
    if start > end || len == 0 {
        return reply::bulk(c.out, b"");
    }
    let range = start as usize..=end as usize;
    if c.db
        .codec
        .with_value(g.entry(i), |v| reply::bulk(c.out, &v[range]))
        .is_err()
    {
        reply::error(c.out, CORRUPT);
    }
}
