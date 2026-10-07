//! Incremental RESP2 request parser (multibulk + inline), modelled on Redis' `processMultibulkBuffer`
//! and `processInlineBuffer`.
//!
//! The parser never rescans bytes it has already consumed: a partially received multibulk command keeps
//! its position and collected argument ranges across reads, and an incomplete inline line remembers how far
//! it has been searched for `\n`. This keeps parsing linear in the number of bytes received, so a client
//! trickling a huge request cannot make the server spin.

use memchr::memchr;

/// Hard limits applied while parsing. Defaults mirror Redis.
#[derive(Clone, Debug)]
pub struct Limits {
    /// Largest accepted bulk string (`proto-max-bulk-len`).
    pub max_bulk_len: usize,
    /// Largest accepted number of arguments in one multibulk request.
    pub max_multibulk_len: usize,
    /// Largest inline request line, and largest `*`/`$` header line.
    pub max_inline_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bulk_len: 512 * 1024 * 1024,
            max_multibulk_len: 1024 * 1024,
            max_inline_len: 64 * 1024,
        }
    }
}

/// A protocol violation. The connection must reply `-ERR Protocol error: <msg>` and close.
#[derive(Debug, PartialEq, Eq)]
pub struct ProtocolError(pub String);

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    /// A full command is available. Argument ranges are in [`Parser::args`]; they index into the
    /// parsed buffer for multibulk requests, or into [`Parser::scratch`] when `inline` is true.
    Command { consumed: usize, inline: bool },
    /// Bytes were consumed but they carried no command (blank inline line, `*0`, `*-1`).
    Empty { consumed: usize },
    /// More bytes are needed.
    Incomplete,
}

#[derive(Default)]
pub struct Parser {
    /// Argument ranges of the last parsed command.
    pub args: Vec<(usize, usize)>,
    /// Unescaped inline arguments.
    pub scratch: Vec<u8>,
    /// Number of arguments announced by the current multibulk header; 0 when not inside one.
    expected: usize,
    /// Resume offset inside the current multibulk command.
    pos: usize,
    /// Bytes of an incomplete inline request already searched for `\n`.
    inline_scanned: usize,
    /// Total size the buffer must reach before the pending bulk string is complete.
    needed: usize,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Buffer size required to complete the pending bulk argument, used to reserve capacity up front
    /// instead of growing the buffer read by read.
    pub fn needed(&self) -> usize {
        self.needed
    }

    /// Parses at most one command from the start of `buf`.
    ///
    /// Between calls the caller may only append to `buf` or drop bytes that a previous call reported as
    /// consumed; offsets of a partial command are relative to the start of the unconsumed data.
    pub fn parse(&mut self, buf: &[u8], lim: &Limits) -> Result<Parsed, ProtocolError> {
        if self.expected == 0 {
            if buf.is_empty() {
                return Ok(Parsed::Incomplete);
            }
            if buf[0] != b'*' {
                return self.parse_inline(buf, lim);
            }
            let Some(eol) = find_crlf(buf, 1) else {
                if buf.len() > lim.max_inline_len {
                    return Err(err("too big mbulk count string"));
                }
                return Ok(Parsed::Incomplete);
            };
            let count = parse_header_int(&buf[1..eol])
                .filter(|&n| n <= lim.max_multibulk_len as i64)
                .ok_or_else(|| err("invalid multibulk length"))?;
            if count <= 0 {
                return Ok(Parsed::Empty { consumed: eol + 2 });
            }
            self.expected = count as usize;
            self.pos = eol + 2;
            self.args.clear();
            self.args.reserve(self.expected.min(1024));
        }

        while self.args.len() < self.expected {
            let pos = self.pos;
            if pos >= buf.len() {
                return Ok(Parsed::Incomplete);
            }
            if buf[pos] != b'$' {
                let got = buf[pos] as char;
                self.reset();
                return Err(err(&format!("expected '$', got '{got}'")));
            }
            let Some(eol) = find_crlf(buf, pos + 1) else {
                if buf.len() - pos > lim.max_inline_len {
                    self.reset();
                    return Err(err("too big bulk count string"));
                }
                return Ok(Parsed::Incomplete);
            };
            let Some(len) = parse_header_int(&buf[pos + 1..eol])
                .filter(|&n| n >= 0 && n as usize <= lim.max_bulk_len)
            else {
                self.reset();
                return Err(err("invalid bulk length"));
            };
            let start = eol + 2;
            let end = start + len as usize;
            if end + 2 > buf.len() {
                self.needed = end + 2;
                return Ok(Parsed::Incomplete);
            }
            if &buf[end..end + 2] != b"\r\n" {
                self.reset();
                return Err(err("expected CRLF after bulk string"));
            }
            self.args.push((start, end));
            self.pos = end + 2;
        }

        let consumed = self.pos;
        self.expected = 0;
        self.pos = 0;
        self.needed = 0;
        Ok(Parsed::Command {
            consumed,
            inline: false,
        })
    }

