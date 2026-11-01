//! Import of a DE GDPR personal-data trade export into ledger rows. Port of WFHelper's gdprImport.
//! No confirmed sample exists, so headers are matched by meaning and anything unreadable becomes a
//! rejected preview row instead of failing. The catalog and the ledger sit behind traits (no
//! ledger until Phase 4).
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use chrono::{DateTime, Local, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha1::{Digest, Sha1};

const MAX_ROWS: usize = 50_000;
const MAX_PREVIEW_ROWS: usize = 100;
const MAX_ITEMS_PER_TRADE: usize = 12;
const MAX_STAGED_BATCHES: usize = 3;
const MAX_NAME_LENGTH: usize = 160;
const MAX_IMPORT_FILE_BYTES: u64 = 50 * 1024 * 1024;
const TRADE_EVENT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TradeType {
    Sale,
    Purchase,
    Trade,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Given,
    Received,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum EventSource {
    Live,
    Gdpr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TradeItem {
    pub internal_name: String,
    pub display_name: String,
    pub count: u32,
    pub direction: Direction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wfm_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wfm_thumb: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TradeEvent {
    pub id: String,
    pub date: String,
    #[serde(rename = "type")]
    pub kind: TradeType,
    #[specta(type = f64)]
    pub plat_change: i64,
    pub items: Vec<TradeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partner: Option<String>,
    pub schema_version: Option<u32>,
    pub source: Option<EventSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_batch_id: Option<String>,
    /// Set once the matching warframe.market listing was closed from this row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wfm_closed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<f64>)]
    pub credits: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<f64>)]
    pub trade_tax: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    Parsed,
    Duplicate,
    Unresolved,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RowPreview {
    pub kind: RowKind,
    pub date: String,
    pub summary: String,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
pub struct Counts {
    pub parsed: u32,
    pub duplicates: u32,
    pub unresolved: u32,
    pub rejected: u32,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub batch_id: String,
    pub file_name: String,
    pub counts: Counts,
    /// Display sample, capped; `counts` carries the full numbers.
    pub rows: Vec<RowPreview>,
}

/// Stable failure ids; the frontend owns the wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LedgerErrorCode {
    FileTooLarge,
    ImportReadFailed,
    NoRows,
    BatchGone,
    ImportWriteFailed,
    RowGone,
    InvalidPatch,
    SaveFailed,
    LedgerReadFailed,
}

pub struct CatalogItem {
    pub game_ref: Option<String>,
    pub url_name: Option<String>,
    pub thumb: Option<String>,
}

/// Name lookup into the warframe.market catalog (not ported yet).
pub trait ItemCatalog {
    fn lookup(&self, name: &str) -> Option<CatalogItem>;
}

pub struct NoCatalog;
impl ItemCatalog for NoCatalog {
    fn lookup(&self, _: &str) -> Option<CatalogItem> {
        None
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub applied: u32,
    pub skipped_duplicates: u32,
}

/// Where staged rows end up (`services::ledger::SqlSink`).
pub trait LedgerSink {
    fn add_events(&self, events: Vec<TradeEvent>) -> Result<ApplyOutcome, LedgerErrorCode>;
}

type Record = Map<String, Value>;

fn re(p: &str) -> Regex {
    Regex::new(p).unwrap()
}

fn to_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    }
}

fn norm_key(k: &str) -> String {
    k.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}

fn canonicalize(rec: &Record, occurrence: usize) -> String {
    let mut pairs: Vec<String> = rec
        .iter()
        .map(|(k, v)| format!("{}={}", norm_key(k), to_text(v)))
        .collect();
    pairs.sort();
    // The ordinal keeps identical source rows distinct while a re-import reproduces the ids.
    format!("gdpr-v1{}#{occurrence}", pairs.concat())
}

fn sha1_hex(text: &str) -> String {
    Sha1::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn split_csv_line(line: &str, delim: char) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let (mut cells, mut cell, mut quoted, mut i) = (Vec::new(), String::new(), false, 0);
    while i < chars.len() {
        let ch = chars[i];
        if quoted {
            if ch == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    cell.push('"');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                cell.push(ch);
            }
        } else if ch == '"' {
            quoted = true;
        } else if ch == delim {
            cells.push(std::mem::take(&mut cell));
        } else {
            cell.push(ch);
        }
        i += 1;
    }
    cells.push(cell);
    cells.into_iter().map(|c| c.trim().to_string()).collect()
}

/// Split on newlines that are not inside a quoted cell.
fn split_csv_rows(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let (mut rows, mut cur, mut quoted, mut i) = (Vec::new(), String::new(), false, 0);
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            quoted = !quoted;
        } else if !quoted && (ch == '\n' || ch == '\r') {
            if ch == '\r' && chars.get(i + 1) == Some(&'\n') {
                i += 1;
            }
            rows.push(std::mem::take(&mut cur));
            i += 1;
            continue;
        }
        cur.push(ch);
        i += 1;
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows.retain(|r| !r.trim().is_empty());
    rows
}

