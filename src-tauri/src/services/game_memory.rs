//! Reads the game's own resident inventory JSON, read-only. Port of WFHelper's
//! gameMemoryInventory/gameMemoryWin; all OS access sits behind `ProcessMemoryReader`.
use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde_json::Value;

const ANCHOR: &[u8] = b"\"LastInventorySync\"";
const SYNC_ID_WINDOW: usize = 64;
const ANCHOR_WITH_ID: usize = ANCHOR.len() + SYNC_ID_WINDOW;
// A 2026-09 inventory is 0.9 MB with this key 3 KB before its end.
const MAX_INVENTORY_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_BYTES_AFTER_ANCHOR: u64 = 1024 * 1024;
const MAX_CANDIDATES: usize = 256;
const MAX_EXTRACTIONS_PER_PROCESS: usize = 8;
const MAX_REPORTED_SYNC_TIMES: usize = 10;

const CHUNK: usize = 4 * 1024 * 1024;
// Keeps the anchor and its id whole across a chunk boundary.
const OVERLAP: usize = 256;
pub const PAGE_SIZE: u64 = 4096;
const USER_ADDRESS_CEILING: u64 = 0x7fff_fff0_0000;

pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_PRIVATE: u32 = 0x20000;
pub const PAGE_READWRITE: u32 = 0x04;

/// One `VirtualQueryEx` result.
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub base: u64,
    pub size: u64,
    pub state: u32,
    pub protect: u32,
    pub kind: u32,
}

pub struct ReadOutcome {
    pub bytes: usize,
    pub failed: bool,
}

/// Read-only view of one process. The only place the OS is touched.
pub trait ProcessMemoryReader {
    /// The region containing `address` (base rounded down to its page), or `None` past the last one.
    fn query(&self, address: u64) -> Option<Region>;
    fn read(&self, address: u64, out: &mut [u8]) -> ReadOutcome;
    /// Whether each of `pages` pages from `first_page` is in the process's working set; `None` when unknown.
    fn resident_pages(&self, first_page: u64, pages: usize) -> Option<Vec<bool>>;
}

/// Heap memory only: committed, private and plain read-write, as parsed JSON lives.
pub fn is_private_read_write(r: &Region) -> bool {
    r.state == MEM_COMMIT && r.kind == MEM_PRIVATE && r.protect == PAGE_READWRITE
}

type Span = (u64, u64);

/// The run of back-to-back `ascending` regions around `address`, clipped to `limit` either side.
fn span_around(
    ascending: impl IntoIterator<Item = Span>,
    address: u64,
    limit: u64,
) -> Option<Span> {
    let mut run: Option<Span> = None;
    for (start, end) in ascending {
        match run {
            Some((_, ref mut run_end)) if start == *run_end => *run_end = end,
            _ => {
                if run.is_some_and(|(_, e)| e > address) {
                    break;
                }
                if start > address {
                    return None;
                }
                run = Some((start, end));
            }
        }
        if run.is_some_and(|(_, e)| e >= address + limit) {
            break;
        }
    }
    let (start, end) = run?;
    if address < start || address >= end {
        return None;
    }
    Some((
        start.max(address.saturating_sub(limit)),
        end.min(address + limit),
    ))
}

/// Regions of `reader` from `from` upward until `to`.
fn regions<'a>(
    reader: &'a dyn ProcessMemoryReader,
    from: u64,
    to: u64,
) -> impl Iterator<Item = Region> + 'a {
    let mut addr = from;
    std::iter::from_fn(move || {
        if addr >= to {
            return None;
        }
        let r = reader.query(addr)?;
        let next = r.base.checked_add(r.size)?;
        if r.size == 0 || next <= addr {
            return None;
        }
        addr = next;
        Some(r)
    })
}

fn readable_span(reader: &dyn ProcessMemoryReader, address: u64, limit: u64) -> Option<Span> {
    let spans = regions(reader, address.saturating_sub(limit), address + limit)
        .filter(is_private_read_write)
        .map(|r| (r.base, r.base + r.size));
    span_around(spans, address, limit)
}

/// Seconds in the ObjectId's first four bytes, as epoch milliseconds.
pub fn sync_id_time(sync_id: &str) -> u64 {
    sync_id
        .get(..8)
        .and_then(|h| u64::from_str_radix(h, 16).ok())
        .unwrap_or(0)
        * 1000
}

fn is_sync_id(s: &str) -> bool {
    s.len() == 24 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn inventory_sync_id(inventory: &Value) -> Option<&str> {
    let oid = inventory.get("LastInventorySync")?.get("$oid")?.as_str()?;
    is_sync_id(oid).then_some(oid)
}

/// Mirrors WFHelper's `hasInventoryShape`.
fn has_inventory_shape(v: &Value) -> bool {
    [
        "Suits",
        "RawUpgrades",
        "Upgrades",
        "Arcanes",
        "LevelKeys",
        "MiscItems",
    ]
    .iter()
    .any(|k| v.get(k).is_some_and(Value::is_array))
}

enum SyncIdParse {
    Found(String),
    Truncated,
    Absent,
}

/// Matches `\s*:\s*{\s*"$oid"\s*:\s*"<24 hex>"` right after the anchor.
fn parse_sync_id_at(view: &[u8], at: usize) -> SyncIdParse {
    let from = at + ANCHOR.len();
    let to = view.len().min(from + SYNC_ID_WINDOW);
    let w = &view[from..to];
    let mut i = 0;
    let skip_ws = |i: &mut usize| {
        while w.get(*i).is_some_and(|b| b.is_ascii_whitespace()) {
            *i += 1;
        }
    };
    let expect = |i: &mut usize, lit: &[u8]| {
        skip_ws(i);
        if w[*i..].starts_with(lit) {
            *i += lit.len();
            true
        } else {
            false
        }
    };
    let ok = expect(&mut i, b":")
        && expect(&mut i, b"{")
        && expect(&mut i, b"\"$oid\"")
        && expect(&mut i, b":")
        && expect(&mut i, b"\"");
    if ok
        && w.len() >= i + 25
        && w[i + 24] == b'"'
        && let Ok(id) = std::str::from_utf8(&w[i..i + 24])
        && is_sync_id(id)
    {
        return SyncIdParse::Found(id.to_owned());
    }
    if to - from < SYNC_ID_WINDOW {
        SyncIdParse::Truncated
    } else {
        SyncIdParse::Absent
    }
}

fn is_escaped(buf: &[u8], quote_at: usize) -> bool {
    buf[..quote_at]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\\')
        .count()
        % 2
        == 1
}

fn is_json_ws(b: u8) -> bool {
    matches!(b, 0x20 | 0x09 | 0x0a | 0x0d)
}

/// Walks backwards from the anchor to the `{` that opens its object.
fn find_object_start(buf: &[u8], key_at: usize) -> Option<usize> {
    let (mut depth, mut in_string) = (0usize, false);
    for i in (0..key_at).rev() {
        let b = buf[i];
        if in_string {
            if b == b'"' && !is_escaped(buf, i) {
                in_string = false;
            } else if b < 0x20 {
                return None;
            }
            continue;
        }
        match b {
            b'"' => {
                if is_escaped(buf, i) {
                    return None;
                }
                in_string = true;
            }
            b'}' | b']' => depth += 1,
            b'{' | b'[' => {
                if depth == 0 {
                    return (b == b'{').then_some(i);
                }
                depth -= 1;
            }
            _ if (b < 0x20 && !is_json_ws(b)) || b >= 0x7f => return None,
            _ => {}
        }
    }
    None
}

/// Walks forwards from `start` to one past the matching `}`.
fn find_object_end(buf: &[u8], start: usize) -> Option<usize> {
    let (mut depth, mut in_string) = (0usize, false);
    let mut i = start;
    while i < buf.len() {
        let b = buf[i];
        if in_string {
            match b {
                b'\\' => i += 1,
                b'"' => in_string = false,
                _ if b < 0x20 => return None,
                _ => {}
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i + 1);
                    }
                }
                _ if (b < 0x20 && !is_json_ws(b)) || b >= 0x7f => return None,
                _ => {}
            }
        }
        i += 1;
    }
    None
}

