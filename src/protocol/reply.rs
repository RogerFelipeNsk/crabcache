//! RESP2 reply encoding into a connection's output buffer.

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

pub fn null(out: &mut Vec<u8>) {
    out.extend_from_slice(b"$-1\r\n");
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