fn detect_delimiter(header: &str) -> char {
    let (mut best, mut best_count) = (',', 0);
    for c in [',', ';', '\t'] {
        let n = split_csv_line(header, c).len();
        if n > best_count {
            (best, best_count) = (c, n);
        }
    }
    best
}

fn parse_csv_records(text: &str) -> Vec<Record> {
    let rows = split_csv_rows(text);
    if rows.len() < 2 {
        return vec![];
    }
    let delim = detect_delimiter(&rows[0]);
    let headers: Vec<String> = split_csv_line(&rows[0], delim)
        .into_iter()
        .enumerate()
        .map(|(i, h)| {
            if h.is_empty() {
                format!("column{i}")
            } else {
                h
            }
        })
        .collect();
    rows[1..]
        .iter()
        .take(MAX_ROWS)
        .map(|row| {
            let cells = split_csv_line(row, delim);
            let mut rec = Record::new();
            for (i, h) in headers.iter().enumerate() {
                let cell = cells.get(i).cloned().unwrap_or_default();
                rec.insert(h.clone(), Value::String(cell));
            }
            rec
        })
        .collect()
}

fn as_records(v: &Value) -> Option<Vec<Record>> {
    let arr = v.as_array().filter(|a| !a.is_empty())?;
    arr.iter()
        .take(MAX_ROWS)
        .map(|e| e.as_object().cloned())
        .collect()
}