    fn reset(&mut self) {
        self.expected = 0;
        self.pos = 0;
        self.needed = 0;
        self.inline_scanned = 0;
        self.args.clear();
    }

    fn parse_inline(&mut self, buf: &[u8], lim: &Limits) -> Result<Parsed, ProtocolError> {
        let from = self.inline_scanned.min(buf.len());
        let Some(rel) = memchr(b'\n', &buf[from..]) else {
            self.inline_scanned = buf.len();
            if buf.len() > lim.max_inline_len {
                self.reset();
                return Err(err("too big inline request"));
            }
            return Ok(Parsed::Incomplete);
        };
        let nl = from + rel;
        self.inline_scanned = 0;
        if nl > lim.max_inline_len {
            return Err(err("too big inline request"));
        }
        let mut line = &buf[..nl];
        if let Some(stripped) = line.strip_suffix(b"\r") {
            line = stripped;
        }
        split_args(line, &mut self.scratch, &mut self.args)
            .map_err(|_| err("unbalanced quotes in request"))?;
        if self.args.is_empty() {
            Ok(Parsed::Empty { consumed: nl + 1 })
        } else {
            Ok(Parsed::Command {
                consumed: nl + 1,
                inline: true,
            })
        }
    }
}

fn err(msg: &str) -> ProtocolError {
    ProtocolError(msg.to_string())
}

/// Index of the `\r` of the first `\r\n` at or after `from`.
fn find_crlf(buf: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < buf.len() {
        let r = i + memchr(b'\r', &buf[i..])?;
        if r + 1 >= buf.len() {
            return None;
        }
        if buf[r + 1] == b'\n' {
            return Some(r);
        }
        i = r + 1;
    }
    None
}

/// Parses the integer of a `*<n>` or `$<n>` header. Accepts an optional leading `-`.
fn parse_header_int(s: &[u8]) -> Option<i64> {
    let (neg, digits) = match s.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, s),
    };
    if digits.is_empty() || digits.len() > 18 {
        return None;
    }
    let mut v: i64 = 0;
    for &c in digits {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + (c - b'0') as i64;
    }
    Some(if neg { -v } else { v })
}