fn find_inventory_object(buf: &[u8], anchor_at: usize) -> Option<(usize, usize)> {
    let start = find_object_start(buf, anchor_at)?;
    let end = find_object_end(buf, start)?;
    (end > anchor_at).then_some((start, end))
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

struct Candidate {
    address: u64,
    region: Span,
    sync_id: String,
}

#[derive(Debug)]
pub struct GameInventoryCopy {
    pub sync_id: String,
    pub sync_time: u64,
    pub inventory: Value,
}

/// What one walk found; counts and times only, so it is safe to log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct InventoryScanStats {
    pub copies: usize,
    pub sync_times: Vec<u64>,
    pub extractions: usize,
    pub failed_extractions: usize,
}

fn extract_candidate(reader: &dyn ProcessMemoryReader, c: &Candidate) -> Option<GameInventoryCopy> {
    // A copy measured in 2026-09 spanned several adjacent 64 KB regions.
    let (rs, re) = readable_span(reader, c.address, MAX_INVENTORY_JSON_BYTES).unwrap_or(c.region);
    let low = rs.max(c.address.saturating_sub(MAX_INVENTORY_JSON_BYTES));
    let high = re.min(c.address + MAX_BYTES_AFTER_ANCHOR);
    let anchor_at = (c.address - low) as usize;
    if high <= low || ((high - low) as usize) < anchor_at + ANCHOR.len() {
        return None;
    }
    let mut buf = vec![0u8; (high - low) as usize];
    let n = reader.read(low, &mut buf).bytes;
    if n < anchor_at + ANCHOR.len() {
        return None;
    }
    let view = &buf[..n];
    // The anchor may have been rewritten between the walk and this read.
    if &view[anchor_at..anchor_at + ANCHOR.len()] != ANCHOR {
        return None;
    }
    let (start, end) = find_inventory_object(view, anchor_at)?;
    let parsed: Value = serde_json::from_slice(&view[start..end]).ok()?;
    if !has_inventory_shape(&parsed) || inventory_sync_id(&parsed) != Some(c.sync_id.as_str()) {
        return None;
    }
    Some(GameInventoryCopy {
        sync_time: sync_id_time(&c.sync_id),
        sync_id: c.sync_id.clone(),
        inventory: parsed,
    })
}

/// Collects `LastInventorySync` anchors during a walk and parses the newest complete copy.
#[derive(Default)]
pub struct InventoryCollector {
    candidates: Vec<Candidate>,
    seen: HashSet<u64>,
    newest: Option<GameInventoryCopy>,
    sync_times: HashSet<u64>,
    copies: usize,
    extractions: usize,
    failed_extractions: usize,
    // The previous chunk's tail, joined to the next chunk's head when that starts where it ended.
    tail: Vec<u8>,
    tail_end: u64,
    tail_region: Option<Span>,
}

/// One consumer of a process walk; chunks of a region overlap by `OVERLAP` bytes.
pub trait MemoryVisitor {
    fn chunk(&mut self, view: &[u8], address: u64, region: Span);
    fn finish(&mut self, reader: &dyn ProcessMemoryReader);
}

impl MemoryVisitor for InventoryCollector {
    fn chunk(&mut self, view: &[u8], address: u64, region: Span) {
        self.chunk(view, address, region)
    }
    fn finish(&mut self, reader: &dyn ProcessMemoryReader) {
        self.finish(reader)
    }
}

impl InventoryCollector {
    fn keep_newest(&mut self, next: Candidate) {
        if self.candidates.len() < MAX_CANDIDATES {
            self.candidates.push(next);
            return;
        }
        let time = |c: &Candidate| sync_id_time(&c.sync_id);
        let (oldest, _) = self
            .candidates
            .iter()
            .enumerate()
            .min_by_key(|(_, c)| time(c))
            .unwrap();
        if time(&next) > time(&self.candidates[oldest]) {
            self.candidates[oldest] = next;
        }
    }

    fn collect(&mut self, view: &[u8], address: u64, region: Span, before: usize) {
        let mut idx = 0;
        while let Some(at) = find(view, ANCHOR, idx).filter(|&at| at < before) {
            idx = at + ANCHOR.len();
            if self.seen.contains(&(address + at as u64)) {
                continue;
            }
            let parsed = parse_sync_id_at(view, at);
            // A cut-off id is read again from the next chunk's overlap or joint.
            if matches!(parsed, SyncIdParse::Truncated) {
                continue;
            }
            self.seen.insert(address + at as u64);
            if let SyncIdParse::Found(sync_id) = parsed {
                self.copies += 1;
                self.sync_times.insert(sync_id_time(&sync_id));
                self.keep_newest(Candidate {
                    address: address + at as u64,
                    region,
                    sync_id,
                });
            }
        }
    }