/// Take the root array, or the most trade-like array inside the root object.
fn parse_json_records(text: &str) -> Result<Vec<Record>, serde_json::Error> {
    let parsed: Value = serde_json::from_str(text)?;
    if let Some(r) = as_records(&parsed) {
        return Ok(r);
    }
    let Some(root) = parsed.as_object() else {
        return Ok(vec![]);
    };
    static PREFERRED: LazyLock<Regex> =
        LazyLock::new(|| re(r"(?i)trade|transaction|record|histor|purchase|sale"));
    let keys: Vec<&String> = root.keys().collect();
    let ordered = keys
        .iter()
        .filter(|k| PREFERRED.is_match(k))
        .chain(keys.iter());
    for key in ordered {
        let value = &root[key.as_str()];
        if let Some(r) = as_records(value) {
            return Ok(r);
        }
        if let Some(obj) = value.as_object()
            && let Some(r) = obj.values().find_map(as_records)
        {
            return Ok(r);
        }
    }
    Ok(vec![])
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Field {
    Date,
    Type,
    ItemsGiven,
    ItemsReceived,
    Items,
    Platinum,
    Credits,
    Tax,
    Quantity,
    Partner,
}

// Tier 0 is an exact header name, tier 1 a distinctive substring, tier 2 a loose guess.
// Every field competes in one tier before the next tier opens.
static FIELD_PATTERNS: LazyLock<Vec<(Field, [Regex; 3])>> = LazyLock::new(|| {
    let p = |f, a: &str, b: &str, c: &str| (f, [re(a), re(b), re(c)]);
    vec![
        p(
            Field::Date,
            r"^(date|time|datetime|timestamp|tradedate|tradetime|createdat|occurredat|when)$",
            r"(datetime|timestamp|tradedate|tradetime|createdat|occurredat)",
            r"(date|time)",
        ),
        p(
            Field::Type,
            r"^(type|kind|action|direction|tradetype|transactiontype)$",
            r"(tradetype|transactiontype|direction)",
            r"(type|action|kind)",
        ),
        p(
            Field::ItemsGiven,
            r"^(itemsgiven|givenitems|itemssold|solditems|outgoing|offered)$",
            r"(itemsgiven|givenitems|itemssold|outgoingitems|offereditems)",
            r"(given|sold|outgoing|offered)",
        ),
        p(
            Field::ItemsReceived,
            r"^(itemsreceived|receiveditems|itemsbought|boughtitems|incoming|requested)$",
            r"(itemsreceived|receiveditems|itemsbought|incomingitems|requesteditems)",
            r"(received|bought|incoming|requested)",
        ),
        p(
            Field::Items,
            r"^(item|items|itemname|itemnames|product|goods|content)$",
            r"(itemname|itemlist)",
            r"item",
        ),
        p(
            Field::Platinum,
            r"^(platinum|plat|premiumcredits|price|cost)$",
            r"(platinum|premiumcredits|platprice|platamount)",
            r"(plat|price|cost|amount|value)",
        ),
        p(
            Field::Credits,
            r"^(credits|regularcredits)$",
            r"(regularcredits|creditamount)",
            r"credit",
        ),
        p(
            Field::Tax,
            r"^(tax|tradetax|taxpaid)$",
            r"(tradetax|taxpaid|taxamount)",
            r"tax",
        ),
        p(
            Field::Quantity,
            r"^(quantity|qty|count)$",
            r"(itemquantity|itemcount)",
            r"(quantity|qty)",
        ),
        p(
            Field::Partner,
            r"^(partner|tradepartner|counterparty|otherplayer|withplayer|buyer|seller|recipient)$",
            r"(tradepartner|counterparty|otherplayer|withplayer|partnername)",
            r"(partner|buyer|seller|recipient|player|username|account)",
        ),
    ]
});

type FieldMap = HashMap<Field, String>;

/// Map source keys onto ledger fields; each key serves at most one field.
fn map_fields(keys: &[String]) -> FieldMap {
    let norm: Vec<(&String, String)> = keys.iter().map(|k| (k, norm_key(k))).collect();
    let (mut used, mut mapped) = (HashSet::new(), FieldMap::new());
    for tier in 0..3 {
        for (field, pats) in FIELD_PATTERNS.iter() {
            if mapped.contains_key(field) {
                continue;
            }
            if let Some((k, _)) = norm
                .iter()
                .find(|(k, n)| !used.contains(*k) && pats[tier].is_match(n))
            {
                mapped.insert(*field, (*k).clone());
                used.insert(*k);
            }
        }
    }
    mapped
}

impl Field {
    fn get<'a>(self, rec: &'a Record, fields: &FieldMap) -> Option<&'a Value> {
        rec.get(fields.get(&self)?)
    }
}

/// An export prints wall-clock dates, so a bare date or time is read as local: a UTC reading
/// would land the row on the wrong calendar day west of Greenwich.
fn local_iso(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> Option<String> {
    let t = Local.with_ymd_and_hms(y, m, d, h, min, s).earliest()?;
    Some(iso(t.with_timezone(&Utc)))
}

fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn from_epoch(v: f64) -> Option<String> {
    let ms = if v > 1e12 {
        v
    } else if v > 1e9 {
        v * 1000.0
    } else {
        return None;
    };
    DateTime::from_timestamp_millis(ms as i64).map(iso)
}

/// ISO datetime from the shapes an export plausibly ships.
// shortcut: free-form dates beyond ISO-8601 are rejected (JS Date.parse is looser), widen if a real export needs it.
pub fn parse_gdpr_date(value: &Value) -> Option<String> {
    if let Some(n) = value.as_f64() {
        return from_epoch(n);
    }
    let text = to_text(value);
    if text.is_empty() {
        return None;
    }
    static EPOCH: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{10}|\d{13})$"));
    static DOTTED: LazyLock<Regex> = LazyLock::new(|| {
        re(r"^(\d{1,2})\.(\d{1,2})\.(\d{4})(?:[ T](\d{1,2}):(\d{2})(?::(\d{2}))?)?$")
    });
    static SLASHED: LazyLock<Regex> = LazyLock::new(|| {
        re(r"^(\d{1,2})/(\d{1,2})/(\d{4})(?:[ T](\d{1,2}):(\d{2})(?::(\d{2}))?)?$")
    });
    static ISO_DAY: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d{4})-(\d{2})-(\d{2})$"));
    static ISO_START: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{4}-\d{2}-\d{2}[ T]"));

    if EPOCH.is_match(&text) {
        return from_epoch(text.parse().ok()?);
    }
    let n =
        |c: &regex::Captures, i| -> u32 { c.get(i).map_or(0, |m| m.as_str().parse().unwrap_or(0)) };
    if let Some(c) = DOTTED.captures(&text) {
        return local_iso(
            n(&c, 3) as i32,
            n(&c, 2),
            n(&c, 1),
            n(&c, 4),
            n(&c, 5),
            n(&c, 6),
        );
    }
    if let Some(c) = SLASHED.captures(&text) {
        // Ambiguous unless the first field cannot be a month; default to US order.
        let day_first = n(&c, 1) > 12;
        let (month, day) = if day_first {
            (n(&c, 2), n(&c, 1))
        } else {
            (n(&c, 1), n(&c, 2))
        };
        return local_iso(n(&c, 3) as i32, month, day, n(&c, 4), n(&c, 5), n(&c, 6));
    }
    if let Some(c) = ISO_DAY.captures(&text) {
        return local_iso(n(&c, 1) as i32, n(&c, 2), n(&c, 3), 0, 0, 0);
    }
    let normalized = if ISO_START.is_match(&text) {
        text.replacen(' ', "T", 1)
    } else {
        text
    };
    if let Ok(t) = DateTime::parse_from_rfc3339(&normalized) {
        return Some(iso(t.with_timezone(&Utc)));
    }
    // A datetime with no offset is local.
    ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(&normalized, f).ok())
        .and_then(|t| Local.from_local_datetime(&t).earliest())
        .map(|t| iso(t.with_timezone(&Utc)))
}