/// Splits an inline request into arguments with the quoting rules of Redis' `sdssplitargs`.
fn split_args(
    line: &[u8],
    scratch: &mut Vec<u8>,
    args: &mut Vec<(usize, usize)>,
) -> Result<(), ()> {
    scratch.clear();
    args.clear();
    let n = line.len();
    let mut i = 0;
    loop {
        while i < n && is_space(line[i]) {
            i += 1;
        }
        if i == n {
            return Ok(());
        }
        let start = scratch.len();
        let (mut in_dq, mut in_sq) = (false, false);
        loop {
            if in_dq {
                if i == n {
                    return Err(());
                }
                let c = line[i];
                if c == b'\\'
                    && i + 3 < n
                    && line[i + 1] == b'x'
                    && hex(line[i + 2]).is_some()
                    && hex(line[i + 3]).is_some()
                {
                    scratch.push(hex(line[i + 2]).unwrap() * 16 + hex(line[i + 3]).unwrap());
                    i += 4;
                } else if c == b'\\' && i + 1 < n {
                    scratch.push(match line[i + 1] {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 0x08,
                        b'a' => 0x07,
                        other => other,
                    });
                    i += 2;
                } else if c == b'"' {
                    // A closing quote must be followed by a space or the end of the line.
                    if i + 1 < n && !is_space(line[i + 1]) {
                        return Err(());
                    }
                    i += 1;
                    break;
                } else {
                    scratch.push(c);
                    i += 1;
                }
            } else if in_sq {
                if i == n {
                    return Err(());
                }
                let c = line[i];
                if c == b'\\' && i + 1 < n && line[i + 1] == b'\'' {
                    scratch.push(b'\'');
                    i += 2;
                } else if c == b'\'' {
                    if i + 1 < n && !is_space(line[i + 1]) {
                        return Err(());
                    }
                    i += 1;
                    break;
                } else {
                    scratch.push(c);
                    i += 1;
                }
            } else {
                if i == n {
                    break;
                }
                match line[i] {
                    b' ' | b'\n' | b'\r' | b'\t' | 0 => break,
                    b'"' => in_dq = true,
                    b'\'' => in_sq = true,
                    c => scratch.push(c),
                }
                i += 1;
            }
        }
        args.push((start, scratch.len()));
    }
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args_of(p: &Parser, buf: &[u8], inline: bool) -> Vec<Vec<u8>> {
        let base = if inline { &p.scratch[..] } else { buf };
        p.args.iter().map(|&(s, e)| base[s..e].to_vec()).collect()
    }

    /// Parses every command in `input`, feeding it in chunks of `chunk` bytes.
    fn parse_all(input: &[u8], chunk: usize) -> Result<Vec<Vec<Vec<u8>>>, ProtocolError> {
        let lim = Limits::default();
        let mut p = Parser::new();
        let mut buf: Vec<u8> = Vec::new();
        let mut out = Vec::new();
        for piece in input.chunks(chunk.max(1)) {
            buf.extend_from_slice(piece);
            loop {
                match p.parse(&buf, &lim)? {
                    Parsed::Command { consumed, inline } => {
                        out.push(args_of(&p, &buf, inline));
                        buf.drain(..consumed);
                    }
                    Parsed::Empty { consumed } => {
                        buf.drain(..consumed);
                    }
                    Parsed::Incomplete => break,
                }
            }
        }
        assert!(buf.is_empty(), "leftover bytes: {buf:?}");
        Ok(out)
    }

    fn v(parts: &[&str]) -> Vec<Vec<u8>> {
        parts.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    #[test]
    fn multibulk_any_split_gives_same_commands() {
        let input = b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$11\r\nhello world\r\n*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n*1\r\n$0\r\n\r\n";
        let expected = vec![
            v(&["SET", "key", "hello world"]),
            v(&["GET", "key"]),
            v(&[""]),
        ];
        for chunk in 1..=input.len() {
            assert_eq!(
                parse_all(input, chunk).unwrap(),
                expected,
                "chunk size {chunk}"
            );
        }
    }

    #[test]
    fn binary_safe_bulk() {
        let input = b"*2\r\n$3\r\nGET\r\n$4\r\n\x00\r\n\xff\r\n";
        assert_eq!(
            parse_all(input, 3).unwrap(),
            vec![vec![b"GET".to_vec(), b"\x00\r\n\xff".to_vec()]]
        );
    }

    #[test]
    fn inline_with_quotes_and_escapes() {
        let input = b"SET k \"a b\\x41\\n\" \r\nGET 'it\\'s'\nPING\r\n\r\n";
        assert_eq!(
            parse_all(input, 2).unwrap(),
            vec![
                v(&["SET", "k", "a bA\n"]),
                v(&["GET", "it's"]),
                v(&["PING"])
            ]
        );
    }

    #[test]
    fn empty_and_negative_multibulk_are_skipped() {
        assert_eq!(
            parse_all(b"*0\r\n*-1\r\nPING\r\n", 1).unwrap(),
            vec![v(&["PING"])]
        );
    }

    #[test]
    fn protocol_errors() {
        assert!(parse_all(b"*2\r\n$3\r\nGET\r\n:1\r\n", 64).is_err());
        assert!(parse_all(b"*1\r\n$-5\r\n", 64).is_err());
        assert!(parse_all(b"*abc\r\n", 64).is_err());
        assert!(parse_all(b"*99999999999\r\n", 64).is_err());
        assert!(parse_all(b"*1\r\n$3\r\nGETXX", 64).is_err());
        assert!(parse_all(b"SET k \"unterminated\r\n", 64).is_err());
        assert!(parse_all(b"SET k \"a\"b\r\n", 64).is_err());
    }

    #[test]
    fn oversized_requests_are_rejected_without_newline() {
        let lim = Limits::default();
        let mut p = Parser::new();
        let big = vec![b'A'; lim.max_inline_len + 1];
        assert!(p.parse(&big, &lim).is_err());
        let mut p = Parser::new();
        let mut hdr = b"*1\r\n$".to_vec();
        hdr.extend(std::iter::repeat_n(b'9', lim.max_inline_len + 1));
        assert!(p.parse(&hdr, &lim).is_err());
    }

    #[test]
    #[allow(clippy::same_item_push)] // the buffer must grow one byte per parse call
    fn inline_scan_is_incremental() {
        // Feeding a long line byte by byte must not rescan from the start each time.
        let lim = Limits::default();
        let mut p = Parser::new();
        let mut buf = Vec::new();
        for _ in 0..10_000 {
            buf.push(b'x');
            assert_eq!(p.parse(&buf, &lim).unwrap(), Parsed::Incomplete);
            assert_eq!(p.inline_scanned, buf.len());
        }
        buf.extend_from_slice(b"\r\n");
        assert!(matches!(
            p.parse(&buf, &lim).unwrap(),
            Parsed::Command { .. }
        ));
    }

    #[test]
    fn reports_needed_size_for_large_bulk() {
        let lim = Limits::default();
        let mut p = Parser::new();
        let buf = b"*2\r\n$3\r\nSET\r\n$1000\r\nab";
        assert_eq!(p.parse(buf, &lim).unwrap(), Parsed::Incomplete);
        assert_eq!(p.needed(), 13 + 7 + 1000 + 2);
    }
}