    pub fn chunk(&mut self, view: &[u8], address: u64, region: Span) {
        if let (Some(tail_region), true) = (
            self.tail_region,
            !self.tail.is_empty() && address == self.tail_end,
        ) {
            let mut joined = std::mem::take(&mut self.tail);
            let tail_len = joined.len();
            joined.extend_from_slice(&view[..view.len().min(ANCHOR_WITH_ID)]);
            self.collect(
                &joined,
                self.tail_end - tail_len as u64,
                tail_region,
                tail_len,
            );
        }
        self.collect(view, address, region, view.len());
        self.tail = view[view.len().saturating_sub(ANCHOR_WITH_ID)..].to_vec();
        self.tail_end = address + view.len() as u64;
        self.tail_region = Some(region);
    }

    /// Runs once per scanned process, before its reader is dropped.
    pub fn finish(&mut self, reader: &dyn ProcessMemoryReader) {
        let mut ordered = std::mem::take(&mut self.candidates);
        ordered.sort_by_key(|c| std::cmp::Reverse(sync_id_time(&c.sync_id)));
        self.seen.clear();
        self.tail_region = None;
        self.tail.clear();
        for c in ordered.iter().take(MAX_EXTRACTIONS_PER_PROCESS) {
            if self
                .newest
                .as_ref()
                .is_some_and(|n| sync_id_time(&c.sync_id) <= n.sync_time)
            {
                break;
            }
            self.extractions += 1;
            match extract_candidate(reader, c) {
                Some(copy) => {
                    self.newest = Some(copy);
                    break;
                }
                None => self.failed_extractions += 1,
            }
        }
    }

    pub fn result(&mut self) -> (Option<GameInventoryCopy>, InventoryScanStats) {
        let mut times: Vec<u64> = self.sync_times.iter().copied().collect();
        times.sort_unstable_by(|a, b| b.cmp(a));
        times.truncate(MAX_REPORTED_SYNC_TIMES);
        (
            self.newest.take(),
            InventoryScanStats {
                copies: self.copies,
                sync_times: times,
                extractions: self.extractions,
                failed_extractions: self.failed_extractions,
            },
        )
    }
}

#[derive(Debug, Default)]
pub struct WalkStats {
    pub regions: usize,
    pub bytes: u64,
    /// Committed bytes left unread because the game did not have them in RAM.
    pub skipped_bytes: u64,
}

/// Runs of `[start, start + len)` that are resident, as `(offset, length)`; `None` when unknown.
fn resident_runs(
    reader: &dyn ProcessMemoryReader,
    start: u64,
    len: usize,
) -> Option<Vec<(usize, usize)>> {
    let first = start / PAGE_SIZE * PAGE_SIZE;
    let pages = (start + len as u64 - first).div_ceil(PAGE_SIZE) as usize;
    let flags = reader.resident_pages(first, pages)?;
    let lead = (start - first) as usize;
    let page = PAGE_SIZE as usize;
    let mut runs = Vec::new();
    let mut run_start = None;
    for i in 0..=pages {
        let valid = flags.get(i).copied().unwrap_or(false) && i < pages;
        if valid && run_start.is_none() {
            run_start = Some(i);
        } else if !valid && let Some(s) = run_start.take() {
            let from = (s * page).saturating_sub(lead);
            let to = (i * page).saturating_sub(lead).min(len);
            if to > from {
                runs.push((from, to - from));
            }
        }
    }
    Some(runs)
}

/// Walks one process's private read-write memory, feeding chunks to `collector`.
/// `resident_only` skips pages outside the game's working set: reading them pages them
/// back into the game (2.2 GB read against 973 MB game RAM lagged it, 2026-09-24).
pub fn walk_process(
    reader: &dyn ProcessMemoryReader,
    collector: &mut impl MemoryVisitor,
    resident_only: bool,
) -> WalkStats {
    let mut stats = WalkStats::default();
    let mut chunk = vec![0u8; CHUNK];
    for region in regions(reader, 0, USER_ADDRESS_CEILING).filter(is_private_read_write) {
        stats.regions += 1;
        let span = (region.base, region.base + region.size);
        let mut off = 0u64;
        while off < region.size {
            let len = (region.size - off).min(CHUNK as u64) as usize;
            let at = region.base + off;
            let mut visit = |o: usize, l: usize| {
                let n = reader.read(at + o as u64, &mut chunk[..l]).bytes;
                if n > 0 {
                    collector.chunk(&chunk[..n], at + o as u64, span);
                    stats.bytes += n as u64;
                }
            };
            match resident_only
                .then(|| resident_runs(reader, at, len))
                .flatten()
            {
                None => visit(0, len),
                Some(runs) => {
                    let mut resident = 0;
                    for (o, l) in runs {
                        resident += l;
                        visit(o, l);
                    }
                    stats.skipped_bytes += (len - resident) as u64;
                }
            }
            if len <= OVERLAP {
                break;
            }
            off += (len - OVERLAP) as u64;
        }
    }
    collector.finish(reader);
    zeroize::Zeroize::zeroize(&mut chunk); // held game memory, auth string included
    stats
}

