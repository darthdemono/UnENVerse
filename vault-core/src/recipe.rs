// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! Password recipes (R41): a template such as `{prefix}-{list:food}-{random:12:base62}` that makes
//! an ordinary password.
//!
//! A recipe is configuration, not a scheme. It runs once, at generation, and the result is stored
//! like any other password, so redaction, history, `exec`, `render` and an older desktop keep
//! working with no change. Nothing here reads the clock or the vault: the caller passes the date,
//! the entry's facts, the lists and a source of randomness, so a recipe with the same template and
//! the same draws renders the same string.
//!
//! The token list is data (`data/recipe-tokens.json`). Every id in it has an implementation in this
//! file and every implementation has a descriptor; a test fails when either side is missing one.
//!
//! Strength is counted honestly: only the random draws add bits. A lengthener adds length and no
//! bits, so the figure never rises above what was drawn.

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

type HmacSha256 = Hmac<Sha256>;

/// Label of the lengthener derivation. The salt is its SHA-256, so a new label is a new version:
/// results of `v1` never change.
const EXPAND_LABEL: &[u8] = b"unv/recipe/expand/v1";
/// The longest single part a token may ask for.
const MAX_PART: usize = 256;
/// The default floor under the random bits of a result.
pub const DEFAULT_FLOOR_BITS: f64 = 64.0;

const REGISTRY: &str = include_str!("../data/recipe-tokens.json");
const BUILTIN_LISTS: &str = include_str!("../data/recipe-lists.json");

/// Tokens that produce a value.
pub const TOKEN_IDS: &[&str] = &[
    "random",
    "digits",
    "letters",
    "hex",
    "base32",
    "base36",
    "base62",
    "symbols",
    "pin",
    "number",
    "year",
    "syllables",
    "mask",
    "dice",
    "one_of",
    "uuid_chunk",
    "list",
    "prefix",
    "suffix",
    "divider",
    "adj",
    "noun",
    "verb",
    "color",
    "animal",
    "food",
    "city",
    "country",
    "name",
    "element",
    "planet",
    "month",
    "weekday",
    "bip39",
    "provider",
    "provider_short",
    "host",
    "host_label",
    "tld",
    "url",
    "user",
    "email_local",
    "project",
    "type",
    "id",
    "key_id",
    "created",
    "now",
    "date",
    "counter",
    "var",
    "fixed",
    "typed",
];

/// Transforms and lengtheners, used after a `|`.
pub const TRANSFORM_IDS: &[&str] = &[
    "lower",
    "upper",
    "title",
    "sentence",
    "camel",
    "snake",
    "alt",
    "toggle",
    "first",
    "last",
    "skip",
    "take",
    "trim",
    "reverse",
    "repeat",
    "dedupe",
    "remove",
    "keep",
    "replace",
    "leet",
    "drop_vowels",
    "pad",
    "initials",
    "join",
    "add",
    "mul",
    "mod",
    "roman",
    "spell",
    "to_hex",
    "hash",
    "check_tail",
    "ensure",
    "class_fill",
    "expand",
    "expand_words",
    "expand_groups",
    "site_bound",
    "family_bound",
    "rotate_by_counter",
    "keyed_case",
];

/// The registry as JSON (descriptors for every token): what the settings forms are drawn from.
pub fn registry() -> Value {
    serde_json::from_str(REGISTRY).unwrap_or(Value::Null)
}

/// The built-in lists by name.
pub fn builtin_lists() -> BTreeMap<String, Vec<String>> {
    let v: Value = serde_json::from_str(BUILTIN_LISTS).unwrap_or(Value::Null);
    let mut out = BTreeMap::new();
    if let Some(map) = v.get("lists").and_then(Value::as_object) {
        for (k, list) in map {
            let words = list
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|w| w.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(k.clone(), words);
        }
    }
    out
}

// ── Randomness ────────────────────────────────────────────────────────────────────────────────

/// A source of uniform draws. The recipe never asks for more than `below`.
pub trait Draw {
    fn next_u64(&mut self) -> u64;

    /// A uniform number in `0..n` (rejection sampling, so no modulo bias). `n` must be above 0.
    fn below(&mut self, n: u64) -> u64 {
        let zone = (u64::MAX / n) * n;
        loop {
            let r = self.next_u64();
            if r < zone {
                return r % n;
            }
        }
    }

    /// An error the source met (an external device failing); `generate` returns it.
    fn failure(&self) -> Option<String> {
        None
    }
}

/// The operating system's generator.
pub struct SysDraw;

impl Draw for SysDraw {
    fn next_u64(&mut self) -> u64 {
        use rand::Rng as _;
        let mut b = [0u8; 8];
        rand::rng().fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
}

/// Draws from a pool filled by `entropy::fill`, so a chosen entropy source is honoured and an
/// external device is read once per refill and not once per character.
pub struct PoolDraw {
    source: crate::entropy::Source,
    pool: Vec<u8>,
    at: usize,
    error: Option<String>,
}

impl PoolDraw {
    pub fn new(source: crate::entropy::Source) -> Self {
        Self {
            source,
            pool: Vec::new(),
            at: 0,
            error: None,
        }
    }
}

impl Draw for PoolDraw {
    fn next_u64(&mut self) -> u64 {
        if self.at + 8 > self.pool.len() {
            let mut fresh = vec![0u8; 4096];
            match crate::entropy::fill(&self.source, "recipe", &mut fresh) {
                Ok(()) => {
                    self.pool = fresh;
                    self.at = 0;
                }
                Err(e) => {
                    self.error = Some(e);
                    return 0;
                }
            }
        }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.pool[self.at..self.at + 8]);
        self.at += 8;
        u64::from_le_bytes(b)
    }

    fn failure(&self) -> Option<String> {
        self.error.clone()
    }
}

/// A fixed sequence for tests and goldens (xorshift64*). Never used for a real password.
pub struct SeqDraw(pub u64);

impl Draw for SeqDraw {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0.max(1);
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

// ── The template ──────────────────────────────────────────────────────────────────────────────

/// One argument of a token: literal text, or `$name` (a fact or entry datum).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
    pub text: String,
    pub quoted: bool,
}

/// `{kind:arg:arg|transform:arg}`, optionally marked droppable with `?N`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: String,
    pub args: Vec<Arg>,
    pub pipes: Vec<(String, Vec<Arg>)>,
    /// Priority to drop this token when a site's rules force a shorter password; highest first.
    pub droppable: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Lit(String),
    Tok(Token),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub segs: Vec<Seg>,
}

/// Splits on `sep` outside double quotes (a backslash escapes inside quotes).
fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let (mut quoted, mut escaped) = (false, false);
    for c in s.chars() {
        let cur = out.last_mut().expect("never empty");
        if quoted {
            cur.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            cur.push(c);
        } else if c == sep {
            out.push(String::new());
        } else {
            cur.push(c);
        }
    }
    out
}

fn parse_arg(raw: &str) -> Arg {
    let t = raw.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        let mut text = String::new();
        let mut esc = false;
        for c in t[1..t.len() - 1].chars() {
            if esc {
                text.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else {
                text.push(c);
            }
        }
        return Arg { text, quoted: true };
    }
    Arg {
        text: t.to_string(),
        quoted: false,
    }
}

fn parse_call(piece: &str) -> (String, Vec<Arg>) {
    let parts = split_top(piece.trim(), ':');
    let name = parts[0].trim().to_string();
    let args = parts[1..].iter().map(|a| parse_arg(a)).collect();
    (name, args)
}

fn parse_token(body: &str) -> Result<Token, String> {
    let pieces = split_top(body, '|');
    let mut head = pieces[0].trim().to_string();
    let mut droppable = None;
    if let Some(at) = head.rfind('?') {
        let tail = &head[at + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            droppable = Some(
                tail.parse::<u32>()
                    .map_err(|_| "the drop priority is too large")?,
            );
            head.truncate(at);
        }
    }
    let (kind, args) = parse_call(&head);
    if kind.is_empty() {
        return Err("a token needs a name, such as {random:12}".into());
    }
    if !TOKEN_IDS.contains(&kind.as_str()) {
        return Err(format!(
            "`{{{kind}}}` is not a token (see `unv recipe tokens` for the list)"
        ));
    }
    let mut pipes = Vec::new();
    for p in &pieces[1..] {
        let (name, a) = parse_call(p);
        if !TRANSFORM_IDS.contains(&name.as_str()) {
            return Err(format!(
                "`|{name}` is not a transform (see `unv recipe tokens` for the list)"
            ));
        }
        pipes.push((name, a));
    }
    Ok(Token {
        kind,
        args,
        pipes,
        droppable,
    })
}

/// Parses a template. Text outside braces is literal; `{{` and `}}` are literal braces.
pub fn parse(template: &str) -> Result<Template, String> {
    let chars: Vec<char> = template.chars().collect();
    let mut segs = Vec::new();
    let mut lit = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '{' && chars.get(i + 1) == Some(&'{') {
            lit.push('{');
            i += 2;
        } else if c == '}' && chars.get(i + 1) == Some(&'}') {
            lit.push('}');
            i += 2;
        } else if c == '}' {
            return Err(format!(
                "a stray `}}` at position {}; write `}}}}` for a literal one",
                i + 1
            ));
        } else if c == '{' {
            let (mut j, mut quoted, mut esc) = (i + 1, false, false);
            let mut body = String::new();
            let mut closed = false;
            while j < chars.len() {
                let d = chars[j];
                if quoted {
                    if esc {
                        esc = false;
                    } else if d == '\\' {
                        esc = true;
                    } else if d == '"' {
                        quoted = false;
                    }
                } else if d == '"' {
                    quoted = true;
                } else if d == '}' {
                    closed = true;
                    break;
                }
                body.push(d);
                j += 1;
            }
            if !closed {
                return Err(format!("the `{{` at position {} is never closed", i + 1));
            }
            if !lit.is_empty() {
                segs.push(Seg::Lit(std::mem::take(&mut lit)));
            }
            segs.push(Seg::Tok(parse_token(&body)?));
            i = j + 1;
        } else {
            lit.push(c);
            i += 1;
        }
    }
    if !lit.is_empty() {
        segs.push(Seg::Lit(lit));
    }
    if segs.is_empty() {
        return Err("the template is empty".into());
    }
    Ok(Template { segs })
}

// ── Context ───────────────────────────────────────────────────────────────────────────────────

/// What a recipe may read: the entry's public facts, the person's facts and lists, and the date.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    pub provider: String,
    pub host: String,
    pub url: String,
    pub user: String,
    pub email: String,
    pub id: String,
    pub key_id: String,
    pub project: String,
    pub secret_type: String,
    pub family: String,
    /// `YYYY-MM-DD` of creation; empty means "today".
    pub created: String,
    /// `YYYY-MM-DD` of today, passed in so nothing reads the clock here.
    pub now: String,
    pub counter: u64,
    pub vars: BTreeMap<String, String>,
    /// Your own lists; a name here wins over the built-in list of that name.
    pub lists: BTreeMap<String, Vec<String>>,
    /// Lists whose words are private (they hide which list and which words).
    pub private_lists: BTreeSet<String>,
    pub typed: Option<String>,
    /// Values the result must never equal (current and past values of the entry).
    pub avoid: Vec<String>,
}

// ── Values and parts ──────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Val {
    text: String,
    bits: f64,
    private: bool,
}

impl Val {
    fn fixed(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            bits: 0.0,
            private: false,
        }
    }
}

/// One token's share of a result, for the preview table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Part {
    pub token: String,
    pub kind: String,
    pub bits: f64,
    pub private: bool,
}

struct Args<'a> {
    list: &'a [Arg],
    ctx: &'a Ctx,
    name: &'a str,
}

