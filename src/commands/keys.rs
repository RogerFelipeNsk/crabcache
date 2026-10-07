//! Keyspace commands: DEL/EXISTS, expiry, TYPE, KEYS/SCAN, RENAME, DBSIZE/FLUSH.

use super::Ctx;
use crate::protocol::{Args, reply};
use crate::store::Shard;
use crate::util::{eq_ic, glob_match, parse_i64};

pub fn del(c: &mut Ctx, args: &Args) {
    let mut n = 0;
    for key in args.iter().skip(1) {
        let (mut g, h) = c.db.lock_key(key);
        if let Some(i) = g.lookup(h, key, c.clock.ms) {
            g.remove_at(i, h);
            n += 1;
        }
    }
    reply::int(c.out, n);
}

fn count_existing(c: &mut Ctx, args: &Args, touch: bool) {
    let mut n = 0;
    for key in args.iter().skip(1) {
        let (mut g, h) = c.db.lock_key(key);
        if let Some(i) = g.lookup(h, key, c.clock.ms) {
            if touch {
                let p = g.policy();
                g.touch(i, p, c.clock);
            }
            n += 1;
        }
    }
    reply::int(c.out, n);
}

pub fn exists(c: &mut Ctx, args: &Args) {
    count_existing(c, args, false);
}

pub fn touch(c: &mut Ctx, args: &Args) {
    count_existing(c, args, true);
}

/// EXPIRE/PEXPIRE/EXPIREAT/PEXPIREAT with NX|XX|GT|LT, following Redis `expireGenericCommand`.
fn expire_generic(c: &mut Ctx, args: &Args, millis: bool, absolute: bool, cmd: &str) {
    let (mut nx, mut xx, mut gt, mut lt) = (false, false, false, false);
    for a in args.iter().skip(3) {
        match a.to_ascii_uppercase().as_slice() {
            b"NX" => nx = true,
            b"XX" => xx = true,
            b"GT" => gt = true,
            b"LT" => lt = true,
            _ => {
                return reply::error(
                    c.out,
                    &format!("ERR Unsupported option {}", String::from_utf8_lossy(a)),
                );
            }
        }
    }
    if nx && (xx || gt || lt) {
        return reply::error(
            c.out,
            "ERR NX and XX, GT or LT options at the same time are not compatible",
        );
    }
    if gt && lt {
        return reply::error(
            c.out,
            "ERR GT and LT options at the same time are not compatible",
        );
    }
    let Some(n) = parse_i64(args.get(2)) else {
        return reply::not_integer(c.out);
    };
    let now = c.clock.ms as i64;
    let when = (if millis { Some(n) } else { n.checked_mul(1000) }).and_then(|ms| {
        if absolute {
            Some(ms)
        } else {
            ms.checked_add(now)
        }
    });
    let Some(when) = when else {
        return reply::invalid_expire(c.out, cmd);
    };

    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    let Some(i) = g.lookup(h, key, c.clock.ms) else {
        return reply::int(c.out, 0);
    };
    let current = g.entry(i).expire_at();
    let has_ttl = current != 0;
    let blocked = (nx && has_ttl)
        || (xx && !has_ttl)
        || (gt && (!has_ttl || when <= current as i64))
        || (lt && has_ttl && when >= current as i64);
    if blocked {
        return reply::int(c.out, 0);
    }
    if when <= now {
        g.remove_at(i, h);
    } else {
        g.set_expire(i, h, when as u64);
    }
    reply::int(c.out, 1);
}

pub fn expire(c: &mut Ctx, args: &Args) {
    expire_generic(c, args, false, false, "expire");
}

pub fn pexpire(c: &mut Ctx, args: &Args) {
    expire_generic(c, args, true, false, "pexpire");
}

pub fn expireat(c: &mut Ctx, args: &Args) {
    expire_generic(c, args, false, true, "expireat");
}

pub fn pexpireat(c: &mut Ctx, args: &Args) {
    expire_generic(c, args, true, true, "pexpireat");
}