#[derive(Debug, PartialEq, Eq)]
pub enum OpenFailure {
    ProcessNotFound,
    AccessDenied,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GameInventoryStatus {
    Ok,
    ProcessNotFound,
    AccessDenied,
    NotFound,
    Unavailable,
}

#[derive(Debug)]
pub struct GameInventoryRead {
    pub status: GameInventoryStatus,
    pub newest: Option<GameInventoryCopy>,
    pub stats: InventoryScanStats,
    pub scan_ms: u128,
    pub scanned_mb: u64,
    pub skipped_mb: u64,
    pub regions: usize,
}

/// Scans every reader and reports the newest inventory copy found.
pub fn read_inventory_from(
    readers: Result<Vec<Box<dyn ProcessMemoryReader>>, OpenFailure>,
) -> GameInventoryRead {
    let started = Instant::now();
    let mut collector = InventoryCollector::default();
    let mut total = WalkStats::default();
    let failure = match readers {
        Ok(readers) => {
            for r in &readers {
                let s = walk_process(r.as_ref(), &mut collector, true);
                total.regions += s.regions;
                total.bytes += s.bytes;
                total.skipped_bytes += s.skipped_bytes;
            }
            None
        }
        Err(f) => Some(f),
    };
    let (newest, stats) = collector.result();
    let status = match failure {
        Some(OpenFailure::ProcessNotFound) => GameInventoryStatus::ProcessNotFound,
        Some(OpenFailure::AccessDenied) => GameInventoryStatus::AccessDenied,
        Some(OpenFailure::Unavailable) => GameInventoryStatus::Unavailable,
        None if newest.is_some() => GameInventoryStatus::Ok,
        None => GameInventoryStatus::NotFound,
    };
    GameInventoryRead {
        status,
        newest,
        stats,
        scan_ms: started.elapsed().as_millis(),
        scanned_mb: total.bytes / (1024 * 1024),
        skipped_mb: total.skipped_bytes / (1024 * 1024),
        regions: total.regions,
    }
}

/// Reads the running game's inventory without any request to DE.
pub fn read_game_inventory() -> GameInventoryRead {
    read_inventory_from(open_game())
}

/// The only way to a process handle: refused unless memory mode is on with consent.
fn open_game() -> Result<Vec<Box<dyn ProcessMemoryReader>>, OpenFailure> {
    if !crate::services::memory_gate::allowed() {
        return Err(OpenFailure::Unavailable);
    }
    #[cfg(windows)]
    return crate::platform::windows::process::open_warframe();
    #[cfg(not(windows))]
    Err(OpenFailure::Unavailable)
}

const ACCOUNT_ID_MARKERS: [&[u8]; 2] = [b"?accountId=", b"&accountId="];
const ACCOUNT_ID_LEN: usize = 24; // DE account ids are 24-hex Mongo ObjectIds
const NONCE_SEP: &[u8] = b"&nonce=";
const MAX_NONCE_DIGITS: usize = 24;

/// The user's own `accountId`/`nonce` from their running game. Memory-only: never serialize,
/// log or persist it. Debug is redacted and the bytes are wiped on drop.
#[derive(Clone, PartialEq, Eq, Hash, zeroize::ZeroizeOnDrop)]
pub struct GameAuthz {
    account_id: String,
    nonce: String,
}

impl std::fmt::Debug for GameAuthz {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GameAuthz(<redacted>)")
    }
}

impl GameAuthz {
    /// `?accountId=..&nonce=..`, for DE's inventory endpoint.
    pub fn query(&self) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(format!(
            "?accountId={}&nonce={}",
            self.account_id, self.nonce
        ))
    }
}

/// Why marker hits were not usable; counts only, never candidate contents.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AuthzDiagnostics {
    pub truncated: u32,
    pub invalid_account_id: u32,
    pub missing_nonce: u32,
    pub empty_nonce: u32,
    pub nonce_too_long: u32,
}

fn parse_authz_candidate(
    view: &[u8],
    at: usize,
    marker_len: usize,
    d: &mut AuthzDiagnostics,
) -> Option<GameAuthz> {
    let id_start = at + marker_len;
    let id_end = id_start + ACCOUNT_ID_LEN;
    if id_end > view.len() {
        d.truncated += 1;
        return None;
    }
    let id = &view[id_start..id_end];
    if !id.iter().all(u8::is_ascii_hexdigit) {
        d.invalid_account_id += 1;
        return None;
    }
    let sep_end = id_end + NONCE_SEP.len();
    if sep_end > view.len() {
        d.truncated += 1;
        return None;
    }
    if &view[id_end..sep_end] != NONCE_SEP {
        d.missing_nonce += 1;
        return None;
    }
    let digits = view[sep_end..]
        .iter()
        .take(MAX_NONCE_DIGITS)
        .take_while(|b| b.is_ascii_digit())
        .count();
    let p = sep_end + digits;
    if digits == 0 {
        if p == view.len() {
            d.truncated += 1;
        } else {
            d.empty_nonce += 1;
        }
        return None;
    }
    if view.get(p).is_some_and(u8::is_ascii_digit) {
        d.nonce_too_long += 1;
        return None;
    }
    // Both slices were checked as ASCII above.
    Some(GameAuthz {
        account_id: String::from_utf8_lossy(id).to_ascii_lowercase(),
        nonce: String::from_utf8_lossy(&view[sep_end..p]).into_owned(),
    })
}

/// Parses the auth string at a marker hit, or `None` when the offset is not one.
pub fn parse_authz_at(view: &[u8], at: usize, d: &mut AuthzDiagnostics) -> Option<GameAuthz> {
    let marker = ACCOUNT_ID_MARKERS
        .iter()
        .find(|m| view.get(at..).is_some_and(|v| v.starts_with(m)))?;
    parse_authz_candidate(view, at, marker.len(), d)
}

/// Counts every auth string in `view`; returns the number of marker hits.
pub fn scan_buffer_for_authz(
    view: &[u8],
    counts: &mut HashMap<GameAuthz, u32>,
    d: &mut AuthzDiagnostics,
) -> usize {
    let mut hits = 0;
    for marker in ACCOUNT_ID_MARKERS {
        let mut from = 0;
        while let Some(at) = find(view, marker, from) {
            hits += 1;
            if let Some(a) = parse_authz_candidate(view, at, marker.len(), d) {
                *counts.entry(a).or_default() += 1;
            }
            from = at + marker.len();
        }
    }
    hits
}

/// The unique most frequent auth string; equal leaders are not safe to use.
pub fn best_authz(counts: &mut HashMap<GameAuthz, u32>) -> (Option<GameAuthz>, u32, bool) {
    let hits = counts.values().copied().max().unwrap_or(0);
    let leaders: Vec<GameAuthz> = counts
        .keys()
        .filter(|_| hits > 0)
        .filter(|k| counts[*k] == hits)
        .cloned()
        .collect();
    match leaders.len() {
        0 => (None, 0, false),
        1 => (
            counts.remove_entry(&leaders[0]).map(|(k, _)| k),
            hits,
            false,
        ),
        _ => (None, hits, true),
    }
}

#[derive(Default)]
struct AuthzCollector {
    counts: HashMap<GameAuthz, u32>,
    diagnostics: AuthzDiagnostics,
    marker_hits: usize,
}

impl MemoryVisitor for AuthzCollector {
    fn chunk(&mut self, view: &[u8], _address: u64, _region: Span) {
        self.marker_hits += scan_buffer_for_authz(view, &mut self.counts, &mut self.diagnostics);
    }
    fn finish(&mut self, _reader: &dyn ProcessMemoryReader) {}
}

#[derive(Debug, PartialEq, Eq)]
pub enum GameAuthzStatus {
    Ok,
    ProcessNotFound,
    AccessDenied,
    Unavailable,
    CrumbsNotFound,
    CrumbsAmbiguous,
}