/// `^[+-]?\d{1,3}(?:([.,])\d{3})(?:\1\d{3})*$`, by hand: the regex crate has no backreferences.
fn grouping_separator(s: &str) -> Option<char> {
    let b = s.strip_prefix(['+', '-']).unwrap_or(s);
    let head = b.find(['.', ','])?;
    let sep = b[head..].chars().next()?;
    if !(1..=3).contains(&head) || !b[..head].bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let groups: Vec<&str> = b[head..].split(sep).skip(1).collect();
    let ok = b[head..].starts_with(sep)
        && groups
            .iter()
            .all(|g| g.len() == 3 && g.bytes().all(|c| c.is_ascii_digit()));
    ok.then_some(sep)
}

/// The number style is inferred per value like the date style: only groups of exactly three
/// digits read as thousands, and when both separators appear the rightmost is the decimal point.
fn normalize_separators(text: &str) -> String {
    if let Some(sep) = grouping_separator(text) {
        return text.replace(sep, "");
    }
    let commas = text.matches(',').count();
    let dots = text.matches('.').count();
    if commas > 0 && dots > 0 {
        let group = if text.rfind(',') > text.rfind('.') {
            '.'
        } else {
            ','
        };
        return text.replace(group, "").replacen(',', ".", 1);
    }
    // A repeated separator cannot be the decimal point ("1,234,56" is 123456, not NaN).
    if commas > 1 {
        return text.replace(',', "");
    }
    if dots > 1 {
        return text.replace('.', "");
    }
    text.replacen(',', ".", 1)
}

fn parse_number(v: &Value) -> Option<f64> {
    if let Value::Number(n) = v {
        return n.as_f64().filter(|f| f.is_finite());
    }
    let text: String = to_text(v)
        .chars()
        .filter(|c| c.is_ascii_digit() || ".,+-".contains(*c))
        .collect();
    if text.is_empty() {
        return None;
    }
    normalize_separators(&text)
        .parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
}

fn normalize_trade_type(v: &Value) -> Option<TradeType> {
    static SALE: LazyLock<Regex> =
        LazyLock::new(|| re(r"(sale|sold|sell|selling|outgoing|gave|given)"));
    static BUY: LazyLock<Regex> =
        LazyLock::new(|| re(r"(purchase|purchased|bought|buy|buying|incoming|received)"));
    static SWAP: LazyLock<Regex> = LazyLock::new(|| re(r"(trade|swap|exchange|barter)"));
    let t = to_text(v).to_lowercase();
    if SALE.is_match(&t) {
        Some(TradeType::Sale)
    } else if BUY.is_match(&t) {
        Some(TradeType::Purchase)
    } else if SWAP.is_match(&t) {
        Some(TradeType::Trade)
    } else {
        None
    }
}