impl Args<'_> {
    fn text(&self, i: usize) -> Result<Option<String>, String> {
        let Some(a) = self.list.get(i) else {
            return Ok(None);
        };
        if !a.quoted && a.text.starts_with('$') {
            let n = &a.text[1..];
            return self.ctx.var(n).map(Some).ok_or_else(|| {
                format!(
                    "`{}`: no fact named {n}; add it under Your facts",
                    self.name
                )
            });
        }
        Ok(Some(a.text.clone()))
    }

    fn text_or(&self, i: usize, default: &str) -> Result<String, String> {
        Ok(self
            .text(i)?
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| default.into()))
    }

    fn int(&self, i: usize, default: i64) -> Result<i64, String> {
        match self.text(i)? {
            None => Ok(default),
            Some(t) if t.is_empty() => Ok(default),
            Some(t) => t
                .trim()
                .parse::<i64>()
                .map_err(|_| format!("`{}`: `{t}` is not a whole number", self.name)),
        }
    }

    fn count(&self, i: usize, default: i64) -> Result<usize, String> {
        let n = self.int(i, default)?;
        if n < 0 || n as usize > MAX_PART {
            return Err(format!(
                "`{}`: the length must be 0 to {MAX_PART}, not {n}",
                self.name
            ));
        }
        Ok(n as usize)
    }
}

impl Ctx {
    fn var(&self, name: &str) -> Option<String> {
        let own = match name {
            "provider" => Some(&self.provider),
            "host" => Some(&self.host),
            "user" => Some(&self.user),
            "id" => Some(&self.id),
            "project" => Some(&self.project),
            _ => None,
        };
        match own {
            Some(v) if !v.is_empty() => Some(v.clone()),
            _ => self.vars.get(name).cloned(),
        }
    }

    fn list(&self, name: &str) -> Result<(Vec<String>, bool), String> {
        if let Some(l) = self.lists.get(name) {
            if !l.is_empty() {
                return Ok((l.clone(), self.private_lists.contains(name)));
            }
        }
        let built = builtin_lists();
        match built.get(name) {
            Some(l) => Ok((l.clone(), false)),
            None => {
                let mut have: Vec<String> = built.keys().cloned().collect();
                have.extend(self.lists.keys().cloned());
                have.sort();
                have.dedup();
                Err(format!(
                    "no list named {name}; lists you have: {}",
                    have.join(", ")
                ))
            }
        }
    }
}

// ── Alphabets ─────────────────────────────────────────────────────────────────────────────────

const SAFE: &str = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*-_=+?";
const CONSONANTS: &str = "bdfgklmnprstvz";
const VOWELS: &str = "aeiou";

fn alphabet(name: &str) -> Result<Vec<char>, String> {
    let s: String = match name {
        "safe" => SAFE.into(),
        "base62" => format!("{UPPER}{LOWER}{DIGITS}"),
        "base36" => format!("{LOWER}{DIGITS}"),
        "base32" => "0123456789ABCDEFGHJKMNPQRSTVWXYZ".into(),
        "hex" => "0123456789abcdef".into(),
        "digits" => DIGITS.into(),
        "letters" => format!("{UPPER}{LOWER}"),
        "lower" => LOWER.into(),
        "upper" => UPPER.into(),
        "symbols" => SYMBOLS.into(),
        other => other.into(),
    };
    let mut seen = Vec::new();
    for c in s.chars() {
        if !seen.contains(&c) {
            seen.push(c);
        }
    }
    if seen.len() < 2 {
        return Err(format!(
            "the alphabet `{name}` needs at least 2 different characters"
        ));
    }
    Ok(seen)
}

fn pick(rng: &mut dyn Draw, alpha: &[char], n: usize) -> Val {
    let text: String = (0..n)
        .map(|_| alpha[rng.below(alpha.len() as u64) as usize])
        .collect();
    Val {
        text,
        bits: n as f64 * (alpha.len() as f64).log2(),
        private: false,
    }
}

// ── Dates ─────────────────────────────────────────────────────────────────────────────────────

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DOWS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

fn parse_date(s: &str) -> Result<(i64, i64, i64), String> {
    let head = s.get(..10).unwrap_or(s);
    let mut it = head.split('-');
    let (y, m, d) = (it.next(), it.next(), it.next());
    let num = |x: Option<&str>| x.and_then(|v| v.parse::<i64>().ok());
    match (num(y), num(m), num(d)) {
        (Some(y), Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d) && y > 0 => {
            Ok((y, m, d))
        }
        _ => Err(format!("`{s}` is not a date like 2018-11-11")),
    }
}

