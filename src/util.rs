//! Small parsing helpers shared by commands and configuration.

/// Strict integer parsing with the rules of Redis `string2ll`: no `+`, no leading zeros, no spaces,
/// no `-0`, and must fit in an `i64`.
pub fn parse_i64(s: &[u8]) -> Option<i64> {
    if s.is_empty() || s.len() > 20 {
        return None;
    }
    if s == b"0" {
        return Some(0);
    }
    let (neg, digits) = match s.split_first() {
        Some((b'-', rest)) => (true, rest),
        _ => (false, s),
    };
    match digits.first() {
        Some(b'1'..=b'9') => {}
        _ => return None,
    }
    let mut v: u64 = 0;
    for &c in digits {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.checked_mul(10)?.checked_add((c - b'0') as u64)?;
    }
    if neg {
        (v <= i64::MAX as u64 + 1).then(|| (v as i64).wrapping_neg())
    } else {
        (v <= i64::MAX as u64).then_some(v as i64)
    }
}

/// Parses memory sizes like `0`, `1048576`, `100kb`, `64mb`, `2gb` (case-insensitive, binary units),
/// matching Redis' `memtoull`.
pub fn parse_memory(s: &[u8]) -> Option<u64> {
    let lower = s.to_ascii_lowercase();
    let units: [(&[u8], u64); 7] = [
        (b"gb", 1 << 30),
        (b"mb", 1 << 20),
        (b"kb", 1 << 10),
        (b"g", 1_000_000_000),
        (b"m", 1_000_000),
        (b"k", 1_000),
        (b"b", 1),
    ];
    let (num, mul) = units
        .iter()
        .find_map(|(u, m)| lower.strip_suffix(*u).map(|n| (n, *m)))
        .unwrap_or((&lower[..], 1));
    if num.is_empty() || !num.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(num)
        .ok()?
        .parse::<u64>()
        .ok()?
        .checked_mul(mul)
}

pub fn eq_ic(a: &[u8], b: &str) -> bool {
    a.eq_ignore_ascii_case(b.as_bytes())
}

/// Glob-style matching with Redis `stringmatchlen` semantics: `*`, `?`, `[abc]`, `[^a-z]`, `\x`.
/// Uses star backtracking, so it runs in O(pattern * string) at worst.
pub fn glob_match(pat: &[u8], s: &[u8], nocase: bool) -> bool {
    let eq = |a: u8, b: u8| {
        if nocase {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    };
    let (mut p, mut i) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while i < s.len() {
        if p < pat.len() {
            match pat[p] {
                b'*' => {
                    while p < pat.len() && pat[p] == b'*' {
                        p += 1;
                    }
                    if p == pat.len() {
                        return true;
                    }
                    star = Some((p, i));
                    continue;
                }
                b'?' => {
                    p += 1;
                    i += 1;
                    continue;
                }
                b'[' => {
                    let (matched, next) = match_class(pat, p + 1, s[i], nocase);
                    if matched {
                        p = next;
                        i += 1;
                        continue;
                    }
                }
                b'\\' if p + 1 < pat.len() => {
                    if eq(pat[p + 1], s[i]) {
                        p += 2;
                        i += 1;
                        continue;
                    }
                }
                c => {
                    if eq(c, s[i]) {
                        p += 1;
                        i += 1;
                        continue;
                    }
                }
            }
        }
        match star {
            Some((sp, si)) => {
                p = sp;
                i = si + 1;
                star = Some((sp, si + 1));
            }
            None => return false,
        }
    }
    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }
    p == pat.len()
}

/// Matches `c` against the class starting at `p` (just after `[`). Returns (matched, index after `]`).
fn match_class(pat: &[u8], mut p: usize, c: u8, nocase: bool) -> (bool, usize) {
    let norm = |x: u8| if nocase { x.to_ascii_lowercase() } else { x };
    let c = norm(c);
    let negate = p < pat.len() && pat[p] == b'^';
    if negate {
        p += 1;
    }
    let mut matched = false;
    while p < pat.len() && pat[p] != b']' {
        if pat[p] == b'\\' && p + 1 < pat.len() {
            matched |= norm(pat[p + 1]) == c;
            p += 2;
        } else if p + 2 < pat.len() && pat[p + 1] == b'-' && pat[p + 2] != b']' {
            let (mut lo, mut hi) = (norm(pat[p]), norm(pat[p + 2]));
            if lo > hi {
                std::mem::swap(&mut lo, &mut hi);
            }
            matched |= (lo..=hi).contains(&c);
            p += 3;
        } else {
            matched |= norm(pat[p]) == c;
            p += 1;
        }
    }
    // An unterminated class ends at the end of the pattern, as in Redis.
    let next = if p < pat.len() { p + 1 } else { p };
    (matched != negate, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_integers() {
        for (s, v) in [
            ("0", 0),
            ("1", 1),
            ("-1", -1),
            ("9223372036854775807", i64::MAX),
            ("-9223372036854775808", i64::MIN),
        ] {
            assert_eq!(parse_i64(s.as_bytes()), Some(v), "{s}");
        }
        for s in [
            "",
            "-",
            "+1",
            "01",
            "-0",
            " 1",
            "1 ",
            "1.0",
            "9223372036854775808",
            "-9223372036854775809",
            "abc",
        ] {
            assert_eq!(parse_i64(s.as_bytes()), None, "{s}");
        }
    }

    #[test]
    fn memory_sizes() {
        assert_eq!(parse_memory(b"0"), Some(0));
        assert_eq!(parse_memory(b"100"), Some(100));
        assert_eq!(parse_memory(b"1kb"), Some(1024));
        assert_eq!(parse_memory(b"64MB"), Some(64 << 20));
        assert_eq!(parse_memory(b"2gb"), Some(2 << 30));
        assert_eq!(parse_memory(b"1k"), Some(1000));
        assert_eq!(parse_memory(b"mb"), None);
        assert_eq!(parse_memory(b"-1"), None);
    }

    #[test]
    fn globs() {
        let cases: &[(&str, &str, bool)] = &[
            ("*", "anything", true),
            ("h?llo", "hello", true),
            ("h?llo", "hllo", false),
            ("h*llo", "heeeello", true),
            ("h[ae]llo", "hallo", true),
            ("h[ae]llo", "hillo", false),
            ("h[^e]llo", "hallo", true),
            ("h[^e]llo", "hello", false),
            ("h[a-b]llo", "hbllo", true),
            ("h[b-a]llo", "hallo", true),
            ("user:*:name", "user:42:name", true),
            ("user:*:name", "user:42:age", false),
            ("a\\*b", "a*b", true),
            ("a\\*b", "axb", false),
            (
                "*a*a*a*a*a*a*a*b",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                false,
            ),
            ("", "", true),
            ("", "a", false),
        ];
        for &(p, s, want) in cases {
            assert_eq!(
                glob_match(p.as_bytes(), s.as_bytes(), false),
                want,
                "{p} ~ {s}"
            );
        }
        assert!(glob_match(b"MAXMEM*", b"maxmemory", true));
    }
}