/// Returns -2 for a missing key, -1 for no expiry, otherwise the absolute deadline in ms.
fn expire_of(c: &mut Ctx, key: &[u8]) -> i64 {
    let (mut g, h) = c.db.lock_key(key);
    match g.lookup(h, key, c.clock.ms) {
        None => -2,
        Some(i) if g.entry(i).expire_at() == 0 => -1,
        Some(i) => g.entry(i).expire_at() as i64,
    }
}

fn ttl_generic(c: &mut Ctx, args: &Args, millis: bool) {
    let at = expire_of(c, args.get(1));
    if at < 0 {
        return reply::int(c.out, at);
    }
    let left = (at - c.clock.ms as i64).max(0);
    reply::int(c.out, if millis { left } else { (left + 500) / 1000 });
}

pub fn ttl(c: &mut Ctx, args: &Args) {
    ttl_generic(c, args, false);
}

pub fn pttl(c: &mut Ctx, args: &Args) {
    ttl_generic(c, args, true);
}

pub fn expiretime(c: &mut Ctx, args: &Args) {
    let at = expire_of(c, args.get(1));
    reply::int(c.out, if at < 0 { at } else { at / 1000 });
}

pub fn pexpiretime(c: &mut Ctx, args: &Args) {
    let at = expire_of(c, args.get(1));
    reply::int(c.out, at);
}

pub fn persist(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    match g.lookup(h, key, c.clock.ms) {
        Some(i) if g.entry(i).expire_at() != 0 => {
            g.set_expire(i, h, 0);
            reply::int(c.out, 1);
        }
        _ => reply::int(c.out, 0),
    }
}

pub fn type_(c: &mut Ctx, args: &Args) {
    let key = args.get(1);
    let (mut g, h) = c.db.lock_key(key);
    let t: &[u8] = if g.lookup(h, key, c.clock.ms).is_some() {
        b"string"
    } else {
        b"none"
    };
    reply::simple(c.out, t);
}

pub fn keys(c: &mut Ctx, args: &Args) {
    let pattern = args.get(1);
    let match_all = pattern == b"*";
    let mut body = Vec::new();
    let mut n = 0;
    for s in 0..c.db.shard_count() {
        let g = c.db.lock(s);
        for e in g.entries().iter() {
            if !e.is_expired(c.clock.ms) && (match_all || glob_match(pattern, e.key(), false)) {
                reply::bulk(&mut body, e.key());
                n += 1;
            }
        }
    }
    reply::array(c.out, n);
    c.out.extend_from_slice(&body);
}

/// SCAN cursor [MATCH pattern] [COUNT count] [TYPE type].
///
/// The cursor is `(shard << 32) | position`. Keys present for the whole iteration are returned unless
/// a deletion moved them to an already visited position (deletes use swap-remove); a key may then be
/// missed, a weaker guarantee than Redis' reverse-binary cursor.
pub fn scan(c: &mut Ctx, args: &Args) {
    let Some(cursor) = std::str::from_utf8(args.get(1))
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return reply::error(c.out, "ERR invalid cursor");
    };
    let mut pattern: Option<&[u8]> = None;
    let mut count: usize = 10;
    let mut type_ok = true;
    let mut j = 2;
    while j < args.len() {
        let a = args.get(j);
        if j + 1 >= args.len() {
            return reply::syntax_error(c.out);
        }
        let v = args.get(j + 1);
        if eq_ic(a, "MATCH") {
            pattern = Some(v).filter(|p| *p != b"*");
        } else if eq_ic(a, "COUNT") {
            match parse_i64(v) {
                Some(n) if n >= 1 => count = n as usize,
                Some(_) => return reply::syntax_error(c.out),
                None => return reply::not_integer(c.out),
            }
        } else if eq_ic(a, "TYPE") {
            type_ok = eq_ic(v, "string");
        } else {
            return reply::syntax_error(c.out);
        }
        j += 2;
    }

    let shards = c.db.shard_count();
    let (mut s, mut pos) = ((cursor >> 32) as usize, (cursor & 0xFFFF_FFFF) as usize);
    let mut body = Vec::new();
    let mut found = 0;
    let mut visited = 0;
    while s < shards && visited < count {
        let g = c.db.lock(s);
        let entries = g.entries();
        while pos < entries.len() && visited < count {
            let e = &entries[pos];
            pos += 1;
            visited += 1;
            if type_ok
                && !e.is_expired(c.clock.ms)
                && pattern.is_none_or(|p| glob_match(p, e.key(), false))
            {
                reply::bulk(&mut body, e.key());
                found += 1;
            }
        }
        if pos >= entries.len() {
            s += 1;
            pos = 0;
        }
    }
    let next = if s >= shards {
        0
    } else {
        ((s as u64) << 32) | pos as u64
    };
    reply::array(c.out, 2);
    reply::bulk_int(c.out, next as i64);
    reply::array(c.out, found);
    c.out.extend_from_slice(&body);
}