/// Strip platform UTF-8 glyphs and legacy Latin-1 mojibake.
fn strip_platform_glyphs(value: &str) -> String {
    static PUA: LazyLock<Regex> = LazyLock::new(|| re(r"[\x{E000}-\x{F8FF}\x{FFFD}]+"));
    static MOJIBAKE: LazyLock<Regex> = LazyLock::new(|| re(r"[\x{EE}\x{EF}][\x{80}-\x{BF}]{2}"));
    static TRAILING: LazyLock<Regex> = LazyLock::new(|| re(r"[\x{80}-\x{10FFFF}]+$"));
    static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s{2,}"));
    let s = PUA.replace_all(value, "");
    let s = MOJIBAKE.replace_all(&s, "");
    let s = TRAILING.replace(&s, "");
    SPACES.replace_all(&s, " ").trim().to_string()
}

fn parse_counted_name(raw: &str) -> Option<(String, u32)> {
    static PREFIX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(\d{1,4})\s*[x*]\s*(.+)$"));
    static SUFFIX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(.+?)\s*[x*]\s*(\d{1,4})$"));
    let text = strip_platform_glyphs(raw.trim());
    if text.is_empty() {
        return None;
    }
    if let Some(c) = PREFIX.captures(&text) {
        return Some((c[2].trim().to_string(), c[1].parse().ok()?));
    }
    if let Some(c) = SUFFIX.captures(&text) {
        return Some((c[1].trim().to_string(), c[2].parse().ok()?));
    }
    Some((text, 1))
}

/// Base name of a traded item: drops a trailing "(rank 5)"-style suffix.
fn traded_base_name(name: &str) -> String {
    static PAREN: LazyLock<Regex> = LazyLock::new(|| re(r"^(.*?)\s*\(([^()]*)\)\s*$"));
    let name = name.trim();
    match PAREN.captures(name) {
        Some(c) if !c[1].trim().is_empty() => c[1].trim().to_string(),
        _ => name.to_string(),
    }
}

fn lookup_traded(catalog: &dyn ItemCatalog, name: &str) -> Option<CatalogItem> {
    static BLUEPRINT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i) Blueprint$"));
    let base = traded_base_name(name);
    if base.is_empty() {
        return None;
    }
    catalog
        .lookup(&base)
        .or_else(|| catalog.lookup(&BLUEPRINT.replace(&base, "")))
}

fn item_from_name(
    raw: &str,
    direction: Direction,
    catalog: &dyn ItemCatalog,
) -> Option<(TradeItem, bool)> {
    let (name, count) = parse_counted_name(raw)?;
    if name.is_empty() || name.chars().count() > MAX_NAME_LENGTH || !(1..=9999).contains(&count) {
        return None;
    }
    let found = lookup_traded(catalog, &name);
    let resolved = found.is_some();
    let (game_ref, wfm_slug, wfm_thumb) = found
        .map(|c| (c.game_ref, c.url_name, c.thumb))
        .unwrap_or_default();
    let item = TradeItem {
        // game_ref is DE's uniqueName; the renderer joins the item database on it.
        internal_name: game_ref.unwrap_or_default(),
        display_name: name,
        count,
        direction,
        wfm_slug: wfm_slug.filter(|s| !s.is_empty()),
        wfm_thumb: wfm_thumb.filter(|s| !s.is_empty()),
    };
    Some((item, resolved))
}

fn extract_items(
    value: &Value,
    direction: Direction,
    catalog: &dyn ItemCatalog,
) -> (Vec<TradeItem>, u32) {
    static SPLIT: LazyLock<Regex> = LazyLock::new(|| re(r"[;|,]|\s\+\s"));
    fn visit(
        v: &Value,
        dir: Direction,
        cat: &dyn ItemCatalog,
        out: &mut Vec<TradeItem>,
        unresolved: &mut u32,
    ) {
        let mut push = |raw: &str| {
            if let Some((item, resolved)) = item_from_name(raw, dir, cat) {
                out.push(item);
                *unresolved += u32::from(!resolved);
            }
        };
        match v {
            Value::String(s) => SPLIT
                .split(s)
                .filter(|p| !p.trim().is_empty())
                .for_each(&mut push),
            Value::Array(a) => {
                for child in a {
                    visit(child, dir, cat, out, unresolved);
                }
            }
            Value::Object(o) => {
                let name = ["name", "itemName", "item", "displayName", "type"]
                    .iter()
                    .find_map(|k| o.get(*k).filter(|v| !v.is_null()))
                    .map(to_text)
                    .unwrap_or_default();
                if name.is_empty() {
                    return;
                }
                let count = ["quantity", "count", "amount"]
                    .iter()
                    .find_map(|k| o.get(*k).filter(|v| !v.is_null()))
                    .and_then(parse_number);
                match count {
                    Some(c) if c > 1.0 => push(&format!("{}x {name}", c.trunc() as i64)),
                    _ => push(&name),
                }
            }
            _ => {}
        }
    }
    let (mut items, mut unresolved) = (Vec::new(), 0);
    visit(value, direction, catalog, &mut items, &mut unresolved);
    items.truncate(MAX_ITEMS_PER_TRADE);
    (items, unresolved)
}