#[derive(Debug)]
pub struct GameAuthzRead {
    pub status: GameAuthzStatus,
    pub authz: Option<GameAuthz>,
    /// Times the winning string was seen.
    pub hits: u32,
    pub marker_hits: usize,
    pub rejects: AuthzDiagnostics,
}

/// Scans every reader for the game's auth string. Read-only; nothing is logged or stored.
pub fn read_authz_from(
    readers: Result<Vec<Box<dyn ProcessMemoryReader>>, OpenFailure>,
) -> GameAuthzRead {
    let mut c = AuthzCollector::default();
    let failure = match readers {
        Ok(readers) => {
            for r in &readers {
                walk_process(r.as_ref(), &mut c, true);
            }
            None
        }
        Err(f) => Some(f),
    };
    let (authz, hits, ambiguous) = best_authz(&mut c.counts);
    let status = match failure {
        Some(OpenFailure::ProcessNotFound) => GameAuthzStatus::ProcessNotFound,
        Some(OpenFailure::AccessDenied) => GameAuthzStatus::AccessDenied,
        Some(OpenFailure::Unavailable) => GameAuthzStatus::Unavailable,
        None if ambiguous => GameAuthzStatus::CrumbsAmbiguous,
        None if authz.is_none() => GameAuthzStatus::CrumbsNotFound,
        None => GameAuthzStatus::Ok,
    };
    GameAuthzRead {
        status,
        authz,
        hits,
        marker_hits: c.marker_hits,
        rejects: std::mem::take(&mut c.diagnostics),
    }
}