fn weekday(y: i64, m: i64, d: i64) -> usize {
    const T: [i64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    ((y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d).rem_euclid(7)) as usize
}

fn format_date(fmt: &str, date: &str) -> Result<String, String> {
    let (y, m, d) = parse_date(date)?;
    let mut out = String::new();
    let mut rest = fmt;
    'outer: while !rest.is_empty() {
        for (tok, f) in [
            ("Month", MONTHS[(m - 1) as usize].to_string()),
            ("Mon", MONTHS[(m - 1) as usize][..3].to_string()),
            ("DOW", DOWS[weekday(y, m, d)].to_string()),
            ("YYYY", format!("{y:04}")),
            ("YY", format!("{:02}", y % 100)),
            ("MM", format!("{m:02}")),
            ("DD", format!("{d:02}")),
            ("M", m.to_string()),
            ("D", d.to_string()),
        ] {
            if let Some(r) = rest.strip_prefix(tok) {
                out.push_str(&f);
                rest = r;
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("not empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    Ok(out)
}

// ── Tokens ────────────────────────────────────────────────────────────────────────────────────

fn need(value: &str, token: &str, what: &str) -> Result<Val, String> {
    if value.is_empty() {
        Err(format!(
            "`{{{token}}}` needs {what}, and this entry has none"
        ))
    } else {
        Ok(Val::fixed(value))
    }
}

fn range_arg(a: &Args, i: usize, default: &str) -> Result<(i64, i64), String> {
    let text = a.text_or(i, default)?;
    let (lo, hi) = text
        .split_once("..")
        .ok_or_else(|| format!("`{}`: write the range as MIN..MAX", a.name))?;
    let num = |s: &str| {
        s.trim()
            .parse::<i64>()
            .map_err(|_| format!("`{}`: `{s}` is not a whole number", a.name))
    };
    let (lo, hi) = (num(lo)?, num(hi)?);
    if hi < lo {
        return Err(format!("`{}`: the range {lo}..{hi} is backwards", a.name));
    }
    Ok((lo, hi))
}

fn eval_token(tok: &Token, ctx: &Ctx, rng: &mut dyn Draw) -> Result<Val, String> {
    let a = Args {
        list: &tok.args,
        ctx,
        name: &tok.kind,
    };
    let k = tok.kind.as_str();
    let mut v = match k {
        "random" => {
            let n = a.count(0, 16)?;
            pick(rng, &alphabet(&a.text_or(1, "safe")?)?, n)
        }
        "digits" | "pin" => pick(
            rng,
            &alphabet("digits")?,
            a.count(0, if k == "pin" { 6 } else { 4 })?,
        ),
        "letters" => pick(rng, &alphabet("letters")?, a.count(0, 8)?),
        "hex" | "uuid_chunk" => pick(rng, &alphabet("hex")?, a.count(0, 8)?),
        "base32" => pick(rng, &alphabet("base32")?, a.count(0, 8)?),
        "base36" => pick(rng, &alphabet("base36")?, a.count(0, 10)?),
        "base62" => pick(rng, &alphabet("base62")?, a.count(0, 12)?),
        "symbols" => {
            let n = a.count(0, 2)?;
            pick(rng, &alphabet(&a.text_or(1, SYMBOLS)?)?, n)
        }
        "number" | "year" => {
            let (lo, hi) = range_arg(
                &a,
                0,
                if k == "year" {
                    "1990..2010"
                } else {
                    "100..999"
                },
            )?;
            let span = (hi - lo + 1) as u64;
            Val {
                text: (lo + rng.below(span) as i64).to_string(),
                bits: (span as f64).log2(),
                private: false,
            }
        }
        "syllables" => {
            let n = a.count(0, 3)?;
            let (c, vw): (Vec<char>, Vec<char>) =
                (CONSONANTS.chars().collect(), VOWELS.chars().collect());
            let mut text = String::new();
            for _ in 0..n {
                text.push(c[rng.below(c.len() as u64) as usize]);
                text.push(vw[rng.below(vw.len() as u64) as usize]);
            }
            Val {
                text,
                bits: n as f64 * ((c.len() * vw.len()) as f64).log2(),
                private: false,
            }
        }
        "mask" => {
            let mask = a.text_or(0, "CvcvCvcv99#")?;
            let mut out = Val::fixed("");
            for ch in mask.chars() {
                let set: Option<String> = match ch {
                    'c' => Some(CONSONANTS.into()),
                    'C' => Some(CONSONANTS.to_uppercase()),
                    'v' => Some(VOWELS.into()),
                    'V' => Some(VOWELS.to_uppercase()),
                    'a' => Some(LOWER.into()),
                    'A' => Some(UPPER.into()),
                    '9' => Some(DIGITS.into()),
                    '#' => Some(SYMBOLS.into()),
                    '?' => Some(SAFE.into()),
                    _ => None,
                };
                match set {
                    Some(s) => {
                        let p = pick(rng, &s.chars().collect::<Vec<_>>(), 1);
                        out.text.push_str(&p.text);
                        out.bits += p.bits;
                    }
                    None => out.text.push(ch),
                }
            }
            out
        }
        "dice" | "bip39" => {
            let words = crate::type_emit::bip39_words();
            let n = a.count(0, if k == "dice" { 4 } else { 1 })?;
            let joiner = if k == "dice" {
                "-".to_string()
            } else {
                a.text_or(1, "")?
            };
            let picked: Vec<&str> = (0..n)
                .map(|_| words[rng.below(words.len() as u64) as usize])
                .collect();
            Val {
                text: picked.join(&joiner),
                bits: n as f64 * (words.len() as f64).log2(),
                private: false,
            }
        }
        "one_of" => {
            let opts: Vec<String> = (0..tok.args.len())
                .map(|i| a.text(i))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect();
            if opts.len() < 2 {
                return Err("`one_of` needs at least two options, as in {one_of:red:green}".into());
            }
            Val {
                text: opts[rng.below(opts.len() as u64) as usize].clone(),
                bits: (opts.len() as f64).log2(),
                private: false,
            }
        }
        "list" | "prefix" | "suffix" | "divider" | "adj" | "noun" | "verb" | "color" | "animal"
        | "food" | "city" | "country" | "name" | "element" | "planet" | "month" | "weekday" => {
            let (name, off) = if k == "list" {
                (a.text_or(0, "food")?, 1)
            } else {
                (k.to_string(), 0)
            };
            let n = a.count(off, 1)?;
            let joiner = a.text_or(off + 1, "")?;
            let (words, private) = ctx.list(&name)?;
            if words.is_empty() {
                return Err(format!("the list {name} is empty"));
            }
            let picked: Vec<&str> = (0..n)
                .map(|_| words[rng.below(words.len() as u64) as usize].as_str())
                .collect();
            Val {
                text: picked.join(&joiner),
                bits: n as f64 * (words.len() as f64).log2(),
                private,
            }
        }
        "provider" => need(&ctx.provider, k, "a provider name")?,
        "provider_short" => {
            let s: String = ctx
                .provider
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .take(12)
                .collect::<String>()
                .to_lowercase();
            need(&s, k, "a provider name")?
        }
        "host" => need(&ctx.host, k, "an address")?,
        "host_label" => {
            let h = ctx.host.strip_prefix("www.").unwrap_or(&ctx.host);
            need(h.split('.').next().unwrap_or(""), k, "an address")?
        }
        "tld" => need(
            ctx.host
                .rsplit('.')
                .next()
                .filter(|t| *t != ctx.host)
                .unwrap_or(""),
            k,
            "an address with a domain",
        )?,
        "url" => need(&ctx.url, k, "an address")?,
        "user" => need(&ctx.user, k, "a user name")?,
        "email_local" => need(
            ctx.email
                .split('@')
                .next()
                .filter(|_| ctx.email.contains('@'))
                .unwrap_or(""),
            k,
            "an email",
        )?,
        "project" => need(&ctx.project, k, "a project")?,
        "type" => need(&ctx.secret_type, k, "a secret type")?,
        "key_id" => need(&ctx.key_id, k, "a key id")?,
        "id" => {
            let n = a.count(0, 0)?;
            let id = if n == 0 {
                ctx.id.clone()
            } else {
                ctx.id.chars().take(n).collect()
            };
            need(&id, k, "an id")?
        }
        "created" => {
            let date = if ctx.created.is_empty() {
                &ctx.now
            } else {
                &ctx.created
            };
            Val::fixed(format_date(&a.text_or(0, "YYYY-MM-DD")?, date)?)
        }
        "now" => Val::fixed(format_date(&a.text_or(0, "YYYY-MM-DD")?, &ctx.now)?),
        "date" => Val::fixed(format_date(
            &a.text_or(1, "YYYY-MM-DD")?,
            &a.text_or(0, "2018-11-11")?,
        )?),
        "counter" => {
            let w = a.count(0, 1)?;
            Val::fixed(format!("{:0w$}", ctx.counter.max(1), w = w))
        }
        "var" => {
            let n = a.text_or(0, "")?;
            let value = ctx
                .vars
                .get(&n)
                .cloned()
                .ok_or_else(|| format!("`var`: no fact named {n}; add it under Your facts"))?;
            Val {
                text: value,
                bits: 0.0,
                private: true,
            }
        }
        "fixed" => Val::fixed(a.text_or(0, "")?),
        "typed" => {
            let t = ctx
                .typed
                .clone()
                .ok_or("`typed` needs a value you type; none was given")?;
            Val {
                bits: estimate_bits(&t),
                text: t,
                private: true,
            }
        }
        other => return Err(format!("`{other}` has no implementation")),
    };
    for (name, args) in &tok.pipes {
        let pa = Args {
            list: args,
            ctx,
            name,
        };
        v = apply_transform(v, name, &pa)?;
    }
    Ok(v)
}

/// A rough strength for text the person typed: length times the size of the character pool it
/// draws on. An estimate, labelled as one, never a promise.
pub fn estimate_bits(s: &str) -> f64 {
    let pool = [
        (s.chars().any(|c| c.is_ascii_lowercase()), 26.0),
        (s.chars().any(|c| c.is_ascii_uppercase()), 26.0),
        (s.chars().any(|c| c.is_ascii_digit()), 10.0),
        (s.chars().any(|c| !c.is_ascii_alphanumeric()), 33.0),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, n)| n)
    .sum::<f64>();
    if pool < 2.0 {
        0.0
    } else {
        s.chars().count() as f64 * pool.log2() * 0.5
    }
}

// ── Transforms and lengtheners ────────────────────────────────────────────────────────────────

fn words_of(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

fn cap(w: &str) -> String {
    let mut ch = w.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &ch.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn scale(v: &mut Val, kept: usize, of: usize) {
    if of > 0 && kept < of {
        v.bits *= kept as f64 / of as f64;
    }
}

fn roman(mut n: i64) -> Result<String, String> {
    if !(1..=3999).contains(&n) {
        return Err("`roman` takes a number from 1 to 3999".into());
    }
    let mut out = String::new();
    for (v, s) in [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ] {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    Ok(out)
}

fn spell(n: u64) -> String {
    const ONES: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    match n {
        0..=19 => ONES[n as usize].into(),
        20..=99 => {
            let t = TENS[(n / 10) as usize];
            if n.is_multiple_of(10) {
                t.into()
            } else {
                format!("{t}-{}", ONES[(n % 10) as usize])
            }
        }
        100..=999 => {
            let h = format!("{} hundred", ONES[(n / 100) as usize]);
            if n.is_multiple_of(100) {
                h
            } else {
                format!("{h} {}", spell(n % 100))
            }
        }
        1000..=999_999 => {
            let t = format!("{} thousand", spell(n / 1000));
            if n.is_multiple_of(1000) {
                t
            } else {
                format!("{t} {}", spell(n % 1000))
            }
        }
        _ => {
            let m = format!("{} million", spell(n / 1_000_000));
            if n.is_multiple_of(1_000_000) {
                m
            } else {
                format!("{m} {}", spell(n % 1_000_000))
            }
        }
    }
}

fn expand_context(name: &str, ctx: &Ctx) -> Result<String, String> {
    match name {
        "none" => Ok(String::new()),
        "entry" => need(&ctx.id, "expand", "an entry id").map(|v| v.text),
        "provider" => need(&ctx.provider.to_lowercase(), "expand", "a provider").map(|v| v.text),
        "family" => need(&ctx.family, "family_bound", "a site family").map(|v| v.text),
        "host" => need(&ctx.host, "site_bound", "an address").map(|v| v.text),
        "counter" => Ok(ctx.counter.max(1).to_string()),
        other => Err(format!(
            "the context `{other}` is none, entry, provider, family, host or counter"
        )),
    }
}

/// HKDF-SHA256 with the fixed lengthener salt. Pinned by golden vectors: `v1` never changes.
fn derive(value: &str, context: &str, len: usize) -> Result<Vec<u8>, String> {
    let mut mac =
        HmacSha256::new_from_slice(&Sha256::digest(EXPAND_LABEL)).map_err(|e| e.to_string())?;
    mac.update(value.as_bytes());
    let prk = mac.finalize().into_bytes();
    let info = format!("v1|{context}");
    let (mut out, mut t, mut counter) = (Vec::with_capacity(len), Vec::<u8>::new(), 1u8);
    while out.len() < len {
        let mut mac = HmacSha256::new_from_slice(&prk).map_err(|e| e.to_string())?;
        mac.update(&t);
        mac.update(info.as_bytes());
        mac.update(&[counter]);
        t = mac.finalize().into_bytes().to_vec();
        out.extend_from_slice(&t);
        counter = counter
            .checked_add(1)
            .ok_or("the derived length is too long")?;
    }
    out.truncate(len);
    Ok(out)
}

fn derive_chars(value: &str, context: &str, alpha: &[char], n: usize) -> Result<String, String> {
    let stream = derive(value, context, n * 8 + 64)?;
    let limit = 65536 - 65536 % alpha.len();
    let mut out = String::new();
    for pair in stream.chunks(2) {
        if out.chars().count() == n {
            break;
        }
        if pair.len() < 2 {
            break;
        }
        let r = u16::from_be_bytes([pair[0], pair[1]]) as usize;
        if r < limit {
            out.push(alpha[r % alpha.len()]);
        }
    }
    if out.chars().count() < n {
        return Err("the derived stream ran out; ask for fewer characters".into());
    }
    Ok(out)
}

fn expanded(v: &Val, text: String, alpha_len: usize) -> Val {
    let n = text.chars().count();
    Val {
        bits: v.bits.min(n as f64 * (alpha_len as f64).log2()),
        text,
        private: v.private,
    }
}

fn hash_chars(text: &str, n: usize, alpha: &[char]) -> String {
    let d = Sha256::digest(text.as_bytes());
    (0..n)
        .map(|i| alpha[d[i % d.len()] as usize % alpha.len()])
        .collect()
}

fn apply_transform(mut v: Val, name: &str, a: &Args) -> Result<Val, String> {
    match name {
        "lower" => v.text = v.text.to_lowercase(),
        "upper" => v.text = v.text.to_uppercase(),
        "title" => {
            let mut out = String::new();
            let mut start = true;
            for c in v.text.chars() {
                if c.is_alphanumeric() {
                    out.extend(if start {
                        c.to_uppercase().collect::<Vec<_>>()
                    } else {
                        c.to_lowercase().collect()
                    });
                    start = false;
                } else {
                    out.push(c);
                    start = true;
                }
            }
            v.text = out;
        }
        "sentence" => v.text = cap(&v.text),
        "camel" => {
            let w = words_of(&v.text);
            v.text = w
                .iter()
                .enumerate()
                .map(|(i, w)| if i == 0 { w.to_lowercase() } else { cap(w) })
                .collect();
        }
        "snake" => v.text = words_of(&v.text).join("_").to_lowercase(),
        "alt" => {
            v.text = v
                .text
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i % 2 == 0 {
                        c.to_ascii_lowercase()
                    } else {
                        c.to_ascii_uppercase()
                    }
                })
                .collect()
        }
        "toggle" => {
            v.text = v
                .text
                .chars()
                .map(|c| {
                    if c.is_ascii_lowercase() {
                        c.to_ascii_uppercase()
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect()
        }
        "first" | "last" | "skip" => {
            let n = a.count(0, if name == "skip" { 1 } else { 4 })?;
            let all: Vec<char> = v.text.chars().collect();
            let kept: Vec<char> = match name {
                "first" => all.iter().copied().take(n).collect(),
                "last" => all
                    .iter()
                    .copied()
                    .skip(all.len().saturating_sub(n))
                    .collect(),
                _ => all.iter().copied().skip(n).collect(),
            };
            scale(&mut v, kept.len(), all.len());
            v.text = kept.into_iter().collect();
        }
        "take" => {
            let (lo, hi) = range_arg(a, 0, "0..4")?;
            let all: Vec<char> = v.text.chars().collect();
            let kept: Vec<char> = all
                .iter()
                .copied()
                .skip(lo as usize)
                .take((hi - lo) as usize)
                .collect();
            scale(&mut v, kept.len(), all.len());
            v.text = kept.into_iter().collect();
        }
        "trim" => v.text = v.text.trim().to_string(),
        "reverse" => v.text = v.text.chars().rev().collect(),
        "repeat" => {
            let n = a.count(0, 2)?;
            if v.text.len().saturating_mul(n) > MAX_PART * 4 {
                return Err("`repeat` would make the part too long".into());
            }
            v.text = v.text.repeat(n);
        }
        "dedupe" => {
            let mut out = String::new();
            for c in v.text.chars() {
                if !out.ends_with(c) {
                    out.push(c);
                }
            }
            v.text = out;
        }
        "remove" => {
            let set = a.text_or(0, "aeiou")?;
            v.text = v.text.chars().filter(|c| !set.contains(*c)).collect();
        }
        "keep" => {
            let class = a.text_or(0, "alnum")?;
            let ok: fn(char) -> bool = match class.as_str() {
                "digits" => |c| c.is_ascii_digit(),
                "letters" => |c| c.is_ascii_alphabetic(),
                "upper" => |c| c.is_ascii_uppercase(),
                "lower" => |c| c.is_ascii_lowercase(),
                "alnum" => |c| c.is_ascii_alphanumeric(),
                other => {
                    return Err(format!(
                        "`keep`: `{other}` is digits, letters, upper, lower or alnum"
                    ))
                }
            };
            let all = v.text.chars().count();
            v.text = v.text.chars().filter(|c| ok(*c)).collect();
            let now = v.text.chars().count();
            scale(&mut v, now, all);
        }
        "replace" => {
            v.text = v
                .text
                .replace(&a.text_or(0, "a")?, &a.text(1)?.unwrap_or_default())
        }
        "leet" => {
            let level = a.int(0, 1)?.clamp(1, 3);
            v.text = v
                .text
                .chars()
                .map(|c| match (c.to_ascii_lowercase(), level) {
                    ('a', _) => '4',
                    ('e', _) => '3',
                    ('o', _) => '0',
                    ('i', 2..) => '1',
                    ('s', 2..) => '5',
                    ('t', 3) => '7',
                    ('l', 3) => '1',
                    ('b', 3) => '8',
                    _ => c,
                })
                .collect()
        }
        "drop_vowels" => {
            v.text = v
                .text
                .chars()
                .filter(|c| !"aeiouAEIOU".contains(*c))
                .collect()
        }
        "pad" => {
            let n = a.count(0, 12)?;
            let ch = a.text_or(1, "x")?.chars().next().unwrap_or('x');
            let missing = n.saturating_sub(v.text.chars().count());
            let fill: String = std::iter::repeat_n(ch, missing).collect();
            v.text = if a.text_or(2, "right")? == "left" {
                fill + &v.text
            } else {
                v.text + &fill
            };
        }
        "initials" => {
            v.text = words_of(&v.text)
                .iter()
                .filter_map(|w| w.chars().next())
                .collect()
        }
        "join" => v.text = words_of(&v.text).join(&a.text_or(0, "-")?),
        "add" | "mul" | "mod" => {
            let n = v
                .text
                .trim()
                .parse::<i64>()
                .map_err(|_| format!("`{name}` needs a number, not `{}`", v.text))?;
            let m = a.int(
                0,
                if name == "mul" {
                    2
                } else if name == "mod" {
                    100
                } else {
                    1
                },
            )?;
            v.text = match name {
                "add" => n.checked_add(m),
                "mul" => n.checked_mul(m),
                _ if m == 0 => return Err("`mod` by zero".into()),
                _ => Some(n.rem_euclid(m)),
            }
            .ok_or("the number is too large")?
            .to_string();
        }
        "roman" => {
            v.text = roman(
                v.text
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| "`roman` needs a number")?,
            )?
        }
        "spell" => {
            v.text = spell(
                v.text
                    .trim()
                    .parse::<u64>()
                    .map_err(|_| "`spell` needs a whole number")?,
            )
        }
        "to_hex" => v.text = hex::encode(v.text.as_bytes()),
        "hash" => {
            let n = a.count(0, 8)?;
            v.text = hash_chars(&v.text, n, &alphabet("hex")?);
            v.bits = v.bits.min(n as f64 * 4.0);
        }
        "check_tail" => {
            let n = a.count(0, 2)?;
            let tail = hash_chars(&v.text, n, &alphabet("base32")?);
            v.text.push_str(&tail);
        }
        "ensure" | "class_fill" => {
            let classes = if name == "class_fill" {
                "upper+lower+digit+symbol".to_string()
            } else {
                a.text_or(0, "upper+lower+digit+symbol")?
            };
            for class in classes.split('+') {
                let (has, set): (fn(char) -> bool, &str) = match class {
                    "upper" => (|c| c.is_ascii_uppercase(), UPPER),
                    "lower" => (|c| c.is_ascii_lowercase(), LOWER),
                    "digit" => (|c| c.is_ascii_digit(), DIGITS),
                    "symbol" => (|c| !c.is_ascii_alphanumeric(), SYMBOLS),
                    other => {
                        return Err(format!(
                            "`{name}`: `{other}` is upper, lower, digit or symbol"
                        ))
                    }
                };
                if !v.text.chars().any(has) {
                    let alpha: Vec<char> = set.chars().collect();
                    v.text.push(
                        hash_chars(&format!("{}{class}", v.text), 1, &alpha)
                            .chars()
                            .next()
                            .unwrap_or('x'),
                    );
                }
            }
        }
        "expand" | "site_bound" | "family_bound" | "rotate_by_counter" => {
            let n = a.count(0, 22)?;
            let alpha_name = a.text_or(1, "base36")?;
            let alpha = alphabet(&alpha_name)?;
            let context = match name {
                "site_bound" => "host".to_string(),
                "family_bound" => "family".to_string(),
                "rotate_by_counter" => "counter".to_string(),
                _ => a.text_or(2, "none")?,
            };
            let ctx_text = expand_context(&context, a.ctx)?;
            let text = derive_chars(&v.text, &ctx_text, &alpha, n)?;
            v = expanded(&v, text, alpha.len());
        }
        "expand_words" => {
            let n = a.count(0, 4)?;
            let words = crate::type_emit::bip39_words();
            let alpha: Vec<char> = (0..words.len() as u32)
                .filter_map(|i| char::from_u32(0x100 + i))
                .collect();
            let idx = derive_chars(
                &v.text,
                &expand_context(&a.text_or(1, "none")?, a.ctx)?,
                &alpha,
                n,
            )?;
            let picked: Vec<&str> = idx
                .chars()
                .map(|c| words[(c as u32 - 0x100) as usize])
                .collect();
            v = Val {
                bits: v.bits.min(n as f64 * (words.len() as f64).log2()),
                text: picked.join("-"),
                private: v.private,
            };
        }
        "expand_groups" => {
            let (groups, size) = (a.count(0, 3)?, a.count(1, 5)?);
            let alpha = alphabet(&a.text_or(2, "base36")?)?;
            let flat = derive_chars(
                &v.text,
                &expand_context(&a.text_or(3, "none")?, a.ctx)?,
                &alpha,
                groups * size,
            )?;
            let chars: Vec<char> = flat.chars().collect();
            let text = chars
                .chunks(size.max(1))
                .map(|c| c.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("-");
            v = Val {
                bits: v
                    .bits
                    .min((groups * size) as f64 * (alpha.len() as f64).log2()),
                text,
                private: v.private,
            };
        }
        "keyed_case" => {
            let stream = derive(&v.text, "case", v.text.len().max(1))?;
            v.text = v
                .text
                .chars()
                .zip(stream)
                .map(|(c, b)| {
                    if b & 1 == 1 {
                        c.to_ascii_uppercase()
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect();
        }
        other => return Err(format!("`|{other}` has no implementation")),
    }
    Ok(v)
}

// ── Options, fitting and generation ───────────────────────────────────────────────────────────

/// Recipe-level settings, applied after the tokens render.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    /// `fit` (default: drop droppable parts when too long, pad when too short) or `refuse`.
    pub policy: String,
    pub require_upper: usize,
    pub require_lower: usize,
    pub require_digit: usize,
    pub require_symbol: usize,
    pub avoid_ambiguous: bool,
    pub forbid: String,
    pub allow: String,
    /// No run of the same character longer than this (0 means no limit).
    pub max_run: usize,
    pub no_sequences: bool,
    pub start_letter: bool,
    pub no_edge_symbol: bool,
    /// Group the result with `chunk_sep` every this many characters (0 means none).
    pub chunk: usize,
    pub chunk_sep: String,
    /// The least random bits a result may have; 64 when unset.
    pub floor_bits: Option<f64>,
}

impl Options {
    pub fn from_value(v: &Value) -> Result<Self, String> {
        if v.is_null() {
            return Ok(Self::default());
        }
        serde_json::from_value(v.clone()).map_err(|e| format!("the options are not valid: {e}"))
    }

    pub fn floor(&self) -> f64 {
        self.floor_bits.unwrap_or(DEFAULT_FLOOR_BITS)
    }
}

/// The finished password and how it was made. Nothing in it but `text` is secret.
#[derive(Debug, Clone, Serialize)]
pub struct Generated {
    pub text: String,
    /// Bits from the random draws, the figure the floor applies to.
    pub bits: f64,
    /// Bits that stay hidden even from someone who knows the scheme (private lists and facts).
    pub private_bits: f64,
    pub parts: Vec<Part>,
    pub dropped: Vec<String>,
    pub warnings: Vec<String>,
    /// True when no part is random and nothing is derived: a person could rebuild it from memory.
    pub rebuildable_from_memory: bool,
}

fn render(
    t: &Template,
    ctx: &Ctx,
    rng: &mut dyn Draw,
    dropped: &BTreeSet<usize>,
) -> Result<(String, Vec<Part>), String> {
    let mut text = String::new();
    let mut parts = Vec::new();
    for (i, seg) in t.segs.iter().enumerate() {
        match seg {
            Seg::Lit(l) => text.push_str(l),
            Seg::Tok(tok) => {
                if dropped.contains(&i) {
                    continue;
                }
                let v = eval_token(tok, ctx, rng)?;
                text.push_str(&v.text);
                parts.push(Part {
                    token: tok.kind.clone(),
                    kind: token_kind(tok),
                    bits: v.bits,
                    private: v.private,
                });
            }
        }
    }
    Ok((text, parts))
}

fn token_kind(tok: &Token) -> String {
    if tok.pipes.iter().any(|(n, _)| {
        matches!(
            n.as_str(),
            "expand"
                | "expand_words"
                | "expand_groups"
                | "site_bound"
                | "family_bound"
                | "rotate_by_counter"
                | "keyed_case"
        )
    }) {
        return "derive".into();
    }
    match tok.kind.as_str() {
        "random" | "digits" | "letters" | "hex" | "base32" | "base36" | "base62" | "symbols"
        | "pin" | "number" | "year" | "syllables" | "mask" | "dice" | "one_of" | "uuid_chunk" => {
            "random"
        }
        "list" | "prefix" | "suffix" | "divider" | "adj" | "noun" | "verb" | "color" | "animal"
        | "food" | "city" | "country" | "name" | "element" | "planet" | "month" | "weekday"
        | "bip39" => "list",
        "var" | "typed" | "fixed" => "typed",
        "created" | "now" | "date" | "counter" => "date",
        _ => "data",
    }
    .into()
}

fn is_ambiguous(c: char) -> bool {
    "0O1lI".contains(c)
}

fn sequence_at(chars: &[char]) -> bool {
    chars.windows(3).any(|w| {
        let (a, b, c) = (w[0] as i32, w[1] as i32, w[2] as i32);
        w.iter().all(|x| x.is_ascii_alphanumeric())
            && ((b - a == 1 && c - b == 1) || (a - b == 1 && b - c == 1))
    })
}

/// Why `text` breaks the options, or `None` when it keeps them. Length is judged by the caller.
fn violation(text: &str, o: &Options) -> Option<&'static str> {
    let chars: Vec<char> = text.chars().collect();
    if !o.allow.is_empty() && chars.iter().any(|c| !o.allow.contains(*c)) {
        return Some("a character outside the allowed set");
    }
    if chars.iter().any(|c| o.forbid.contains(*c)) {
        return Some("a forbidden character");
    }
    if o.avoid_ambiguous && chars.iter().any(|c| is_ambiguous(*c)) {
        return Some("an ambiguous character");
    }
    let count = |f: fn(&char) -> bool| chars.iter().filter(|c| f(c)).count();
    if count(char::is_ascii_uppercase) < o.require_upper {
        return Some("too few upper-case letters");
    }
    if count(char::is_ascii_lowercase) < o.require_lower {
        return Some("too few lower-case letters");
    }
    if count(char::is_ascii_digit) < o.require_digit {
        return Some("too few digits");
    }
    if chars.iter().filter(|c| !c.is_ascii_alphanumeric()).count() < o.require_symbol {
        return Some("too few symbols");
    }
    if o.max_run > 0 {
        let mut run = 1;
        for w in chars.windows(2) {
            run = if w[0] == w[1] { run + 1 } else { 1 };
            if run > o.max_run {
                return Some("a run of one character");
            }
        }
    }
    if o.no_sequences && sequence_at(&chars) {
        return Some("a sequence such as abc or 123");
    }
    if o.start_letter && !chars.first().is_some_and(|c| c.is_alphabetic()) {
        return Some("it does not start with a letter");
    }
    if o.no_edge_symbol
        && (chars.first().is_some_and(|c| !c.is_alphanumeric())
            || chars.last().is_some_and(|c| !c.is_alphanumeric()))
    {
        return Some("a symbol at the start or end");
    }
    None
}

fn chunked(text: &str, o: &Options) -> String {
    if o.chunk == 0 {
        return text.to_string();
    }
    let sep = if o.chunk_sep.is_empty() {
        "-"
    } else {
        &o.chunk_sep
    };
    text.chars()
        .collect::<Vec<_>>()
        .chunks(o.chunk)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(sep)
}

/// Makes a password from `t`. The random draws come from `rng`; everything else from `ctx`.
///
/// Constraints are met by rejection sampling, so every accepted draw stays uniform. A droppable
/// token is removed (highest priority number first) when a maximum length cannot otherwise be met.
/// If the random bits fall below the floor, nothing is generated: a hostile page that declares a
/// short maximum cannot steer the result to a weak password.
pub fn generate(
    t: &Template,
    o: &Options,
    ctx: &Ctx,
    rng: &mut dyn Draw,
) -> Result<Generated, String> {
    let mut droppable: Vec<(u32, usize)> = t
        .segs
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s {
            Seg::Tok(tok) => tok.droppable.map(|p| (p, i)),
            Seg::Lit(_) => None,
        })
        .collect();
    droppable.sort_by(|a, b| b.cmp(a));
    let mut dropped: BTreeSet<usize> = BTreeSet::new();
    let refuse = o.policy == "refuse";
    let mut last = "the constraints";
    for _ in 0..=droppable.len() {
        let mut too_long = false;
        for _ in 0..500 {
            let (text, mut parts) = render(t, ctx, rng, &dropped)?;
            if let Some(e) = rng.failure() {
                return Err(e);
            }
            let mut text = text;
            let mut warnings = Vec::new();
            if let Some(min) = o.min_len {
                if text.chars().count() < min {
                    if refuse {
                        return Err(format!(
                            "the recipe makes {} characters, below the minimum of {min}",
                            text.chars().count()
                        ));
                    }
                    let extra = pick(rng, &alphabet("safe")?, min - text.chars().count());
                    text.push_str(&extra.text);
                    parts.push(Part {
                        token: "pad".into(),
                        kind: "random".into(),
                        bits: extra.bits,
                        private: false,
                    });
                    warnings.push(format!(
                        "padded with {} random characters to reach the minimum of {min}",
                        extra.text.chars().count()
                    ));
                }
            }
            if let Some(why) = violation(&text, o) {
                last = why;
                continue;
            }
            let shown = chunked(&text, o);
            if let Some(max) = o.max_len {
                if shown.chars().count() > max {
                    too_long = true;
                    break;
                }
            }
            if ctx.avoid.contains(&shown) {
                last = "a value this entry already used";
                continue;
            }
            let bits: f64 = parts.iter().map(|p| p.bits).sum();
            if bits + 1e-9 < o.floor() {
                return Err(format!(
                    "cannot fit: the random parts give {bits:.0} bits, below the floor of {:.0}",
                    o.floor()
                ));
            }
            let names: Vec<String> = dropped
                .iter()
                .filter_map(|i| match &t.segs[*i] {
                    Seg::Tok(k) => Some(k.kind.clone()),
                    Seg::Lit(_) => None,
                })
                .collect();
            let expands = parts.iter().any(|p| p.kind == "derive");
            if parts
                .iter()
                .any(|p| p.kind == "list" && p.bits > 0.0 && p.bits < 6.0)
            {
                warnings.push(
                    "a list pick adds under 6 bits; a longer list or more picks is stronger".into(),
                );
            }
            let private_bits = parts.iter().filter(|p| p.private).map(|p| p.bits).sum();
            return Ok(Generated {
                text: shown,
                bits,
                private_bits,
                dropped: names,
                warnings,
                rebuildable_from_memory: bits == 0.0 && !expands,
                parts,
            });
        }
        if !too_long {
            return Err(format!("cannot fit: no result without {last} in 500 tries"));
        }
        if refuse {
            return Err(
                "cannot fit: the recipe is longer than the maximum and the policy is refuse".into(),
            );
        }
        match droppable.first().copied() {
            Some((_, i)) => {
                droppable.remove(0);
                dropped.insert(i);
            }
            None => {
                return Err("cannot fit: the recipe is longer than the maximum and has no droppable part (mark one with ?1, as in {suffix?1})".into())
            }
        }
    }
    Err("cannot fit: dropping every droppable part is still too long".into())
}

/// Parses `template` and generates in one call.
pub fn make(
    template: &str,
    o: &Options,
    ctx: &Ctx,
    rng: &mut dyn Draw,
) -> Result<Generated, String> {
    generate(&parse(template)?, o, ctx, rng)
}

/// The random bits a template would give, without drawing: the strength figure for the editor.
/// Errors from missing facts surface here too, so the editor shows them while the person types.
pub fn analyse(template: &str, o: &Options, ctx: &Ctx) -> Result<Generated, String> {
    let mut o = o.clone();
    o.max_len = None;
    generate(
        &parse(template)?,
        &Options {
            floor_bits: Some(0.0),
            ..o
        },
        ctx,
        &mut SeqDraw(0x5eed),
    )
}

// ── The vault side: saved recipes, lists, facts and rules ────────────────────────────────────
//
// Kept at the top level of the vault document: `recipes`, `recipe_lists`, `recipe_vars`,
// `recipe_rules` and `recipe_default`. An entry only carries `generated_by` (public metadata of how
// its password was made, never a draw) and, when the person picked one there, `recipe`.

/// A saved recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct Saved {
    pub name: String,
    pub template: String,
    pub options: Options,
}

fn text_of<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

/// The saved recipes of a vault.
pub fn saved(vault: &Value) -> Vec<Saved> {
    vault
        .get("recipes")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter(|r| !text_of(r, "name").is_empty())
                .map(|r| Saved {
                    name: text_of(r, "name").into(),
                    template: text_of(r, "template").into(),
                    options: r
                        .get("options")
                        .and_then(|o| Options::from_value(o).ok())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Stores a recipe by name (replacing one of that name when `replace`). The template must parse.
pub fn save(
    vault: &mut Value,
    name: &str,
    template: &str,
    options: &Options,
    replace: bool,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 || name.chars().any(|c| c.is_control()) {
        return Err("a recipe name is 1 to 64 characters".into());
    }
    parse(template)?;
    if vault.get("recipes").and_then(Value::as_array).is_none() {
        vault["recipes"] = serde_json::json!([]);
    }
    let list = vault["recipes"].as_array_mut().expect("just made");
    let row = serde_json::json!({
        "name": name,
        "template": template,
        "options": serde_json::to_value(options).map_err(|e| e.to_string())?,
    });
    match list.iter().position(|r| text_of(r, "name") == name) {
        Some(_) if !replace => Err(format!("a recipe named {name} exists; use --replace")),
        Some(i) => {
            list[i] = row;
            Ok(())
        }
        None => {
            list.push(row);
            Ok(())
        }
    }
}

/// Removes a recipe and every rule and default that named it. Returns whether it existed.
pub fn remove(vault: &mut Value, name: &str) -> bool {
    let mut found = false;
    if let Some(list) = vault.get_mut("recipes").and_then(Value::as_array_mut) {
        let before = list.len();
        list.retain(|r| text_of(r, "name") != name);
        found = list.len() != before;
    }
    if let Some(rules) = vault.get_mut("recipe_rules").and_then(Value::as_array_mut) {
        rules.retain(|r| text_of(r, "recipe") != name);
    }
    if vault.get("recipe_default").and_then(Value::as_str) == Some(name) {
        if let Some(o) = vault.as_object_mut() {
            o.remove("recipe_default");
        }
    }
    found
}

/// Your own lists: `(name, words, private)`.
pub fn custom_lists(vault: &Value) -> Vec<(String, Vec<String>, bool)> {
    vault
        .get("recipe_lists")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|x| {
                    let words = x
                        .get("words")
                        .and_then(Value::as_array)
                        .map(|w| {
                            w.iter()
                                .filter_map(|s| s.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();
                    (
                        text_of(x, "name").to_string(),
                        words,
                        x.get("private").and_then(Value::as_bool).unwrap_or(false),
                    )
                })
                .filter(|(n, _, _)| !n.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Stores one of your lists. Words are trimmed and deduplicated; fewer than 2 is refused.
pub fn save_list(
    vault: &mut Value,
    name: &str,
    words: &[String],
    private: bool,
) -> Result<usize, String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 32
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err("a list name is 1 to 32 letters, digits, - or _".into());
    }
    let mut clean: Vec<String> = Vec::new();
    for w in words {
        let w = w.trim();
        if !w.is_empty() && !clean.iter().any(|c| c == w) {
            clean.push(w.to_string());
        }
    }
    if clean.len() < 2 {
        return Err("a list needs at least 2 different words".into());
    }
    if vault
        .get("recipe_lists")
        .and_then(Value::as_array)
        .is_none()
    {
        vault["recipe_lists"] = serde_json::json!([]);
    }
    let list = vault["recipe_lists"].as_array_mut().expect("just made");
    let row = serde_json::json!({ "name": name, "private": private, "words": clean });
    match list.iter().position(|r| text_of(r, "name") == name) {
        Some(i) => list[i] = row,
        None => list.push(row),
    }
    Ok(clean.len())
}

/// Removes a list of yours. Returns whether it existed.
pub fn remove_list(vault: &mut Value, name: &str) -> bool {
    let Some(list) = vault.get_mut("recipe_lists").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = list.len();
    list.retain(|r| text_of(r, "name") != name);
    list.len() != before
}

/// Your facts: `(name, value, private)`.
pub fn facts(vault: &Value) -> Vec<(String, String, bool)> {
    vault
        .get("recipe_vars")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|x| {
                    (
                        text_of(x, "name").to_string(),
                        text_of(x, "value").to_string(),
                        x.get("private").and_then(Value::as_bool).unwrap_or(true),
                    )
                })
                .filter(|(n, _, _)| !n.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Sets a fact. Names are lower-case words so `{var:pet}` and `$pet` are never ambiguous.
pub fn set_fact(vault: &mut Value, name: &str, value: &str, private: bool) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty()
        || name.len() > 32
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("a fact name is 1 to 32 lower-case letters, digits or _".into());
    }
    if value.is_empty() || value.len() > 256 {
        return Err("a fact value is 1 to 256 characters".into());
    }
    if vault.get("recipe_vars").and_then(Value::as_array).is_none() {
        vault["recipe_vars"] = serde_json::json!([]);
    }
    let list = vault["recipe_vars"].as_array_mut().expect("just made");
    let row = serde_json::json!({ "name": name, "value": value, "private": private });
    match list.iter().position(|r| text_of(r, "name") == name) {
        Some(i) => list[i] = row,
        None => list.push(row),
    }
    Ok(())
}

/// Removes a fact. Returns whether it existed.
pub fn remove_fact(vault: &mut Value, name: &str) -> bool {
    let Some(list) = vault.get_mut("recipe_vars").and_then(Value::as_array_mut) else {
        return false;
    };
    let before = list.len();
    list.retain(|r| text_of(r, "name") != name);
    list.len() != before
}

/// What an assignment rule matches on.
pub const RULE_KINDS: [&str; 6] = ["provider", "family", "host", "type", "project", "tag"];

/// The assignment rules, in order: `(kind, value, recipe)`.
pub fn rules(vault: &Value) -> Vec<(String, String, String)> {
    vault
        .get("recipe_rules")
        .and_then(Value::as_array)
        .map(|l| {
            l.iter()
                .map(|r| {
                    (
                        text_of(r, "kind").into(),
                        text_of(r, "value").into(),
                        text_of(r, "recipe").into(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Appends a rule. The recipe must exist.
pub fn add_rule(vault: &mut Value, kind: &str, value: &str, recipe: &str) -> Result<(), String> {
    if !RULE_KINDS.contains(&kind) {
        return Err(format!("a rule matches on {}", RULE_KINDS.join(", ")));
    }
    if value.trim().is_empty() {
        return Err("a rule needs a value to match".into());
    }
    if !saved(vault).iter().any(|r| r.name == recipe) && recipe != "none" {
        return Err(format!(
            "no recipe named {recipe} (use `none` to switch recipes off for a match)"
        ));
    }
    if vault
        .get("recipe_rules")
        .and_then(Value::as_array)
        .is_none()
    {
        vault["recipe_rules"] = serde_json::json!([]);
    }
    vault["recipe_rules"]
        .as_array_mut()
        .expect("just made")
        .push(serde_json::json!({ "kind": kind, "value": value.trim(), "recipe": recipe }));
    Ok(())
}

/// Removes the rule at 1-based `n`. Returns whether it existed.
pub fn remove_rule(vault: &mut Value, n: usize) -> bool {
    match vault.get_mut("recipe_rules").and_then(Value::as_array_mut) {
        Some(l) if n >= 1 && n <= l.len() => {
            l.remove(n - 1);
            true
        }
        _ => false,
    }
}

fn first_host(entry: &Value) -> String {
    crate::sites::entry_sites(entry)
        .into_iter()
        .next()
        .map(|s| s.site.host)
        .unwrap_or_default()
}

fn rule_matches(kind: &str, value: &str, entry: &Value, vault: &Value) -> bool {
    let eq = |a: &str| a.eq_ignore_ascii_case(value);
    match kind {
        "provider" => eq(text_of(entry, "provider")),
        "type" => eq(match text_of(entry, "secretType") {
            "" => "api_key",
            t => t,
        }),
        "family" => eq(text_of(entry, "site_family")),
        "host" => crate::sites::entry_sites(entry).iter().any(|s| {
            s.site.host.eq_ignore_ascii_case(value)
                || s.site
                    .host
                    .ends_with(&format!(".{}", value.to_ascii_lowercase()))
        }),
        "tag" => entry
            .get("tags")
            .and_then(Value::as_array)
            .is_some_and(|t| t.iter().any(|x| x.as_str().is_some_and(eq))),
        "project" => entry
            .get("projectIds")
            .and_then(Value::as_array)
            .is_some_and(|ids| {
                ids.iter().filter_map(Value::as_str).any(|id| {
                    eq(id)
                        || vault
                            .get("projects")
                            .and_then(Value::as_array)
                            .is_some_and(|ps| {
                                ps.iter()
                                    .any(|p| text_of(p, "id") == id && eq(text_of(p, "name")))
                            })
                })
            }),
        _ => false,
    }
}

/// Which saved recipe applies to `entry`, and why: the entry's own pick, the first matching rule,
/// then the vault default; `None` means plain random, as before recipes existed.
pub fn resolve(vault: &Value, entry: &Value) -> Option<(Saved, String)> {
    let all = saved(vault);
    let find = |name: &str| all.iter().find(|r| r.name == name).cloned();
    let own = text_of(entry, "recipe");
    if own == "none" {
        return None;
    }
    if !own.is_empty() {
        return find(own).map(|r| (r, "this entry".into()));
    }
    for (kind, value, recipe) in rules(vault) {
        if rule_matches(&kind, &value, entry, vault) {
            return if recipe == "none" {
                None
            } else {
                find(&recipe).map(|r| (r, format!("rule: {kind} {value}")))
            };
        }
    }
    vault
        .get("recipe_default")
        .and_then(Value::as_str)
        .and_then(find)
        .map(|r| (r, "vault default".into()))
}

/// The context for generating a password for `entry` (or for no entry at all, when testing).
/// `now` is `YYYY-MM-DD`, passed in so nothing here reads the clock.
pub fn context(vault: &Value, entry: Option<&Value>, now: &str) -> Ctx {
    let mut c = Ctx {
        now: now.into(),
        counter: 1,
        ..Ctx::default()
    };
    for (n, v, _) in facts(vault) {
        c.vars.insert(n, v);
    }
    for (n, words, private) in custom_lists(vault) {
        if private {
            c.private_lists.insert(n.clone());
        }
        c.lists.insert(n, words);
    }
    let Some(e) = entry else {
        return c;
    };
    let t = |k: &str| text_of(e, k).to_string();
    c.provider = t("provider");
    c.host = first_host(e);
    c.url = e
        .get("sites")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
        .map(|s| text_of(s, "url").to_string())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| t("api_url"));
    c.user = t("username");
    c.email = t("email");
    c.id = t("id");
    c.key_id = t("key_id");
    c.family = t("site_family");
    c.secret_type = match t("secretType").as_str() {
        "" => "api_key".into(),
        other => other.into(),
    };
    c.created = t("created_at").chars().take(10).collect();
    c.project = e
        .get("projectIds")
        .and_then(Value::as_array)
        .and_then(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .find(|i| *i != "Universal")
        })
        .map(|id| {
            vault
                .get("projects")
                .and_then(Value::as_array)
                .and_then(|ps| ps.iter().find(|p| text_of(p, "id") == id))
                .map(|p| text_of(p, "name").to_string())
                .unwrap_or_else(|| id.to_string())
        })
        .unwrap_or_default();
    c.counter = e
        .get("generated_by")
        .and_then(|g| g.get("counter"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        + 1;
    // Never repeat what this entry holds now, or held before.
    if let Some(k) = e
        .get("api_key")
        .and_then(Value::as_str)
        .filter(|k| !k.is_empty())
    {
        c.avoid.push(k.to_string());
    }
    if let Some(h) = e.get("version_history").and_then(Value::as_array) {
        for old in h {
            if let Some(k) = old
                .get("api_key")
                .and_then(Value::as_str)
                .filter(|k| !k.is_empty())
            {
                c.avoid.push(k.to_string());
            }
        }
    }
    c
}

/// The public record of how a password was made: the recipe name, the counter and the date. It
/// holds no secret and no draw, so it is safe in every listing.
pub fn generated_by(recipe: &str, counter: u64, now: &str) -> Value {
    serde_json::json!({ "recipe": recipe, "version": 1, "counter": counter, "at": now })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        let mut c = Ctx {
            provider: "Instagram".into(),
            host: "www.instagram.com".into(),
            url: "https://www.instagram.com".into(),
            user: "joy".into(),
            email: "joy@example.org".into(),
            id: "0a1b2c3d-1111".into(),
            key_id: "k1".into(),
            project: "Social".into(),
            secret_type: "password".into(),
            family: "meta".into(),
            created: "2018-11-11".into(),
            now: "2026-11-15".into(),
            counter: 3,
            ..Ctx::default()
        };
        c.vars.insert("pet".into(), "rex".into());
        c
    }

    fn open() -> Options {
        Options {
            floor_bits: Some(0.0),
            ..Options::default()
        }
    }

    fn run(t: &str) -> Result<Generated, String> {
        make(t, &open(), &ctx(), &mut SeqDraw(7))
    }

    fn text(t: &str) -> String {
        run(t).unwrap().text
    }

    #[test]
    fn literals_braces_and_errors_in_the_grammar() {
        assert_eq!(text("a{{b}}c-{fixed:\"x y:z\"}"), "a{b}c-x y:z");
        for (bad, want) in [
            ("{random:4", "never closed"),
            ("a}b", "stray"),
            ("{nope}", "not a token"),
            ("{random:4|nope}", "not a transform"),
            ("{}", "needs a name"),
            ("", "empty"),
            ("{random:x}", "not a whole number"),
            ("{random:4:a}", "at least 2"),
            ("{number:5..1}", "backwards"),
            ("{number:5}", "MIN..MAX"),
            ("{list:nope}", "no list named nope"),
            ("{var:ghost}", "no fact named ghost"),
            ("{typed}", "typed"),
            ("{created:YYYY}{date:2018-13-40}", "not a date"),
            ("{random:999}", "0 to 256"),
        ] {
            let err = run(bad).unwrap_err();
            assert!(err.contains(want), "{bad}: {err}");
        }
    }

    #[test]
    fn the_registry_and_the_implementation_list_each_other() {
        let reg = registry();
        let tokens = reg["tokens"].as_array().unwrap();
        let mut seen = BTreeSet::new();
        for t in tokens {
            let id = t["id"].as_str().unwrap();
            assert!(seen.insert(id.to_string()), "{id} is listed twice");
            let kind = t["kind"].as_str().unwrap();
            let implemented = if matches!(kind, "transform" | "derive") {
                TRANSFORM_IDS.contains(&id)
            } else {
                TOKEN_IDS.contains(&id)
            };
            assert!(implemented, "{id} has a descriptor and no implementation");
            for f in ["label", "summary", "group", "bits", "example"] {
                assert!(!t[f].as_str().unwrap_or("").is_empty(), "{id}.{f}");
            }
        }
        for id in TOKEN_IDS.iter().chain(TRANSFORM_IDS) {
            assert!(
                seen.contains(*id),
                "{id} has an implementation and no descriptor"
            );
        }
        assert_eq!(seen.len(), TOKEN_IDS.len() + TRANSFORM_IDS.len());
    }

    #[test]
    fn every_documented_example_renders() {
        for t in registry()["tokens"].as_array().unwrap() {
            let ex = t["example"].as_str().unwrap();
            let mut c = ctx();
            c.typed = Some("Typed-123".into());
            make(ex, &open(), &c, &mut SeqDraw(11)).unwrap_or_else(|e| panic!("{ex}: {e}"));
        }
    }

    #[test]
    fn the_same_draws_give_the_same_password_and_other_draws_differ() {
        let t = "{prefix}-{list:food:2:+}-{random:12:base62}";
        let a = make(t, &open(), &ctx(), &mut SeqDraw(5)).unwrap();
        let b = make(t, &open(), &ctx(), &mut SeqDraw(5)).unwrap();
        assert_eq!(a.text, b.text);
        assert_ne!(
            a.text,
            make(t, &open(), &ctx(), &mut SeqDraw(6)).unwrap().text
        );
    }

    #[test]
    fn random_tokens_have_the_right_shape_and_the_bits_the_formula_says() {
        let g = run("{random:20:safe}").unwrap();
        assert_eq!(g.text.chars().count(), 20);
        assert!((g.bits - 20.0 * (alphabet("safe").unwrap().len() as f64).log2()).abs() < 1e-9);
        assert_eq!(text("{digits:6}").len(), 6);
        assert!(text("{digits:6}").chars().all(|c| c.is_ascii_digit()));
        assert!(text("{hex:9}").chars().all(|c| c.is_ascii_hexdigit()));
        assert!(text("{base32:9}").chars().all(|c| !"ILOU".contains(c)));
        assert!(text("{base36:9}")
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_eq!(
            text("{base62:9}|{letters:5}|{pin:4}|{uuid_chunk:4}").len(),
            9 + 1 + 5 + 1 + 4 + 1 + 4
        );
        assert!(text("{symbols:3}").chars().all(|c| SYMBOLS.contains(c)));
        let n: i64 = text("{number:100..102}").parse().unwrap();
        assert!((100..=102).contains(&n));
        assert!((1990..=2010).contains(&text("{year:1990..2010}").parse::<i64>().unwrap()));
        let s = text("{syllables:3}");
        assert_eq!(s.len(), 6);
        assert!(s.chars().step_by(2).all(|c| CONSONANTS.contains(c)));
        let m = text("{mask:\"CvcV-99#\"}");
        assert_eq!(m.chars().count(), 8);
        assert!(m.chars().nth(4) == Some('-'));
        let d = run("{dice:4}").unwrap();
        assert_eq!(d.text.split('-').count(), 4);
        assert!((d.bits - 44.0).abs() < 1e-9);
        assert!(["red", "green"].contains(&text("{one_of:red:green}").as_str()));
        assert!(run("{one_of:red}").is_err());
    }

    #[test]
    fn lists_prefer_your_own_and_report_their_size_in_bits() {
        let mut c = ctx();
        c.lists
            .insert("food".into(), vec!["kimchi".into(), "naan".into()]);
        c.private_lists.insert("food".into());
        let g = make("{list:food:3:/}", &open(), &c, &mut SeqDraw(2)).unwrap();
        assert_eq!(g.text.split('/').count(), 3);
        assert!((g.bits - 3.0).abs() < 1e-9);
        assert!(g.parts[0].private && g.private_bits == 3.0);
        assert!(g.warnings.iter().any(|w| w.contains("under 6 bits")));
        let built = run("{prefix}{divider}{adj}{noun}{verb}{color}{animal}{city}{country}{name}{element}{planet}{month}{weekday}{bip39:2: }").unwrap();
        assert!(built.bits > 30.0);
    }

    #[test]
    fn entry_data_dates_counters_and_facts_come_from_the_context() {
        assert_eq!(
            text("{provider}|{provider_short}|{host}|{host_label}|{tld}"),
            "Instagram|instagram|www.instagram.com|instagram|com"
        );
        assert_eq!(
            text("{url}|{user}|{email_local}|{project}|{type}|{key_id}|{id:4}"),
            "https://www.instagram.com|joy|joy|Social|password|k1|0a1b"
        );
        assert_eq!(text("{created:D_M_YYYY}"), "11_11_2018");
        assert_eq!(
            text("{created:DD/MM/YY Month Mon DOW}"),
            "11/11/18 November Nov Sun"
        );
        assert_eq!(
            text("{now:YYYY-MM-DD}|{date:2018-11-11:D_Mon_YY}|{counter:3}|{var:pet}"),
            "2026-11-15|11_Nov_18|003|rex"
        );
        assert_eq!(text("{fixed:$pet}{random:2:digits|first:0}"), "rex");
        // The maintainer's arrangement from the plan.
        let g =
            run("{provider}_{created:D_M_YYYY}_Password_{random:22:base36}_11_Nov_18_{digits:3}")
                .unwrap();
        assert!(g.text.starts_with("Instagram_11_11_2018_Password_"));
        assert!(g.text.contains("_11_Nov_18_"));
        // No data: the error says what is missing.
        let bare = Ctx::default();
        let e = make("{host}", &open(), &bare, &mut SeqDraw(1)).unwrap_err();
        assert!(e.contains("needs an address"), "{e}");
        let mut c = ctx();
        c.created.clear();
        assert_eq!(
            make("{created:YYYY}", &open(), &c, &mut SeqDraw(1))
                .unwrap()
                .text,
            "2026"
        );
        let mut typed = ctx();
        typed.typed = Some("Correct-Horse-9".into());
        let g = make("{typed}", &open(), &typed, &mut SeqDraw(1)).unwrap();
        assert!(g.text == "Correct-Horse-9" && g.bits > 20.0 && g.parts[0].private);
    }

    #[test]
    fn transforms_do_what_their_names_say() {
        for (t, want) in [
            ("{fixed:\"hello world\"|upper}", "HELLO WORLD"),
            ("{fixed:\"HELLO\"|lower}", "hello"),
            ("{fixed:\"hello big world\"|title}", "Hello Big World"),
            ("{fixed:\"hELLO\"|sentence}", "Hello"),
            ("{fixed:\"hello big world\"|camel}", "helloBigWorld"),
            ("{fixed:\"Hello Big World\"|snake}", "hello_big_world"),
            ("{fixed:\"abcd\"|alt}", "aBcD"),
            ("{fixed:\"aBc\"|toggle}", "AbC"),
            ("{fixed:\"abcdef\"|first:2}", "ab"),
            ("{fixed:\"abcdef\"|last:2}", "ef"),
            ("{fixed:\"abcdef\"|skip:4}", "ef"),
            ("{fixed:\"abcdef\"|take:1..3}", "bc"),
            ("{fixed:\"  x \"|trim}", "x"),
            ("{fixed:\"abc\"|reverse}", "cba"),
            ("{fixed:\"ab\"|repeat:3}", "ababab"),
            ("{fixed:\"aabbcc\"|dedupe}", "abc"),
            ("{fixed:\"banana\"|remove:an}", "b"),
            ("{fixed:\"a1b2\"|keep:digits}", "12"),
            ("{fixed:\"a-b\"|replace:\"-\":\"+\"}", "a+b"),
            ("{fixed:\"aeios\"|leet:2}", "43105"),
            ("{fixed:\"education\"|drop_vowels}", "dctn"),
            ("{fixed:\"ab\"|pad:5:x:left}", "xxxab"),
            ("{fixed:\"ab\"|pad:4:\"-\"}", "ab--"),
            ("{fixed:\"Hello Big World\"|initials}", "HBW"),
            ("{fixed:\"a b c\"|join:\"-\"}", "a-b-c"),
            ("{number:7..7|add:5}", "12"),
            ("{number:7..7|mul:3}", "21"),
            ("{number:7..7|mod:4}", "3"),
            ("{number:2018..2018|roman}", "MMXVIII"),
            ("{number:42..42|spell}", "forty-two"),
            (
                "{number:1234..1234|spell}",
                "one thousand two hundred thirty-four",
            ),
            ("{fixed:\"ab\"|to_hex}", "6162"),
        ] {
            assert_eq!(text(t), want, "{t}");
        }
        assert!(run("{fixed:\"x\"|add:1}").is_err());
        assert!(run("{number:5000..5000|roman}").is_err());
        assert!(run("{number:5..5|mod:0}").is_err());
        assert!(run("{fixed:\"x\"|keep:blue}").is_err());
        assert_eq!(text("{fixed:\"abc\"|hash:6}").len(), 6);
        assert_eq!(text("{fixed:\"abc\"|check_tail:2}").len(), 5);
        assert_eq!(
            text("{fixed:\"abc\"|check_tail:2}"),
            text("{fixed:\"abc\"|check_tail:2}")
        );
        let e = text("{fixed:\"abc\"|ensure:upper+digit+symbol}");
        assert!(e.chars().any(|c| c.is_ascii_uppercase()) && e.chars().any(|c| c.is_ascii_digit()));
        let all = text("{fixed:\"abc\"|class_fill}");
        assert!(
            all.chars().any(|c| c.is_ascii_uppercase())
                && all.chars().any(|c| !c.is_ascii_alphanumeric())
        );
        assert!(run("{fixed:\"abc\"|ensure:blue}").is_err());
        let keyed = text("{fixed:\"abcdefgh\"|keyed_case}");
        assert_eq!(keyed.to_lowercase(), "abcdefgh");
        assert_eq!(keyed, text("{fixed:\"abcdefgh\"|keyed_case}"));
    }

    #[test]
    fn a_slice_keeps_its_share_of_the_bits() {
        let g = run("{random:20:base36|first:10}").unwrap();
        assert!((g.bits - 10.0 * 36f64.log2()).abs() < 1e-9);
    }

    #[test]
    fn lengtheners_add_length_and_no_bits_and_never_change_between_versions() {
        // Golden vectors for `v1`: computed outside this file from the stated construction (HKDF-SHA256,
        // salt SHA-256("unv/recipe/expand/v1"), info "v1|" + context, 16-bit draws with rejection). Never regenerated.
        let e =
            |ctx: &str| derive_chars("abcdefgh", ctx, &alphabet("base36").unwrap(), 22).unwrap();
        assert_eq!(e(""), "3v68o4p9q8zt8jt1u8dk2j");
        assert_eq!(e("sonarr.home.lan"), "jnqpoppoozsmowaa0x9xdj");
        assert_eq!(
            derive_chars("abcdefgh", "", &alphabet("base36").unwrap(), 10).unwrap(),
            "3v68o4p9q8"
        );
        let g = run("{fixed:\"abcdefgh\"|expand:22:base36}").unwrap();
        assert_eq!(g.text, "3v68o4p9q8zt8jt1u8dk2j");
        assert_eq!(g.bits, 0.0);
        // A random part keeps the bits it was drawn with, however long the expansion.
        let r = run("{random:8:base36|expand:40:base36}").unwrap();
        assert_eq!(r.text.len(), 40);
        assert!((r.bits - 8.0 * 36f64.log2()).abs() < 1e-9);
        assert_eq!(r.parts[0].kind, "derive");
        // The context decides what is shared.
        let host = |h: &str, f: &str| {
            let mut c = ctx();
            c.host = h.into();
            c.family = f.into();
            make(
                "{fixed:\"abcdefgh\"|site_bound:12}|{fixed:\"abcdefgh\"|family_bound:12}",
                &open(),
                &c,
                &mut SeqDraw(1),
            )
            .unwrap()
            .text
        };
        let (a, b) = (host("a.example.org", "meta"), host("b.example.org", "meta"));
        let (a1, a2) = a.split_once('|').unwrap();
        let (b1, b2) = b.split_once('|').unwrap();
        assert_ne!(a1, b1, "a different host gives a different result");
        assert_eq!(a2, b2, "one family shares one result");
        assert_ne!(text("{fixed:\"abcdefgh\"|rotate_by_counter:12}"), {
            let mut c = ctx();
            c.counter = 4;
            make(
                "{fixed:\"abcdefgh\"|rotate_by_counter:12}",
                &open(),
                &c,
                &mut SeqDraw(1),
            )
            .unwrap()
            .text
        });
        let words = text("{fixed:\"abcdefgh\"|expand_words:3}");
        assert_eq!(words.split('-').count(), 3);
        assert_eq!(words, text("{fixed:\"abcdefgh\"|expand_words:3}"));
        let groups = text("{fixed:\"abcdefgh\"|expand_groups:2:5:hex}");
        assert_eq!(groups.len(), 11);
        assert!(groups.chars().nth(5) == Some('-'));
        for bad in [
            "{fixed:\"a\"|expand:5:base36:nowhere}",
            "{fixed:\"a\"|expand:5:base36:family}",
            "{fixed:\"a\"|expand:5:x}",
        ] {
            let mut c = Ctx::default();
            c.family.clear();
            assert!(make(bad, &open(), &c, &mut SeqDraw(1)).is_err(), "{bad}");
        }
        for ctxname in ["entry", "provider", "counter"] {
            run(&format!("{{fixed:\"a\"|expand:6:base36:{ctxname}}}")).unwrap();
        }
    }

    #[test]
    fn the_floor_refuses_a_password_that_is_too_weak() {
        let strict = Options::default();
        let weak = make("{prefix}-{digits:3}", &strict, &ctx(), &mut SeqDraw(1)).unwrap_err();
        assert!(weak.contains("below the floor of 64"), "{weak}");
        assert!(make("{random:16:safe}", &strict, &ctx(), &mut SeqDraw(1)).is_ok());
        let low = Options {
            floor_bits: Some(8.0),
            ..Options::default()
        };
        assert!(make("{digits:3}", &low, &ctx(), &mut SeqDraw(1)).is_ok());
    }

    #[test]
    fn options_are_met_by_drawing_again_and_unmeetable_ones_say_so() {
        let o = Options {
            floor_bits: Some(0.0),
            require_upper: 2,
            require_digit: 2,
            require_symbol: 1,
            avoid_ambiguous: true,
            forbid: "x".into(),
            max_run: 2,
            no_sequences: true,
            start_letter: true,
            no_edge_symbol: true,
            ..Options::default()
        };
        for seed in 1..40 {
            let g = make(
                "{random:14:safe}{symbols:1}{random:3:safe}",
                &o,
                &ctx(),
                &mut SeqDraw(seed),
            );
            let g = g.unwrap();
            assert!(g.text.chars().next().unwrap().is_alphabetic());
            assert!(g.text.chars().filter(|c| c.is_ascii_uppercase()).count() >= 2);
            assert!(!g.text.contains(['x', '0', 'O', '1', 'l', 'I']));
        }
        let impossible = Options {
            floor_bits: Some(0.0),
            require_digit: 5,
            ..Options::default()
        };
        let e = make("{letters:6}", &impossible, &ctx(), &mut SeqDraw(1)).unwrap_err();
        assert!(e.contains("too few digits"), "{e}");
        let only = Options {
            floor_bits: Some(0.0),
            allow: "ab".into(),
            ..Options::default()
        };
        assert!(make("{fixed:\"abc\"}", &only, &ctx(), &mut SeqDraw(1)).is_err());
        assert!(make(
            "{fixed:\"aab\"}",
            &Options {
                floor_bits: Some(0.0),
                allow: "ab".into(),
                ..Options::default()
            },
            &ctx(),
            &mut SeqDraw(1)
        )
        .is_ok());
    }

    #[test]
    fn length_is_fitted_by_dropping_the_marked_parts_then_refused_if_still_long() {
        let o = Options {
            floor_bits: Some(40.0),
            max_len: Some(16),
            ..Options::default()
        };
        let t = "{random:14:base62}{suffix?1}{counter?2}{divider?3}";
        let g = make(t, &o, &ctx(), &mut SeqDraw(3)).unwrap();
        assert!(g.text.chars().count() <= 16);
        assert!(g.dropped.contains(&"divider".to_string()));
        let e = make("{random:20:base62}", &o, &ctx(), &mut SeqDraw(3)).unwrap_err();
        assert!(e.contains("no droppable part"), "{e}");
        // A hostile maximum cannot steer the result below the floor.
        let hostile = Options {
            floor_bits: Some(64.0),
            max_len: Some(6),
            ..Options::default()
        };
        let e = make(
            "{random:16:base62}{suffix?1}",
            &hostile,
            &ctx(),
            &mut SeqDraw(3),
        )
        .unwrap_err();
        assert!(e.contains("cannot fit"), "{e}");
        let refuse = Options {
            floor_bits: Some(0.0),
            max_len: Some(3),
            policy: "refuse".into(),
            ..Options::default()
        };
        assert!(
            make("{random:9}{suffix?1}", &refuse, &ctx(), &mut SeqDraw(3))
                .unwrap_err()
                .contains("refuse")
        );
        let nothing_left = Options {
            floor_bits: Some(0.0),
            max_len: Some(2),
            ..Options::default()
        };
        assert!(
            make("abcdefgh{suffix?1}", &nothing_left, &ctx(), &mut SeqDraw(3))
                .unwrap_err()
                .contains("no droppable part")
        );
    }

    #[test]
    fn a_short_result_is_padded_or_refused_and_the_padding_counts_its_bits() {
        let o = Options {
            floor_bits: Some(0.0),
            min_len: Some(12),
            ..Options::default()
        };
        let g = make("{fixed:\"abc\"}", &o, &ctx(), &mut SeqDraw(1)).unwrap();
        assert_eq!(g.text.chars().count(), 12);
        assert!(g.bits > 40.0 && g.warnings[0].contains("padded"));
        let refuse = Options {
            policy: "refuse".into(),
            ..o
        };
        assert!(make("{fixed:\"abc\"}", &refuse, &ctx(), &mut SeqDraw(1))
            .unwrap_err()
            .contains("below the minimum"));
    }

    #[test]
    fn chunking_and_the_reuse_check_and_memory_rebuildability() {
        let o = Options {
            floor_bits: Some(0.0),
            chunk: 4,
            chunk_sep: "_".into(),
            ..Options::default()
        };
        let g = make("{random:8:hex}", &o, &ctx(), &mut SeqDraw(1)).unwrap();
        assert_eq!(g.text.matches('_').count(), 1);
        let dash = Options {
            floor_bits: Some(0.0),
            chunk: 2,
            ..Options::default()
        };
        assert_eq!(
            make("{fixed:\"abcd\"}", &dash, &ctx(), &mut SeqDraw(1))
                .unwrap()
                .text,
            "ab-cd"
        );
        let mut c = ctx();
        let first = make("{digits:1}", &open(), &c, &mut SeqDraw(1))
            .unwrap()
            .text;
        c.avoid = vec![first.clone()];
        for seed in 1..20 {
            assert_ne!(
                make("{digits:1}", &open(), &c, &mut SeqDraw(seed))
                    .unwrap()
                    .text,
                first
            );
        }
        c.avoid = (0..10).map(|d| d.to_string()).collect();
        assert!(make("{digits:1}", &open(), &c, &mut SeqDraw(1))
            .unwrap_err()
            .contains("already used"));
        assert!(
            run("{provider}{created:YYYY}")
                .unwrap()
                .rebuildable_from_memory
        );
        assert!(!run("{provider}{random:4}").unwrap().rebuildable_from_memory);
        assert!(
            !run("{fixed:\"a\"|expand:6}")
                .unwrap()
                .rebuildable_from_memory
        );
        assert!(
            analyse("{provider}-{random:10:base62}", &Options::default(), &ctx())
                .unwrap()
                .bits
                > 50.0
        );
        assert!(analyse("{nope}", &Options::default(), &ctx()).is_err());
    }

    #[test]
    fn the_generated_part_table_never_carries_a_value() {
        let g = run("{prefix}{random:12:base62}").unwrap();
        let shown = serde_json::to_string(&g.parts).unwrap();
        assert!(!shown.contains(&g.text));
        assert_eq!(g.parts.len(), 2);
        assert_eq!(g.parts[0].kind, "list");
    }

    #[test]
    fn draws_are_uniform_enough_and_failures_surface() {
        let mut d = SeqDraw(9);
        let mut hits = [0u32; 6];
        for _ in 0..6000 {
            hits[d.below(6) as usize] += 1;
        }
        assert!(hits.iter().all(|h| (800..1200).contains(h)), "{hits:?}");
        struct Failing;
        impl Draw for Failing {
            fn next_u64(&mut self) -> u64 {
                0
            }
            fn failure(&self) -> Option<String> {
                Some("device failed".into())
            }
        }
        assert_eq!(
            make("{random:4}", &open(), &ctx(), &mut Failing).unwrap_err(),
            "device failed"
        );
        let mut sys = SysDraw;
        assert_ne!(sys.next_u64(), sys.next_u64());
        let mut pool = PoolDraw::new(crate::entropy::Source::Os);
        assert_ne!(pool.next_u64(), pool.next_u64());
        assert!(pool.failure().is_none());
    }

    #[test]
    fn options_read_from_json_and_reject_nonsense() {
        let o = Options::from_value(
            &serde_json::json!({ "max_len": 12, "require_digit": 2, "floor_bits": 32.0 }),
        )
        .unwrap();
        assert_eq!((o.max_len, o.require_digit, o.floor()), (Some(12), 2, 32.0));
        assert_eq!(
            Options::from_value(&Value::Null).unwrap().floor(),
            DEFAULT_FLOOR_BITS
        );
        assert!(Options::from_value(&serde_json::json!({ "max_len": "long" })).is_err());
        assert!(builtin_lists().contains_key("prefix"));
    }

    #[test]
    fn the_syntax_helpers_handle_quotes_and_escapes() {
        assert_eq!(split_top("a:\"b:c\":d", ':'), ["a", "\"b:c\"", "d"]);
        assert_eq!(parse_arg("\"a\\\"b\"").text, "a\"b");
        assert!(parse_arg("$pet").text.starts_with('$') && !parse_arg("$pet").quoted);
        assert!(parse_arg("\"$pet\"").quoted);
        let t = parse("{suffix?2}{random:4?1}x").unwrap();
        assert_eq!(t.segs.len(), 3);
        assert!(matches!(&t.segs[0], Seg::Tok(k) if k.droppable == Some(2)));
        assert_eq!(format_date("Y D", "2026-11-15").unwrap(), "Y 15");
        assert_eq!(weekday(2026, 11, 15), 0);
    }

    fn vault() -> Value {
        serde_json::json!({
            "projects": [{ "id": "p1", "name": "Social" }],
            "api_keys": []
        })
    }

    fn entry() -> Value {
        serde_json::json!({
            "id": "e1", "provider": "Instagram", "secretType": "password", "api_key": "old-1",
            "username": "joy", "email": "joy@example.org", "created_at": "2018-11-11T10:00:00Z",
            "projectIds": ["Universal", "p1"], "tags": ["social"], "site_family": "meta",
            "sites": [{ "url": "https://www.instagram.com", "kind": "web" }],
            "version_history": [{ "api_key": "old-0" }],
            "generated_by": { "recipe": "x", "counter": 4 }
        })
    }

    #[test]
    fn recipes_are_saved_replaced_and_removed_with_their_rules() {
        let mut v = vault();
        save(&mut v, "social", "{random:16}", &Options::default(), false).unwrap();
        assert!(
            save(&mut v, "social", "{random:20}", &Options::default(), false)
                .unwrap_err()
                .contains("--replace")
        );
        save(
            &mut v,
            "social",
            "{random:20}",
            &Options {
                max_len: Some(30),
                ..Options::default()
            },
            true,
        )
        .unwrap();
        assert!(save(&mut v, "bad", "{nope}", &Options::default(), false).is_err());
        assert!(save(&mut v, "", "{random:4}", &Options::default(), false).is_err());
        let all = saved(&v);
        assert_eq!(
            (all.len(), all[0].template.as_str(), all[0].options.max_len),
            (1, "{random:20}", Some(30))
        );
        add_rule(&mut v, "provider", "Instagram", "social").unwrap();
        v["recipe_default"] = serde_json::json!("social");
        assert!(add_rule(&mut v, "colour", "x", "social").is_err());
        assert!(add_rule(&mut v, "provider", " ", "social").is_err());
        assert!(add_rule(&mut v, "provider", "x", "ghost").is_err());
        assert!(remove(&mut v, "social"));
        assert!(!remove(&mut v, "social"));
        assert!(rules(&v).is_empty() && v.get("recipe_default").is_none() && saved(&v).is_empty());
    }

    #[test]
    fn lists_and_facts_are_validated_and_replaced_by_name() {
        let mut v = vault();
        assert_eq!(
            save_list(
                &mut v,
                "pets",
                &["rex".into(), " rex ".into(), "tom".into(), "".into()],
                true
            )
            .unwrap(),
            2
        );
        assert!(save_list(&mut v, "pets", &["only".into()], false).is_err());
        assert!(save_list(&mut v, "bad name", &["a".into(), "b".into()], false).is_err());
        save_list(&mut v, "pets", &["a".into(), "b".into(), "c".into()], false).unwrap();
        let l = custom_lists(&v);
        assert_eq!((l.len(), l[0].1.len(), l[0].2), (1, 3, false));
        assert!(remove_list(&mut v, "pets") && !remove_list(&mut v, "pets"));
        assert!(!remove_list(&mut serde_json::json!({}), "x"));
        set_fact(&mut v, "pet", "rex", true).unwrap();
        set_fact(&mut v, "pet", "tom", false).unwrap();
        assert_eq!(
            facts(&v),
            vec![("pet".to_string(), "tom".to_string(), false)]
        );
        assert!(set_fact(&mut v, "Pet", "x", true).is_err());
        assert!(set_fact(&mut v, "pet", "", true).is_err());
        assert!(remove_fact(&mut v, "pet") && !remove_fact(&mut v, "pet"));
        assert!(!remove_fact(&mut serde_json::json!({}), "x"));
    }

    #[test]
    fn the_entrys_own_recipe_beats_a_rule_which_beats_the_vault_default() {
        let mut v = vault();
        for n in ["own", "ruled", "default"] {
            save(&mut v, n, "{random:16}", &Options::default(), false).unwrap();
        }
        v["recipe_default"] = serde_json::json!("default");
        let mut e = entry();
        assert_eq!(resolve(&v, &e).unwrap().1, "vault default");
        add_rule(&mut v, "type", "ssh_key", "none").unwrap();
        add_rule(&mut v, "host", "instagram.com", "ruled").unwrap();
        assert_eq!(resolve(&v, &e).unwrap().0.name, "ruled");
        e["recipe"] = serde_json::json!("own");
        assert_eq!(resolve(&v, &e).unwrap().1, "this entry");
        e["recipe"] = serde_json::json!("none");
        assert!(resolve(&v, &e).is_none());
        e["recipe"] = serde_json::json!("ghost");
        assert!(resolve(&v, &e).is_none());
        // Every rule kind matches what it says; `none` switches recipes off for a match.
        e.as_object_mut().unwrap().remove("recipe");
        for (kind, value) in [
            ("provider", "instagram"),
            ("family", "meta"),
            ("tag", "social"),
            ("project", "Social"),
            ("project", "p1"),
            ("type", "password"),
        ] {
            let mut w = v.clone();
            w["recipe_rules"] = serde_json::json!([]);
            add_rule(&mut w, kind, value, "ruled").unwrap();
            assert_eq!(resolve(&w, &e).unwrap().1, format!("rule: {kind} {value}"));
        }
        let mut w = v.clone();
        w["recipe_rules"] = serde_json::json!([]);
        add_rule(&mut w, "provider", "Instagram", "none").unwrap();
        assert!(resolve(&w, &e).is_none());
        add_rule(&mut w, "host", "other.org", "ruled").unwrap();
        assert!(!rule_matches("host", "other.org", &e, &w) && !rule_matches("bogus", "x", &e, &w));
        assert!(
            remove_rule(&mut v, 1)
                && !remove_rule(&mut v, 9)
                && !remove_rule(&mut serde_json::json!({}), 1)
        );
    }

    #[test]
    fn the_context_is_built_from_the_entry_the_facts_and_the_lists() {
        let mut v = vault();
        set_fact(&mut v, "pet", "rex", true).unwrap();
        save_list(&mut v, "food", &["kimchi".into(), "naan".into()], true).unwrap();
        let c = context(&v, Some(&entry()), "2026-11-15");
        assert_eq!(
            (
                c.provider.as_str(),
                c.host.as_str(),
                c.user.as_str(),
                c.project.as_str()
            ),
            ("Instagram", "www.instagram.com", "joy", "Social")
        );
        assert_eq!(
            (
                c.created.as_str(),
                c.counter,
                c.family.as_str(),
                c.secret_type.as_str()
            ),
            ("2018-11-11", 5, "meta", "password")
        );
        assert_eq!(c.avoid, ["old-1", "old-0"]);
        assert_eq!(c.vars["pet"], "rex");
        assert!(c.private_lists.contains("food") && c.lists["food"].len() == 2);
        assert_eq!(c.url, "https://www.instagram.com");
        let bare = context(&v, None, "2026-11-15");
        assert!(bare.provider.is_empty() && bare.counter == 1 && bare.now == "2026-11-15");
        // An entry with only the older `api_url` still has a host and an address.
        let old = context(
            &v,
            Some(&serde_json::json!({ "api_url": "https://api.x.test", "secretType": "" })),
            "2026-01-01",
        );
        assert_eq!(
            (
                old.host.as_str(),
                old.url.as_str(),
                old.secret_type.as_str()
            ),
            ("api.x.test", "https://api.x.test", "api_key")
        );
        let g = generated_by("social", 5, "2026-11-15");
        assert_eq!(
            (
                g["recipe"].as_str(),
                g["counter"].as_u64(),
                g["version"].as_u64()
            ),
            (Some("social"), Some(5), Some(1))
        );
    }
}