struct ParsedRow {
    event: Option<TradeEvent>,
    preview: RowPreview,
    unresolved: bool,
}

fn rejected(date: String, summary: String, reason: &str) -> ParsedRow {
    ParsedRow {
        event: None,
        unresolved: false,
        preview: RowPreview {
            kind: RowKind::Rejected,
            date,
            summary,
            reason: Some(reason.into()),
        },
    }
}

fn type_str(t: TradeType) -> &'static str {
    match t {
        TradeType::Sale => "sale",
        TradeType::Purchase => "purchase",
        TradeType::Trade => "trade",
    }
}

fn summarize(e: &TradeEvent) -> String {
    let names: Vec<&str> = e.items.iter().map(|i| i.display_name.as_str()).collect();
    let names = if names.is_empty() {
        String::new()
    } else {
        format!(" - {}", names.join(", "))
    };
    let partner = e
        .partner
        .as_ref()
        .map_or(String::new(), |p| format!(" with {p}"));
    format!("{} {}p{names}{partner}", type_str(e.kind), e.plat_change)
}

fn take_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn parse_row(
    rec: &Record,
    fields: &FieldMap,
    occurrence: usize,
    cat: &dyn ItemCatalog,
) -> ParsedRow {
    let source_record_id = sha1_hex(&canonicalize(rec, occurrence));
    let raw_date = Field::Date.get(rec, fields);
    let Some(date) = raw_date.and_then(parse_gdpr_date) else {
        let first = rec.values().next().map(to_text).unwrap_or_default();
        return rejected(
            take_chars(&raw_date.map(to_text).unwrap_or_default(), 40),
            take_chars(&first, 80),
            "no readable date",
        );
    };
    let day = date[..10].to_string();

    let plat_value = Field::Platinum.get(rec, fields);
    let plat_raw = plat_value.and_then(parse_number);
    if plat_raw.is_none() && plat_value.is_some_and(|v| !to_text(v).is_empty()) {
        return rejected(day, String::new(), "platinum is not a number");
    }

    // Signed platinum is the only other direction signal an export may carry.
    let kind = Field::Type
        .get(rec, fields)
        .and_then(normalize_trade_type)
        .unwrap_or(match plat_raw {
            Some(p) if p > 0.0 => TradeType::Sale,
            Some(p) if p < 0.0 => TradeType::Purchase,
            _ => TradeType::Trade,
        });
    let plat_change = plat_raw.map_or(0, |p| p.round().abs().min(10_000_000.0) as i64);

    let given_dir = if kind == TradeType::Purchase {
        Direction::Received
    } else {
        Direction::Given
    };
    let (mut items, mut unresolved_count) = (Vec::new(), 0);
    for (field, dir) in [
        (Field::ItemsGiven, Direction::Given),
        (Field::ItemsReceived, Direction::Received),
        (Field::Items, given_dir),
    ] {
        if let Some(v) = field.get(rec, fields) {
            let (found, unresolved) = extract_items(v, dir, cat);
            items.extend(found);
            unresolved_count += unresolved;
        }
    }
    items.truncate(MAX_ITEMS_PER_TRADE);
    if items.is_empty() && plat_change == 0 {
        return rejected(day, String::new(), "no items and no platinum");
    }

    let partner_raw = strip_platform_glyphs(
        &Field::Partner
            .get(rec, fields)
            .map(to_text)
            .unwrap_or_default(),
    );
    let partner =
        (!partner_raw.is_empty() && partner_raw.chars().count() <= 120).then_some(partner_raw);
    let money = |f: Field| f.get(rec, fields).and_then(parse_number);

    let event = TradeEvent {
        id: format!("gdpr-{source_record_id}"),
        date,
        kind,
        plat_change,
        items,
        partner,
        schema_version: Some(TRADE_EVENT_SCHEMA_VERSION),
        source: Some(EventSource::Gdpr),
        source_record_id: Some(source_record_id),
        import_batch_id: None,
        wfm_closed: None,
        credits: money(Field::Credits).map(|c| c.round() as i64),
        trade_tax: money(Field::Tax).map(|t| t.abs().round() as i64),
    };
    let unresolved = unresolved_count > 0;
    ParsedRow {
        preview: RowPreview {
            kind: if unresolved {
                RowKind::Unresolved
            } else {
                RowKind::Parsed
            },
            date: day,
            summary: summarize(&event),
            reason: unresolved
                .then(|| format!("{unresolved_count} item name(s) not in the catalog")),
        },
        event: Some(event),
        unresolved,
    }
}