/// Reads the running game's own auth string, to fetch the user's own inventory.
pub fn read_game_authz() -> GameAuthzRead {
    read_authz_from(open_game())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PLASTIDS: &str = "/Lotus/Types/Items/MiscItems/Plastids";
    const OLD_SYNC: &str = "6aaf0000aaaaaaaaaaaaaaaa";
    const NEW_SYNC: &str = "6aaf1000bbbbbbbbbbbbbbbb";
    const REGION: usize = 64 * 1024;
    const MEM_IMAGE: u32 = 0x1000000;

    fn inventory_json(sync_id: &str, plastids: u32, padding: usize) -> String {
        // preserve_order is off, so build the text by hand to keep LastInventorySync after Filler.
        format!(
            r#"{{"SubscribedToEmails":0,"MiscItems":[{{"ItemType":"{PLASTIDS}","ItemCount":{plastids}}}],"Tricky":"braces }} {{ and a \"quote\" and a trailing backslash \\","Filler":"{}","LastInventorySync":{{"$oid":"{sync_id}"}},"RegularCredits":100}}"#,
            "x".repeat(padding)
        )
    }

    fn anchor_in(buf: &[u8]) -> usize {
        find(buf, ANCHOR, 0).unwrap()
    }

    struct FakeRegion {
        base: u64,
        contents: Vec<u8>,
        kind: u32,
    }

    struct Fake {
        regions: Vec<FakeRegion>,
        resident_below: Option<u64>,
    }

    impl Fake {
        fn new(regions: Vec<FakeRegion>) -> Self {
            Self {
                regions,
                resident_below: None,
            }
        }
        fn at(&self, a: u64) -> Option<&FakeRegion> {
            self.regions
                .iter()
                .find(|r| a >= r.base && a < r.base + r.contents.len() as u64)
        }
    }

    impl ProcessMemoryReader for Fake {
        // Like VirtualQueryEx: rounds down to a page and reports gaps as free.
        fn query(&self, a: u64) -> Option<Region> {
            let page = a - a % PAGE_SIZE;
            if let Some(r) = self.at(a) {
                let base = page.max(r.base);
                return Some(Region {
                    base,
                    size: r.base + r.contents.len() as u64 - base,
                    state: MEM_COMMIT,
                    protect: PAGE_READWRITE,
                    kind: r.kind,
                });
            }
            let next = self.regions.iter().find(|r| r.base > a)?;
            Some(Region {
                base: page,
                size: next.base - page,
                state: 0x10000,
                protect: 1,
                kind: 0,
            })
        }
        fn read(&self, a: u64, out: &mut [u8]) -> ReadOutcome {
            let mut copied = 0;
            while copied < out.len() {
                let Some(r) = self.at(a + copied as u64) else {
                    break;
                };
                let from = (a + copied as u64 - r.base) as usize;
                let n = (r.contents.len() - from).min(out.len() - copied);
                out[copied..copied + n].copy_from_slice(&r.contents[from..from + n]);
                copied += n;
            }
            ReadOutcome {
                bytes: copied,
                failed: copied < out.len(),
            }
        }
        fn resident_pages(&self, first: u64, pages: usize) -> Option<Vec<bool>> {
            let below = self.resident_below?;
            Some(
                (0..pages as u64)
                    .map(|i| first + i * PAGE_SIZE < below)
                    .collect(),
            )
        }
    }

    /// Feeds a memory image to a collector the way `walk_process` chunks a region.
    fn walk_image(
        memory: &[u8],
        chunk_size: usize,
    ) -> (Option<GameInventoryCopy>, InventoryScanStats) {
        let base = 0x10000u64;
        let region = (base, base + memory.len() as u64);
        let mut c = InventoryCollector::default();
        let mut off = 0;
        while off < memory.len() {
            let view = &memory[off..memory.len().min(off + chunk_size)];
            c.chunk(view, base + off as u64, region);
            if off + chunk_size >= memory.len() {
                break;
            }
            off += chunk_size - OVERLAP;
        }
        c.finish(&Fake::new(vec![FakeRegion {
            base,
            contents: memory.to_vec(),
            kind: MEM_PRIVATE,
        }]));
        c.result()
    }

    fn walk_fake(fake: &Fake, resident_only: bool) -> (InventoryCollector, WalkStats) {
        let mut c = InventoryCollector::default();
        let s = walk_process(fake, &mut c, resident_only);
        (c, s)
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    #[test]
    fn parses_whole_object_ignoring_braces_and_quotes_in_strings() {
        let json = inventory_json(NEW_SYNC, 3, 0);
        let memory = cat(&[&[0, 0, 7, b'{', b'}'], json.as_bytes(), &[0, b'}', 0]]);
        let (newest, _) = walk_image(&memory, 64 * 1024);
        assert_eq!(
            newest.unwrap().inventory,
            serde_json::from_str::<Value>(&json).unwrap()
        );
    }

    #[test]
    fn rejects_garbage_before_the_opening_brace() {
        let json = inventory_json(NEW_SYNC, 3, 0);
        let memory = cat(&[&[1, 2, 3], &json.as_bytes()[1..]]);
        let (newest, stats) = walk_image(&memory, 64 * 1024);
        assert!(newest.is_none());
        assert_eq!(
            (stats.copies, stats.extractions, stats.failed_extractions),
            (1, 1, 1)
        );
    }

    #[test]
    fn rejects_an_object_cut_off_before_its_closing_brace() {
        let json = inventory_json(NEW_SYNC, 3, 0);
        let memory = cat(&[&json.as_bytes()[..json.len() - 1], &[0, 0]]);
        let (newest, stats) = walk_image(&memory, 64 * 1024);
        assert!(newest.is_none());
        assert_eq!(
            (stats.copies, stats.extractions, stats.failed_extractions),
            (1, 1, 1)
        );
    }

    #[test]
    fn sync_id_time_and_validation() {
        assert_eq!(sync_id_time(NEW_SYNC), 0x6aaf1000 * 1000);
        assert_eq!(
            inventory_sync_id(&json!({"LastInventorySync": {"$oid": NEW_SYNC}})),
            Some(NEW_SYNC)
        );
        assert_eq!(
            inventory_sync_id(&json!({"LastInventorySync": {"$oid": "short"}})),
            None
        );
        assert_eq!(inventory_sync_id(&Value::Null), None);
    }

    #[test]
    fn inventory_shape_needs_a_known_array() {
        assert!(has_inventory_shape(&json!({"MiscItems": []})));
        assert!(!has_inventory_shape(&json!({"MiscItems": 1})));
        assert!(!has_inventory_shape(&json!([])));
    }

    #[test]
    fn parses_newest_of_several_copies_and_counts_every_one() {
        let memory = cat(&[
            inventory_json(OLD_SYNC, 1, 0).as_bytes(),
            &[0; 64],
            b"\"LastInventorySync\" in some unrelated string table",
            &[0; 64],
            inventory_json(NEW_SYNC, 9, 0).as_bytes(),
            &[0; 64],
        ]);
        let (newest, stats) = walk_image(&memory, 64 * 1024);
        let newest = newest.unwrap();
        assert_eq!(newest.sync_id, NEW_SYNC);
        assert_eq!(
            newest.inventory["MiscItems"],
            json!([{"ItemType": PLASTIDS, "ItemCount": 9}])
        );
        assert_eq!(stats.copies, 2);
        assert_eq!(
            stats.sync_times,
            [sync_id_time(NEW_SYNC), sync_id_time(OLD_SYNC)]
        );
        assert_eq!((stats.extractions, stats.failed_extractions), (1, 0));
    }

    #[test]
    fn falls_back_to_the_next_copy_when_the_newest_is_torn() {
        let torn = inventory_json(NEW_SYNC, 9, 0);
        let memory = cat(&[
            inventory_json(OLD_SYNC, 1, 0).as_bytes(),
            &[0; 32],
            &torn.as_bytes()[..40],
            &[0, 0, 0, 1],
            &torn.as_bytes()[44..],
            &[0; 32],
        ]);
        let (newest, stats) = walk_image(&memory, 64 * 1024);
        assert_eq!(newest.unwrap().sync_id, OLD_SYNC);
        assert_eq!(
            (stats.copies, stats.extractions, stats.failed_extractions),
            (2, 2, 1)
        );
    }

    #[test]
    fn reassembles_a_copy_spanning_chunks_and_counts_an_overlapped_anchor_once() {
        let (chunk, step) = (1024usize, 1024 - OVERLAP);
        let json = inventory_json(NEW_SYNC, 4, 3_000);
        // Puts the anchor 100 bytes into a chunk, which is also the previous chunk's overlap.
        let prefix = ((100 + step - anchor_in(json.as_bytes()) % step) % step) % step;
        let memory = cat(&[&vec![0; prefix], json.as_bytes(), &[0; 100]]);
        assert_eq!(anchor_in(&memory) % step, 100);
        let (newest, stats) = walk_image(&memory, chunk);
        assert_eq!(newest.unwrap().sync_id, NEW_SYNC);
        assert_eq!(stats.copies, 1);
    }

    #[test]
    fn keeps_the_newest_copy_when_stale_anchors_pass_the_candidate_cap() {
        let stale = format!(r#""LastInventorySync":{{"$oid":"{OLD_SYNC}"}} padding"#);
        let mut memory = stale.repeat(300).into_bytes();
        memory.extend(cat(&[
            &[0; 64],
            inventory_json(NEW_SYNC, 9, 0).as_bytes(),
            &[0; 64],
        ]));
        let (newest, stats) = walk_image(&memory, 64 * 1024);
        assert_eq!(stats.copies, 301);
        assert_eq!(newest.unwrap().sync_id, NEW_SYNC);
    }

    #[test]
    fn finds_nothing_in_memory_without_an_inventory() {
        let (newest, stats) = walk_image(&[0x41; 4096], 1024);
        assert!(newest.is_none());
        assert_eq!(stats.copies, 0);
    }

    #[test]
    fn walk_reads_only_private_read_write_memory() {
        let heap = cat(&[&[0; 4096], inventory_json(OLD_SYNC, 2, 0).as_bytes()]);
        let image = inventory_json(NEW_SYNC, 99, 0).into_bytes();
        let image_len = image.len() as u64;
        let fake = Fake::new(vec![
            FakeRegion {
                base: 0,
                contents: image,
                kind: MEM_IMAGE,
            },
            FakeRegion {
                base: image_len,
                contents: heap,
                kind: MEM_PRIVATE,
            },
        ]);
        let (mut c, stats) = walk_fake(&fake, false);
        assert_eq!(stats.regions, 1);
        assert_eq!(c.result().0.unwrap().sync_id, OLD_SYNC);
    }

    #[test]
    fn walk_skips_pages_outside_the_working_set_when_asked() {
        let pad = |mut b: Vec<u8>| {
            b.resize(b.len().div_ceil(4096) * 4096, 0);
            b
        };
        let in_ram = pad(inventory_json(OLD_SYNC, 2, 0).into_bytes());
        let heap = cat(&[&in_ram, &pad(inventory_json(NEW_SYNC, 99, 0).into_bytes())]);
        let heap_len = heap.len();
        let mut fake = Fake::new(vec![FakeRegion {
            base: 0,
            contents: heap,
            kind: MEM_PRIVATE,
        }]);
        fake.resident_below = Some(in_ram.len() as u64);
        let (mut c, stats) = walk_fake(&fake, true);
        assert_eq!(c.result().0.unwrap().sync_id, OLD_SYNC);
        assert_eq!(stats.bytes, in_ram.len() as u64);
        assert!(stats.skipped_bytes >= (heap_len - in_ram.len()) as u64);
    }

    fn split_regions(memory: &[u8], kinds: &[u32]) -> Vec<FakeRegion> {
        kinds
            .iter()
            .enumerate()
            .map(|(i, &kind)| FakeRegion {
                base: (0x10000 + i * REGION) as u64,
                contents: memory[i * REGION..(i + 1) * REGION].to_vec(),
                kind,
            })
            .collect()
    }

    #[test]
    fn extracts_a_copy_spanning_three_adjacent_regions() {
        let json = inventory_json(NEW_SYNC, 7, 110_000);
        let start = 40 * 1024;
        let mut memory = vec![0; 4 * REGION];
        memory[start..start + json.len()].copy_from_slice(json.as_bytes());
        assert_eq!((start / REGION, anchor_in(&memory) / REGION), (0, 2));
        let fake = Fake::new(split_regions(&memory, &[MEM_PRIVATE; 4]));
        let (mut c, _) = walk_fake(&fake, false);
        let (newest, stats) = c.result();
        assert_eq!(
            newest.unwrap().inventory["MiscItems"],
            json!([{"ItemType": PLASTIDS, "ItemCount": 7}])
        );
        assert_eq!((stats.copies, stats.extractions), (1, 1));
    }

    #[test]
    fn misses_a_copy_that_crosses_a_region_outside_the_filter() {
        let json = inventory_json(NEW_SYNC, 7, 110_000);
        let mut memory = vec![0; 4 * REGION];
        memory[40 * 1024..40 * 1024 + json.len()].copy_from_slice(json.as_bytes());
        let fake = Fake::new(split_regions(
            &memory,
            &[MEM_PRIVATE, MEM_IMAGE, MEM_PRIVATE, MEM_PRIVATE],
        ));
        let (mut c, _) = walk_fake(&fake, false);
        assert!(c.result().0.is_none());
    }

    #[test]
    fn finds_an_anchor_whose_name_or_id_crosses_into_the_next_region() {
        for before_boundary in [8usize, 40] {
            let json = inventory_json(NEW_SYNC, 5, 1_000);
            let mut memory = vec![0; 2 * REGION];
            let at = REGION - before_boundary - anchor_in(json.as_bytes());
            memory[at..at + json.len()].copy_from_slice(json.as_bytes());
            assert_eq!(anchor_in(&memory), REGION - before_boundary);
            let fake = Fake::new(split_regions(&memory, &[MEM_PRIVATE; 2]));
            let (mut c, _) = walk_fake(&fake, false);
            let (newest, stats) = c.result();
            assert_eq!(newest.unwrap().sync_id, NEW_SYNC);
            assert_eq!(
                (stats.copies, stats.extractions, stats.failed_extractions),
                (1, 1, 0)
            );
        }
    }

    #[test]
    fn does_not_join_the_tails_of_regions_with_a_gap_between_them() {
        let json = inventory_json(NEW_SYNC, 5, 1_000);
        let mut memory = vec![0; 2 * REGION];
        let at = REGION - 8 - anchor_in(json.as_bytes());
        memory[at..at + json.len()].copy_from_slice(json.as_bytes());
        let mut regions = split_regions(&memory, &[MEM_PRIVATE; 2]);
        regions[1].base += 0x10000;
        let (mut c, _) = walk_fake(&Fake::new(regions), false);
        assert_eq!(c.result().1.copies, 0);
    }

    #[test]
    fn reports_a_missing_game_and_a_failed_open() {
        assert_eq!(
            read_inventory_from(Err(OpenFailure::ProcessNotFound)).status,
            GameInventoryStatus::ProcessNotFound
        );
        assert_eq!(
            read_inventory_from(Err(OpenFailure::AccessDenied)).status,
            GameInventoryStatus::AccessDenied
        );
        assert_eq!(
            read_inventory_from(Ok(vec![])).status,
            GameInventoryStatus::NotFound
        );
    }

    #[test]
    fn reads_every_process_and_returns_the_newest() {
        let make = |sync: &str| -> Box<dyn ProcessMemoryReader> {
            Box::new(Fake::new(vec![FakeRegion {
                base: 0,
                contents: inventory_json(sync, 1, 0).into_bytes(),
                kind: MEM_PRIVATE,
            }]))
        };
        let read = read_inventory_from(Ok(vec![make(OLD_SYNC), make(NEW_SYNC)]));
        assert_eq!(read.status, GameInventoryStatus::Ok);
        assert_eq!(read.newest.unwrap().sync_id, NEW_SYNC);
    }

    #[test]
    fn span_around_joins_back_to_back_regions_and_clips_to_the_limit() {
        let regions = [
            (0x1000, 0x2000),
            (0x2000, 0x3000),
            (0x3000, 0x4000),
            (0x5000, 0x6000),
        ];
        assert_eq!(
            span_around(regions, 0x2800, 0x10000),
            Some((0x1000, 0x4000))
        );
        assert_eq!(span_around(regions, 0x2800, 0x100), Some((0x2700, 0x2900)));
        assert_eq!(
            span_around(regions, 0x5800, 0x10000),
            Some((0x5000, 0x6000))
        );
    }

    #[test]
    fn span_around_has_none_for_an_address_outside_every_region() {
        let regions = [
            (0x1000, 0x2000),
            (0x2000, 0x3000),
            (0x3000, 0x4000),
            (0x5000, 0x6000),
        ];
        for a in [0x800, 0x4800, 0x7000] {
            assert_eq!(span_around(regions, a, 0x10000), None);
        }
    }

    const ACC: &str = "0123456789abcdef01234567";
    fn valid() -> String {
        format!("?accountId={ACC}&nonce=1712345678901")
    }
    fn other() -> String {
        "?accountId=ffffffffffffffffffffffff&nonce=99".into()
    }
    fn parse(s: &str) -> Option<String> {
        let at = s.find("accountId=").unwrap() - 1;
        parse_authz_at(s.as_bytes(), at, &mut AuthzDiagnostics::default())
            .map(|a| a.query().to_string())
    }
    fn counted(s: &str) -> HashMap<GameAuthz, u32> {
        let mut m = HashMap::new();
        scan_buffer_for_authz(s.as_bytes(), &mut m, &mut AuthzDiagnostics::default());
        m
    }
    fn authz(s: &str) -> GameAuthz {
        counted(s).into_keys().next().unwrap()
    }

    #[test]
    fn authz_extracts_a_well_formed_string_and_stops_at_the_first_non_digit() {
        let v = valid();
        assert_eq!(parse(&format!("noise {v} tail")), Some(v.clone()));
        assert_eq!(
            parse(&format!("?accountId={ACC}&nonce=42&extra=x")),
            Some(format!("?accountId={ACC}&nonce=42"))
        );
    }

    #[test]
    fn authz_accepts_account_id_after_another_parameter_and_lowercases() {
        assert_eq!(
            parse(&format!("?platform=pc&accountId={ACC}&nonce=42")),
            Some(format!("?accountId={ACC}&nonce=42"))
        );
        assert_eq!(
            parse(&format!("?accountId={}&nonce=42", ACC.to_uppercase())),
            Some(format!("?accountId={ACC}&nonce=42"))
        );
    }

    #[test]
    fn authz_rejects_malformed_candidates() {
        for bad in [
            "?accountId=zzz456789abcdef01234567&nonce=42".to_string(),
            "?accountId=abc&nonce=42".into(),
            format!("?accountId={ACC}?nonce=42"),
            format!("?accountId={ACC}&nonce=x"),
            format!("?accountId={ACC}&nonce=1234567890123456789012345 "),
            format!("?accountId={ACC}&nonce="),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn authz_nonce_limit_is_24_digits() {
        let nonce = "123456789012345678901234";
        assert_eq!(
            parse(&format!("?accountId={ACC}&nonce={nonce} ")),
            Some(format!("?accountId={ACC}&nonce={nonce}"))
        );
    }

    #[test]
    fn authz_counts_repeats_and_ignores_malformed_candidates() {
        let (v, o) = (valid(), other());
        let text = format!("?accountId=bad&nonce=1 {v} junk {v}   {o} {v}");
        let mut m = HashMap::new();
        let hits = scan_buffer_for_authz(text.as_bytes(), &mut m, &mut AuthzDiagnostics::default());
        assert_eq!(hits, 5);
        assert_eq!(m[&authz(&v)], 3);
        assert_eq!(m[&authz(&o)], 1);
        let (best, n, amb) = best_authz(&mut m);
        assert_eq!((best, n, amb), (Some(authz(&v)), 3, false));
    }

    #[test]
    fn authz_reports_rejection_reasons_only_as_counts() {
        let id = "z".repeat(24);
        let long = "1".repeat(25);
        let text = format!(
            "?accountId={id}&nonce=1 ?accountId={ACC}?nonce=1 ?accountId={ACC}&nonce=x ?accountId={ACC}&nonce={long} ?accountId={ACC}&nonce="
        );
        let mut d = AuthzDiagnostics::default();
        assert_eq!(
            scan_buffer_for_authz(text.as_bytes(), &mut HashMap::new(), &mut d),
            5
        );
        assert_eq!(
            d,
            AuthzDiagnostics {
                truncated: 1,
                invalid_account_id: 1,
                missing_nonce: 1,
                empty_nonce: 1,
                nonce_too_long: 1
            }
        );
    }

    #[test]
    fn best_authz_handles_empty_ties_and_a_leader_after_a_lower_tie() {
        assert_eq!(best_authz(&mut HashMap::new()), (None, 0, false));
        let (a, b) = (authz(&valid()), authz(&other()));
        let c = authz("?accountId=aaaaaaaaaaaaaaaaaaaaaaaa&nonce=7");
        let mut tie = HashMap::from([(a.clone(), 2), (b.clone(), 2)]);
        assert_eq!(best_authz(&mut tie), (None, 2, true));
        let mut lead = HashMap::from([(a, 2), (b, 2), (c.clone(), 3)]);
        assert_eq!(best_authz(&mut lead), (Some(c), 3, false));
    }

    #[test]
    fn authz_debug_is_redacted() {
        let a = authz(&valid());
        assert_eq!(format!("{a:?}"), "GameAuthz(<redacted>)");
        let read = read_authz_from(Ok(vec![authz_reader(&valid(), 0)]));
        let shown = format!("{read:?}");
        assert!(!shown.contains(ACC) && !shown.contains("1712345678901"));
    }

    fn authz_reader(text: &str, base: u64) -> Box<dyn ProcessMemoryReader> {
        let mut contents = vec![0u8; 128];
        contents[..text.len()].copy_from_slice(text.as_bytes());
        Box::new(Fake::new(vec![FakeRegion {
            base,
            contents,
            kind: MEM_PRIVATE,
        }]))
    }

    #[test]
    fn reads_authz_across_regions_and_every_process() {
        let v = valid();
        let empty = FakeRegion {
            base: 0,
            contents: vec![0; 128],
            kind: MEM_PRIVATE,
        };
        let mut contents = vec![0u8; 128];
        contents[..v.len()].copy_from_slice(v.as_bytes());
        let two = Box::new(Fake::new(vec![
            empty,
            FakeRegion {
                base: 0x1000,
                contents,
                kind: MEM_PRIVATE,
            },
        ]));
        for readers in [
            vec![two as Box<dyn ProcessMemoryReader>],
            vec![authz_reader("?accountId=invalid", 0), authz_reader(&v, 0)],
        ] {
            let read = read_authz_from(Ok(readers));
            assert_eq!(read.status, GameAuthzStatus::Ok);
            assert_eq!(
                (read.hits, read.authz.unwrap().query().as_str()),
                (1, v.as_str())
            );
        }
    }

    #[test]
    fn authz_status_reports_failures_ambiguity_and_absence() {
        assert_eq!(
            read_authz_from(Err(OpenFailure::AccessDenied)).status,
            GameAuthzStatus::AccessDenied
        );
        assert_eq!(
            read_authz_from(Ok(vec![authz_reader("nothing", 0)])).status,
            GameAuthzStatus::CrumbsNotFound
        );
        let both = format!("{} {}", valid(), other());
        assert_eq!(
            read_authz_from(Ok(vec![authz_reader(&both, 0)])).status,
            GameAuthzStatus::CrumbsAmbiguous
        );
    }
}
