//! RESP2/RESP3 reply encoding into a connection's output buffer. Most replies are identical in both
//! protocols; the ones that differ take a `resp3` flag.

pub fn ok(out: &mut Vec<u8>) {
    out.extend_from_slice(b"+OK\r\n");
}

pub fn simple(out: &mut Vec<u8>, s: &[u8]) {
    out.push(b'+');
    out.extend_from_slice(s);
    out.extend_from_slice(b"\r\n");
}

/// Error reply. `msg` must start with the error code, e.g. `ERR syntax error`.
pub fn error(out: &mut Vec<u8>, msg: &str) {
    out.push(b'-');
    out.extend_from_slice(msg.as_bytes());
    out.extend_from_slice(b"\r\n");
}

pub fn int(out: &mut Vec<u8>, n: i64) {
    out.push(b':');
    out.extend_from_slice(itoa::Buffer::new().format(n).as_bytes());
    out.extend_from_slice(b"\r\n");
}

pub fn bulk(out: &mut Vec<u8>, data: &[u8]) {
    out.push(b'$');
    out.extend_from_slice(itoa::Buffer::new().format(data.len()).as_bytes());
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(data);
    out.extend_from_slice(b"\r\n");
}

pub fn bulk_int(out: &mut Vec<u8>, n: i64) {
    bulk(out, itoa::Buffer::new().format(n).as_bytes());
}

/// Null bulk reply: `$-1` in RESP2, `_` in RESP3.
pub fn null(out: &mut Vec<u8>, resp3: bool) {
    out.extend_from_slice(if resp3 { b"_\r\n" } else { b"$-1\r\n" });
}

/// Map header for `pairs` key/value pairs: `%n` in RESP3, a flat array of `2n` items in RESP2.
pub fn map(out: &mut Vec<u8>, pairs: usize, resp3: bool) {
    if resp3 {
        out.push(b'%');
        out.extend_from_slice(itoa::Buffer::new().format(pairs).as_bytes());
        out.extend_from_slice(b"\r\n");
    } else {
        array(out, pairs * 2);
    }
}

/// Plain-text reply (INFO, CLIENT INFO): a verbatim string `=len\r\ntxt:...` in RESP3, a bulk string
/// in RESP2.
pub fn verbatim_text(out: &mut Vec<u8>, text: &[u8], resp3: bool) {
    if resp3 {
        out.push(b'=');
        out.extend_from_slice(itoa::Buffer::new().format(text.len() + 4).as_bytes());
        out.extend_from_slice(b"\r\ntxt:");
        out.extend_from_slice(text);
        out.extend_from_slice(b"\r\n");
    } else {
        bulk(out, text);
    }
}

pub fn array(out: &mut Vec<u8>, len: usize) {
    out.push(b'*');
    out.extend_from_slice(itoa::Buffer::new().format(len).as_bytes());
    out.extend_from_slice(b"\r\n");
}

pub fn wrong_arity(out: &mut Vec<u8>, cmd: &str) {
    error(
        out,
        &format!("ERR wrong number of arguments for '{cmd}' command"),
    );
}

pub fn syntax_error(out: &mut Vec<u8>) {
    error(out, "ERR syntax error");
}

pub fn not_integer(out: &mut Vec<u8>) {
    error(out, "ERR value is not an integer or out of range");
}

pub fn invalid_expire(out: &mut Vec<u8>, cmd: &str) {
    error(out, &format!("ERR invalid expire time in '{cmd}' command"));
}