/// Case- and glyph-insensitive text, so a console partner or a re-cased name still joins.
fn normalize_loose(s: &str) -> String {
    let s = strip_platform_glyphs(s.trim()).to_lowercase();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Live and imported rows describe direction from opposite ends of the deal, so only name and
/// count belong in the key.
fn live_match_key(e: &TradeEvent) -> String {
    let mut items: Vec<String> = e
        .items
        .iter()
        .map(|i| format!("{}:{}", normalize_loose(&i.display_name), i.count))
        .collect();
    items.sort();
    format!("{}|{}|{}", type_str(e.kind), e.plat_change, items.join("|"))
}

/// One day of slack absorbs a date-only export row against the live row's exact timestamp.
const LIVE_MATCH_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

struct LiveCandidate {
    at: i64,
    partner: String,
    used: bool,
}

fn millis(date: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(date)
        .ok()
        .map(|t| t.timestamp_millis())
}

fn index_live(existing: &[TradeEvent]) -> HashMap<String, Vec<LiveCandidate>> {
    let mut index: HashMap<String, Vec<LiveCandidate>> = HashMap::new();
    for e in existing {
        if e.source.unwrap_or(EventSource::Live) != EventSource::Live {
            continue;
        }
        let Some(at) = millis(&e.date) else { continue };
        let partner = normalize_loose(e.partner.as_deref().unwrap_or(""));
        index
            .entry(live_match_key(e))
            .or_default()
            .push(LiveCandidate {
                at,
                partner,
                used: false,
            });
    }
    index
}

/// One live row absorbs at most one imported row, so two genuine same-day trades both survive.
/// Partner is compared when both sides carry one; a GDPR export may not name the counterparty.
fn take_live_match(index: &mut HashMap<String, Vec<LiveCandidate>>, e: &TradeEvent) -> bool {
    let Some(bucket) = index.get_mut(&live_match_key(e)) else {
        return false;
    };
    let Some(at) = millis(&e.date) else {
        return false;
    };
    let partner = normalize_loose(e.partner.as_deref().unwrap_or(""));
    match bucket.iter_mut().find(|c| {
        !c.used
            && (c.at - at).abs() <= LIVE_MATCH_WINDOW_MS
            && (partner.is_empty() || c.partner.is_empty() || partner == c.partner)
    }) {
        Some(c) => {
            c.used = true;
            true
        }
        None => false,
    }
}

pub struct ParseResult {
    pub events: Vec<TradeEvent>,
    pub rows: Vec<RowPreview>,
    pub counts: Counts,
}

/// Parse export text into ledger rows, marking rows already in the ledger.
pub fn parse_gdpr_trade_export(
    text: &str,
    file_name: &str,
    existing: &[TradeEvent],
    catalog: &dyn ItemCatalog,
) -> ParseResult {
    static JSONISH: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*[\[{]"));
    let looks_json = file_name.to_lowercase().ends_with(".json") || JSONISH.is_match(text);
    let records = if looks_json {
        parse_json_records(text).unwrap_or_else(|_| parse_csv_records(text))
    } else {
        parse_csv_records(text)
    };

    let mut out = ParseResult {
        events: vec![],
        rows: vec![],
        counts: Counts::default(),
    };
    if records.is_empty() {
        return out;
    }
    let mut keys: Vec<String> = Vec::new();
    for rec in records.iter().take(200) {
        for k in rec.keys() {
            if !keys.contains(k) {
                keys.push(k.clone());
            }
        }
    }
    let fields = map_fields(&keys);

    let known: HashSet<&str> = existing
        .iter()
        .filter_map(|e| e.source_record_id.as_deref())
        .collect();
    let mut live = index_live(existing);
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for rec in &records {
        let n = occurrences.entry(canonicalize(rec, 0)).or_insert(0);
        let occurrence = *n;
        *n += 1;

        let parsed = parse_row(rec, &fields, occurrence, catalog);
        let Some(event) = parsed.event else {
            out.counts.rejected += 1;
            out.rows.push(parsed.preview);
            continue;
        };
        let id = event.source_record_id.clone().unwrap_or_default();
        if known.contains(id.as_str()) || seen.contains(&id) || take_live_match(&mut live, &event) {
            out.counts.duplicates += 1;
            out.rows.push(RowPreview {
                kind: RowKind::Duplicate,
                date: parsed.preview.date,
                summary: parsed.preview.summary,
                reason: None,
            });
            continue;
        }
        seen.insert(id);
        out.events.push(event);
        if parsed.unresolved {
            out.counts.unresolved += 1;
        } else {
            out.counts.parsed += 1;
        }
        out.rows.push(parsed.preview);
    }
    out
}

/// Problems first: the sample is capped, and a rejected row is what needs eyes.
fn preview_sample(rows: Vec<RowPreview>) -> Vec<RowPreview> {
    let mut sample = Vec::new();
    for kind in [
        RowKind::Rejected,
        RowKind::Unresolved,
        RowKind::Parsed,
        RowKind::Duplicate,
    ] {
        sample.extend(rows.iter().filter(|r| r.kind == kind).cloned());
    }
    sample.truncate(MAX_PREVIEW_ROWS);
    sample
}

/// Staged previews awaiting apply; a batch id can only be applied once.
#[derive(Default)]
pub struct GdprStaging(Mutex<VecDeque<(String, Vec<TradeEvent>)>>);

impl GdprStaging {
    /// Read and stage a file; the returned batch id is what apply consumes.
    pub fn preview_file(
        &self,
        path: &Path,
        existing: &[TradeEvent],
        catalog: &dyn ItemCatalog,
    ) -> Result<ImportPreview, LedgerErrorCode> {
        let read = || -> std::io::Result<Option<String>> {
            if std::fs::metadata(path)?.len() > MAX_IMPORT_FILE_BYTES {
                return Ok(None);
            }
            let text = std::fs::read_to_string(path)?;
            Ok(Some(text.trim_start_matches('\u{feff}').to_string()))
        };
        let text = match read() {
            Ok(Some(t)) => t,
            Ok(None) => return Err(LedgerErrorCode::FileTooLarge),
            Err(e) => {
                tracing::warn!("[Ledger] GDPR import read failed: {e}");
                return Err(LedgerErrorCode::ImportReadFailed);
            }
        };
        let file_name = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let result = parse_gdpr_trade_export(&text, &file_name, existing, catalog);
        if result.rows.is_empty() {
            return Err(LedgerErrorCode::NoRows);
        }
        let batch_id = uuid::Uuid::new_v4().to_string();
        let mut staged = self.0.lock().unwrap();
        if staged.len() >= MAX_STAGED_BATCHES {
            staged.pop_front();
        }
        staged.push_back((batch_id.clone(), result.events));
        Ok(ImportPreview {
            batch_id,
            file_name,
            counts: result.counts,
            rows: preview_sample(result.rows),
        })
    }

    /// Consume a staged batch, stamping its id on each row.
    pub fn take(&self, batch_id: &str) -> Option<Vec<TradeEvent>> {
        let mut staged = self.0.lock().unwrap();
        let at = staged.iter().position(|(id, _)| id == batch_id)?;
        let (_, mut events) = staged.remove(at)?;
        for e in &mut events {
            e.import_batch_id = Some(batch_id.to_string());
        }
        Some(events)
    }

    pub fn apply(
        &self,
        batch_id: &str,
        sink: &dyn LedgerSink,
    ) -> Result<ApplyOutcome, LedgerErrorCode> {
        sink.add_events(self.take(batch_id).ok_or(LedgerErrorCode::BatchGone)?)
    }
}

#[cfg(test)]
mod tests;