pub fn randomkey(c: &mut Ctx, _args: &Args) {
    let shards = c.db.shard_count();
    let start = c.session.next_rand() as usize;
    for k in 0..shards {
        let mut g = c.db.lock((start + k) % shards);
        for _ in 0..8 {
            let Some(i) = g.random_index() else { break };
            if !g.entry(i).is_expired(c.clock.ms) {
                return reply::bulk(c.out, g.entry(i).key());
            }
        }
    }
    reply::null(c.out, c.session.resp3);
}

fn rename_generic(c: &mut Ctx, args: &Args, nx: bool) {
    let (src, dst) = (args.get(1), args.get(2));
    let (hs, hd) = (c.db.hash(src), c.db.hash(dst));
    let (ss, sd) = (c.db.shard_index(hs), c.db.shard_index(hd));
    let now = c.clock.ms;

    let result: Result<bool, &str> = if ss == sd {
        let mut g = c.db.lock(ss);
        rename_same_shard(&mut g, (src, hs), (dst, hd), now, nx)
    } else {
        let mut guards = c.db.lock_many(&[ss.min(sd), ss.max(sd)]);
        let (a, b) = guards.split_at_mut(1);
        let (gs, gd) = if ss < sd {
            (&mut a[0], &mut b[0])
        } else {
            (&mut b[0], &mut a[0])
        };
        rename_across(gs, gd, (src, hs), (dst, hd), now, nx)
    };
    match result {
        Err(e) => reply::error(c.out, e),
        Ok(done) if nx => reply::int(c.out, done as i64),
        Ok(_) => reply::ok(c.out),
    }
}

fn rename_same_shard(
    g: &mut Shard,
    (src, hs): (&[u8], u64),
    (dst, hd): (&[u8], u64),
    now: u64,
    nx: bool,
) -> Result<bool, &'static str> {
    let Some(_) = g.lookup(hs, src, now) else {
        return Err("ERR no such key");
    };
    if src == dst {
        return Ok(!nx);
    }
    if let Some(j) = g.lookup(hd, dst, now) {
        if nx {
            return Ok(false);
        }
        g.remove_at(j, hd);
    }
    // Look the source up again: removing dst may have moved it.
    let i = g.lookup(hs, src, now).expect("source present");
    let e = g.remove_at(i, hs);
    g.insert(hd, e.with_key(dst));
    Ok(true)
}

fn rename_across(
    gs: &mut Shard,
    gd: &mut Shard,
    (src, hs): (&[u8], u64),
    (dst, hd): (&[u8], u64),
    now: u64,
    nx: bool,
) -> Result<bool, &'static str> {
    let Some(i) = gs.lookup(hs, src, now) else {
        return Err("ERR no such key");
    };
    if let Some(j) = gd.lookup(hd, dst, now) {
        if nx {
            return Ok(false);
        }
        gd.remove_at(j, hd);
    }
    let e = gs.remove_at(i, hs);
    gd.insert(hd, e.with_key(dst));
    Ok(true)
}

pub fn rename(c: &mut Ctx, args: &Args) {
    rename_generic(c, args, false);
}

pub fn renamenx(c: &mut Ctx, args: &Args) {
    rename_generic(c, args, true);
}

pub fn dbsize(c: &mut Ctx, _args: &Args) {
    reply::int(c.out, c.db.dbsize() as i64);
}

pub fn flush(c: &mut Ctx, args: &Args) {
    match args.len() {
        1 => {}
        2 if eq_ic(args.get(1), "ASYNC") || eq_ic(args.get(1), "SYNC") => {}
        _ => return reply::syntax_error(c.out),
    }
    c.db.flush();
    reply::ok(c.out);
}
