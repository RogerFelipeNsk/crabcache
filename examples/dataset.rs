//! Writes a realistic cache dataset as RESP `SET` commands, for `redis-cli --pipe`.
//!
//! ```text
//! cargo run --release --example dataset -- <session|product|api> <count> > data.resp
//! ```
//!
//! Keys are `<kind>:<n>`; values are compact JSON with the shared structure typical of cache entries
//! (same fields, enums and formats, varying ids, names, dates and tokens). Output is deterministic.

use std::fmt::Write as _;
use std::io::{BufWriter, Write};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len() as u64) as usize]
    }
}

const FIRST: &[&str] = &[
    "Ana", "Bruno", "Carla", "Diego", "Elisa", "Felipe", "Gabriela", "Hugo", "Isabela", "João",
    "Karen", "Lucas", "Marina", "Nicolas", "Olivia", "Pedro", "Rafaela", "Samuel", "Tatiana",
    "Vitor",
];
const LAST: &[&str] = &[
    "Silva",
    "Santos",
    "Oliveira",
    "Souza",
    "Pereira",
    "Costa",
    "Rodrigues",
    "Almeida",
    "Nascimento",
    "Lima",
    "Araújo",
    "Fernandes",
    "Carvalho",
    "Gomes",
    "Martins",
    "Rocha",
];
const WORDS: &[&str] = &[
    "camiseta",
    "tênis",
    "notebook",
    "fone",
    "mochila",
    "relógio",
    "cadeira",
    "mesa",
    "luminária",
    "garrafa",
    "caneca",
    "teclado",
    "mouse",
    "monitor",
    "jaqueta",
    "boné",
    "carregador",
    "câmera",
    "livro",
    "panela",
];
const ADJ: &[&str] = &[
    "preto",
    "branco",
    "azul",
    "premium",
    "esportivo",
    "compacto",
    "sem fio",
    "ergonômico",
    "térmico",
    "slim",
];
const CATS: &[&str] = &[
    "moda",
    "eletrônicos",
    "casa",
    "esporte",
    "escritório",
    "cozinha",
    "livros",
    "acessórios",
];

fn ts(r: &mut Rng) -> String {
    format!(
        "2026-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1 + r.below(12),
        1 + r.below(28),
        r.below(24),
        r.below(60),
        r.below(60)
    )
}

fn session(r: &mut Rng) -> String {
    let (f, l) = (r.pick(FIRST), r.pick(LAST));
    let mut cart = String::new();
    for i in 0..r.below(4) {
        if i > 0 {
            cart.push(',');
        }
        let _ = write!(
            cart,
            r#"{{"sku":"SKU-{}","qty":{}}}"#,
            1000 + r.below(99_000),
            1 + r.below(4)
        );
    }
    let roles = if r.below(4) == 0 {
        r#""user","beta""#
    } else {
        r#""user""#
    };
    format!(
        r#"{{"user_id":{},"name":"{f} {l}","email":"{}.{}{}@example.com","roles":[{roles}],"locale":"{}","theme":"{}","created_at":"{}","last_seen":"{}","cart":[{cart}],"csrf":"{:016x}{:016x}"}}"#,
        1 + r.below(10_000_000),
        f.to_lowercase(),
        l.to_lowercase(),
        1 + r.below(999),
        r.pick(&["pt-BR", "en-US", "es-ES"]),
        r.pick(&["dark", "light", "system"]),
        ts(r),
        ts(r),
        r.next(),
        r.next()
    )
}

fn product(r: &mut Rng) -> String {
    let mut tags = String::new();
    for i in 0..2 + r.below(4) {
        if i > 0 {
            tags.push(',');
        }
        let _ = write!(
            tags,
            r#""{}""#,
            if r.below(2) == 0 {
                r.pick(WORDS)
            } else {
                r.pick(ADJ)
            }
        );
    }
    let title = r.pick(WORDS);
    let mut title = title.to_string();
    if let Some(c) = title.get(..1) {
        title = c.to_uppercase() + &title[1..];
    }
    format!(
        r#"{{"id":{},"title":"{title} {} {}","price":{}.{:02},"currency":"BRL","stock":{},"category":"{}","tags":[{tags}],"rating":{}.{},"reviews":{},"updated_at":"{}","seller":{{"id":{},"name":"Loja {}","verified":{}}}}}"#,
        1 + r.below(5_000_000),
        r.pick(ADJ),
        r.pick(ADJ),
        9 + r.below(4990),
        r.below(100),
        r.below(500),
        r.pick(CATS),
        1 + r.below(4),
        r.below(10),
        r.below(20_000),
        ts(r),
        1 + r.below(50_000),
        r.pick(LAST),
        r.below(10) < 7
    )
}

fn api(r: &mut Rng) -> String {
    let mut items = String::new();
    for i in 0..1 + r.below(4) {
        if i > 0 {
            items.push(',');
        }
        let _ = write!(
            items,
            r#"{{"id":{},"status":"{}","amount":{}.{:02},"at":"{}"}}"#,
            1 + r.below(1_000_000_000),
            r.pick(&["ok", "pending", "failed"]),
            1 + r.below(999),
            r.below(100),
            ts(r)
        );
    }
    format!(
        r#"{{"request_id":"{:016x}{:016x}","status":200,"data":{{"items":[{items}],"page":1,"has_more":{}}},"took_ms":{}}}"#,
        r.next(),
        r.next(),
        r.below(10) < 3,
        1 + r.below(900)
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(kind), Some(count)) = (args.get(1), args.get(2).and_then(|n| n.parse::<u64>().ok()))
    else {
        eprintln!("usage: dataset <session|product|api> <count>");
        std::process::exit(2);
    };
    let generate: fn(&mut Rng) -> String = match kind.as_str() {
        "session" => session,
        "product" => product,
        "api" => api,
        _ => {
            eprintln!("unknown kind {kind}");
            std::process::exit(2);
        }
    };
    let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ kind.len() as u64);
    let mut out = BufWriter::new(std::io::stdout().lock());
    for i in 0..count {
        let key = format!("{kind}:{i}");
        let value = generate(&mut r);
        let _ = write!(
            out,
            "*3\r\n$3\r\nSET\r\n${}\r\n{key}\r\n${}\r\n{value}\r\n",
            key.len(),
            value.len()
        );
    }
    let _ = out.flush();
}
