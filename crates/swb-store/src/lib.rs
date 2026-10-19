//! SQLite coordination state. Mutations and their receipts commit in one writer
//! transaction (SWB-R02, SWB-R09, SWB-R14, SWB-R46).
//!
//! Every time the store reads or writes comes from an injected [`Clock`]
//! (R-C262). Production uses [`SystemClock`], which reads the same Unix
//! seconds SQLite `unixepoch()` returned before the seam existed. Tests and
//! the spec live adapter use [`ManualClock`], which moves only when told to.
//!
//! v0.3.0 (R-C944): `register` returns a fresh end token (each register
//! invalidates the last) that only `end` accepts, `peers` lists live
//! sessions by default (bounded, paginated, one row per harness, host and
//! session id) and never returns `proc_start`, and retention
//! pruning runs on the broker's interval instead of inside `send`/`inbox`.

use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::Sha256;
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use ulid::Ulid;

pub const RETENTION_ACKED_DAYS: u32 = 7;
pub const RETENTION_UNACKED_DAYS: u32 = 30;
pub const DEFAULT_TTL_HOURS: u32 = 72;
pub const MAX_TTL_DAYS: u32 = 14;
/// Lease length `register` reports, and the live band of `peers`.
pub const LEASE_SECONDS: i64 = 900;
/// Upper bound of the idle band; older non-ended sessions are gone.
pub const IDLE_MAX_AGE_SECONDS: i64 = 21_600;
/// `peers` page size when the caller names none, and the largest it accepts.
pub const PEERS_DEFAULT_LIMIT: u32 = 50;
pub const PEERS_MAX_LIMIT: u32 = 200;
/// Hex length of an end token (an HMAC-SHA256).
pub const END_TOKEN_HEX_LEN: usize = 64;

pub fn effective_ttl_hours(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_TTL_HOURS)
        .clamp(1, MAX_TTL_DAYS * 24)
}

/// What kind of failure a store operation hit. The broker maps each kind to
/// one HTTP status and a stable machine-readable `code` (R-C944).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// The request is malformed or names an invalid value.
    Invalid,
    /// `me` names no registered session.
    Unregistered,
    /// The message does not exist for this caller, or has expired.
    Unavailable,
    /// A `msg_id` was reused with a different sender or body (SWB-R46).
    Conflict,
    /// The store itself failed; not the caller's fault.
    Internal,
}

impl ErrorKind {
    /// The stable `code` the REST and MCP error bodies carry.
    pub fn code(self) -> &'static str {
        match self {
            Self::Invalid => "invalid_argument",
            Self::Unregistered => "unregistered",
            Self::Unavailable => "message_unavailable",
            Self::Conflict => "msg_id_conflict",
            Self::Internal => "internal",
        }
    }
}

/// A store failure: a kind plus a short fixed message that never contains a
/// message body or a stored receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError {
    pub kind: ErrorKind,
    pub message: String,
}

impl StoreError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn code(&self) -> &'static str {
        self.kind.code()
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

/// A bare message is a validation failure; every other kind is explicit.
impl From<&str> for StoreError {
    fn from(message: &str) -> Self {
        Self::new(ErrorKind::Invalid, message)
    }
}

impl From<String> for StoreError {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Invalid, message)
    }
}

impl PartialEq<&str> for StoreError {
    fn eq(&self, other: &&str) -> bool {
        self.message == *other
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Source of the current time, in whole seconds since the Unix epoch
/// (R-C262). The store asks it once per operation and binds the answer into
/// its SQL, so no statement reads SQLite's own clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}

/// The wall clock. Truncated Unix seconds, the value SQLite `unixepoch()`
/// returns, so the store's production behaviour is unchanged by the seam.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64)
    }
}

/// A clock that stands still until [`ManualClock::advance`] moves it. It
/// never moves backwards. For tests and the spec live adapter only; no
/// production path constructs one.
#[derive(Debug)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(start: i64) -> Self {
        Self(AtomicI64::new(start))
    }
    /// Start at the current wall-clock second.
    pub fn starting_now() -> Self {
        Self::new(SystemClock.now())
    }
    /// Move forward by `seconds` and return the new time.
    pub fn advance(&self, seconds: u32) -> i64 {
        let step = i64::from(seconds);
        self.0.fetch_add(step, Ordering::SeqCst) + step
    }
}

impl Clock for ManualClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

impl<C: Clock + ?Sized> Clock for Arc<C> {
    fn now(&self) -> i64 {
        (**self).now()
    }
}

pub struct Store(Mutex<Connection>, Arc<dyn Clock>);
#[derive(Debug)]
pub struct SendOutcome {
    pub envelope: Value,
    pub created: bool,
}

/// A `peers` request (R-C944). The default lists live sessions only, one
/// row per (harness, host, session id), at most [`PEERS_DEFAULT_LIMIT`] rows.
#[derive(Debug, Default, Clone, Copy)]
pub struct PeersQuery<'a> {
    /// A registered caller whose own lease this call refreshes first.
    pub me: Option<&'a str>,
    /// Include idle, gone and ended sessions too.
    pub all: bool,
    /// Only sessions on this host.
    pub host: Option<&'a str>,
    /// Page size; clamped to 1..=[`PEERS_MAX_LIMIT`].
    pub limit: Option<u32>,
    /// The `next_cursor` of the previous page.
    pub cursor: Option<&'a str>,
    /// Only sessions whose latest registration used this `proc_start`. The
    /// caller supplies it; listings still never return it.
    pub proc_start: Option<&'a str>,
}

fn field<'a>(v: &'a Value, key: &str) -> StoreResult<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {key}").into())
}
fn valid_part(p: &str) -> bool {
    !p.is_empty()
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}
fn valid_agent_id(id: &str) -> bool {
    if id.len() > 512 {
        return false;
    }
    let parts: Vec<_> = id.split(':').collect();
    parts.len() == 4
        && matches!(
            parts[0],
            "claude" | "kimi" | "codex" | "junie" | "opencode" | "pi"
        )
        && parts[2].parse::<u32>().is_ok()
        && parts[1..].iter().all(|p| valid_part(p))
}
fn valid_ulid(s: &str) -> bool {
    s.len() == 26 && s.parse::<Ulid>().is_ok_and(|id| id.to_string() == s)
}
fn valid_rfc3339_shape(s: &str) -> bool {
    let b = s.as_bytes();
    if !(20..=40).contains(&b.len())
        || b.get(4) != Some(&b'-')
        || b.get(7) != Some(&b'-')
        || b.get(10) != Some(&b'T')
        || b.get(13) != Some(&b':')
        || b.get(16) != Some(&b':')
        || [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .any(|&i| !b[i].is_ascii_digit())
    {
        return false;
    }
    let mut suffix = &b[19..];
    if suffix.first() == Some(&b'.') {
        suffix = &suffix[1..];
        let digits = suffix.iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return false;
        }
        suffix = &suffix[digits..];
    }
    suffix == b"Z"
        || (suffix.len() == 6
            && matches!(suffix[0], b'+' | b'-')
            && suffix[1].is_ascii_digit()
            && suffix[2].is_ascii_digit()
            && suffix[3] == b':'
            && suffix[4].is_ascii_digit()
            && suffix[5].is_ascii_digit())
}
fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}
fn sql_err(e: rusqlite::Error) -> StoreError {
    StoreError::new(ErrorKind::Internal, e.to_string())
}
fn random_bytes<const N: usize>() -> StoreResult<[u8; N]> {
    let mut bytes = [0u8; N];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
        .map_err(|e| StoreError::new(ErrorKind::Internal, format!("randomness: {e}")))?;
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .filter(|pair| pair.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}
/// The session id is the fourth part of an agent id.
fn session_of(agent_id: &str) -> &str {
    agent_id.splitn(4, ':').nth(3).unwrap_or(agent_id)
}
fn lease_state(seen: i64, ended: bool, now: i64) -> &'static str {
    if ended {
        "ended"
    } else if seen >= now - LEASE_SECONDS {
        "live"
    } else if seen >= now - IDLE_MAX_AGE_SECONDS {
        "idle"
    } else {
        "gone"
    }
}

impl Store {
    /// The store's current time, in Unix seconds.
    pub fn now(&self) -> i64 {
        self.1.now()
    }
    fn db(&self) -> StoreResult<MutexGuard<'_, Connection>> {
        self.0
            .lock()
            .map_err(|_| StoreError::new(ErrorKind::Internal, "store lock poisoned"))
    }
    /// Retention (SWB-R09): acked rows after 7 days, every other row 30 days
    /// after creation. The broker runs this at startup and on its interval;
    /// `send` and `inbox` no longer do (R-C944).
    pub fn prune(&self) -> StoreResult<()> {
        let db = self.db()?;
        let now = self.now();
        db.execute("DELETE FROM messages WHERE (state='acked' AND acked_at<=?1-604800) OR (state!='acked' AND created_at<=?1-2592000)", [now]).map_err(sql_err)?;
        Ok(())
    }
    /// A cheap read that proves the store answers (`/healthz`).
    pub fn health(&self) -> StoreResult<()> {
        let db = self.db()?;
        db.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .map_err(sql_err)?;
        Ok(())
    }
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        Self::open_with_clock(path, Arc::new(SystemClock))
    }
    pub fn open_with_clock(path: impl AsRef<Path>, clock: Arc<dyn Clock>) -> StoreResult<Self> {
        let db = Connection::open(path).map_err(sql_err)?;
        let store = Self::init(db, clock)?;
        store.prune()?;
        Ok(store)
    }
    pub fn memory() -> StoreResult<Self> {
        Self::memory_with_clock(Arc::new(SystemClock))
    }
    pub fn memory_with_clock(clock: Arc<dyn Clock>) -> StoreResult<Self> {
        Self::init(Connection::open_in_memory().map_err(sql_err)?, clock)
    }
    fn init(db: Connection, clock: Arc<dyn Clock>) -> StoreResult<Self> {
        db.busy_timeout(Duration::from_secs(5)).map_err(sql_err)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS sessions (
              agent_id TEXT PRIMARY KEY, harness TEXT NOT NULL, host TEXT NOT NULL,
              proc_start TEXT NOT NULL, last_seen INTEGER NOT NULL, ended INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS messages (
              msg_id TEXT PRIMARY KEY, thread_id TEXT NOT NULL, seq INTEGER NOT NULL,
              sender TEXT NOT NULL, recipient TEXT NOT NULL, body TEXT NOT NULL,
              envelope TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'queued',
              delivery_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL,
              expires_at INTEGER NOT NULL, acked_at INTEGER,
              UNIQUE(thread_id,seq));
            CREATE INDEX IF NOT EXISTS messages_inbox ON messages(recipient,state,created_at);
            CREATE TABLE IF NOT EXISTS counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS thread_counters (thread_id TEXT PRIMARY KEY, high_water INTEGER NOT NULL);
            INSERT INTO thread_counters(thread_id, high_water)
              SELECT thread_id, MAX(seq) FROM messages GROUP BY thread_id
              ON CONFLICT(thread_id) DO UPDATE SET high_water=MAX(high_water, excluded.high_water);
            CREATE TABLE IF NOT EXISTS settings (name TEXT PRIMARY KEY, value BLOB NOT NULL);").map_err(sql_err)?;
        // v0.3.0 (R-C944) session columns, added in place to a v0.2 store:
        // the session id `peers` deduplicates on, the registration sequence
        // that decides which row of a session is latest, and the nonce the
        // end token is derived from. A v0.2 row has no nonce, so it cannot be
        // ended; it decays to gone instead.
        let columns: Vec<String> = db
            .prepare("SELECT name FROM pragma_table_info('sessions')")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()
            })
            .map_err(sql_err)?;
        for (column, ddl) in [
            ("session_id", "ALTER TABLE sessions ADD COLUMN session_id TEXT"),
            (
                "registered_seq",
                "ALTER TABLE sessions ADD COLUMN registered_seq INTEGER NOT NULL DEFAULT 0",
            ),
            ("token_nonce", "ALTER TABLE sessions ADD COLUMN token_nonce BLOB"),
        ] {
            if !columns.iter().any(|c| c == column) {
                db.execute(ddl, []).map_err(sql_err)?;
            }
        }
        let unfilled: Vec<String> = db
            .prepare("SELECT agent_id FROM sessions WHERE session_id IS NULL")
            .and_then(|mut stmt| {
                stmt.query_map([], |r| r.get::<_, String>(0))?
                    .collect::<Result<_, _>>()
            })
            .map_err(sql_err)?;
        for id in unfilled {
            db.execute(
                "UPDATE sessions SET session_id=?2 WHERE agent_id=?1",
                params![id, session_of(&id)],
            )
            .map_err(sql_err)?;
        }
        db.execute_batch(
            "CREATE INDEX IF NOT EXISTS sessions_by_session ON sessions(session_id, registered_seq);",
        )
        .map_err(sql_err)?;
        for name in ["audit_hmac_key", "end_token_key"] {
            let key = random_bytes::<32>()?;
            db.execute(
                "INSERT OR IGNORE INTO settings(name,value) VALUES(?1,?2)",
                params![name, &key[..]],
            )
            .map_err(sql_err)?;
        }
        Ok(Self(Mutex::new(db), clock))
    }
    fn keyed_mac(db: &Connection, key_name: &str) -> StoreResult<Hmac<Sha256>> {
        let key: Vec<u8> = db
            .query_row("SELECT value FROM settings WHERE name=?1", [key_name], |r| {
                r.get(0)
            })
            .map_err(sql_err)?;
        Hmac::<Sha256>::new_from_slice(&key)
            .map_err(|e| StoreError::new(ErrorKind::Internal, e.to_string()))
    }
    pub fn body_hmac(&self, body: &str) -> StoreResult<String> {
        let db = self.db()?;
        let mut mac = Self::keyed_mac(&db, "audit_hmac_key")?;
        mac.update(body.as_bytes());
        Ok(hex(&mac.finalize().into_bytes()))
    }
    /// The end token of one registration: HMAC(end_token_key, agent id,
    /// nonce). Nothing secret is stored beside the key, and a new nonce
    /// (new session row, new `proc_start`, or a register after `end`)
    /// invalidates every earlier token.
    fn end_token_mac(db: &Connection, agent_id: &str, nonce: &[u8]) -> StoreResult<Hmac<Sha256>> {
        let mut mac = Self::keyed_mac(db, "end_token_key")?;
        mac.update(b"swb-end-token-v1\0");
        mac.update(agent_id.as_bytes());
        mac.update(b"\0");
        mac.update(nonce);
        Ok(mac)
    }
    /// Register (or re-register) a self-asserted session and return its
    /// agent id, lease and end token (R-C944). Every registration mints a
    /// fresh token and invalidates every earlier one, so knowing a row's
    /// `proc_start` never retrieves the token its registrant holds. A hook
    /// that registers on every prompt saves the newest token each time.
    pub fn register(&self, input: &Value) -> StoreResult<Value> {
        let harness = field(input, "harness")?;
        if !matches!(
            harness,
            "claude" | "kimi" | "codex" | "junie" | "opencode" | "pi"
        ) {
            return Err("invalid harness".into());
        }
        let host = field(input, "host")?;
        let pid = input
            .get("pid")
            .and_then(Value::as_u64)
            .filter(|p| *p > 0)
            .ok_or("invalid pid")?;
        let session_id = field(input, "session_id")?;
        let proc_start = field(input, "proc_start")?;
        if proc_start.len() > 256 || proc_start.chars().any(char::is_control) {
            return Err("invalid proc_start".into());
        }
        let id = format!("{harness}:{host}:{pid}:{session_id}");
        if !valid_agent_id(&id) {
            return Err("invalid agent id".into());
        }
        let mut db = self.db()?;
        let now = self.now();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        let nonce = random_bytes::<16>()?.to_vec();
        tx.execute("INSERT INTO counters(name,value) VALUES('registrations',1) ON CONFLICT(name) DO UPDATE SET value=value+1",[]).map_err(sql_err)?;
        let seq: i64 = tx
            .query_row(
                "SELECT value FROM counters WHERE name='registrations'",
                [],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        tx.execute("INSERT INTO sessions(agent_id,harness,host,proc_start,last_seen,session_id,registered_seq,token_nonce) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
                    ON CONFLICT(agent_id) DO UPDATE SET proc_start=excluded.proc_start,last_seen=excluded.last_seen,ended=0,
                      session_id=excluded.session_id,registered_seq=excluded.registered_seq,token_nonce=excluded.token_nonce",
            params![id,harness,host,proc_start,now,session_id,seq,nonce]).map_err(sql_err)?;
        let token = hex(&Self::end_token_mac(&tx, &id, &nonce)?
            .finalize()
            .into_bytes());
        tx.commit().map_err(sql_err)?;
        Ok(json!({"agent_id":id,"lease_seconds":LEASE_SECONDS,"end_token":token}))
    }
    /// End a session. Only the end token its latest registration returned
    /// ends it; any other token, or an unknown session, answers `unknown`
    /// without saying which (R-C944).
    pub fn end(&self, me: &str, end_token: &str) -> StoreResult<Value> {
        if !valid_agent_id(me) {
            return Err("invalid me".into());
        }
        if end_token.len() != END_TOKEN_HEX_LEN {
            return Err("invalid end_token".into());
        }
        let presented = unhex(end_token).ok_or("invalid end_token")?;
        let db = self.db()?;
        let nonce: Option<Option<Vec<u8>>> = db
            .query_row(
                "SELECT token_nonce FROM sessions WHERE agent_id=?1",
                [me],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let matches = match nonce.flatten() {
            Some(nonce) => Self::end_token_mac(&db, me, &nonce)?
                .verify_slice(&presented)
                .is_ok(),
            None => false,
        };
        if matches {
            db.execute("UPDATE sessions SET ended=1 WHERE agent_id=?1", [me])
                .map_err(sql_err)?;
        }
        Ok(json!({"agent_id":me,"state":if matches {"ended"} else {"unknown"}}))
    }
    /// List sessions (R-C944): live only unless `all`, one row per
    /// (harness, host, session id) (the latest registration wins), ordered
    /// by agent id, paginated by `cursor`. Rows never carry `proc_start`; a
    /// caller may filter on one it already knows. A registered `me`
    /// refreshes its own lease first; listing alone refreshes nothing.
    pub fn peers(&self, query: &PeersQuery) -> StoreResult<Value> {
        if let Some(host) = query.host
            && !valid_part(host)
        {
            return Err("invalid host".into());
        }
        if let Some(cursor) = query.cursor
            && !valid_agent_id(cursor)
        {
            return Err("invalid cursor".into());
        }
        if let Some(proc_start) = query.proc_start
            && (proc_start.is_empty()
                || proc_start.len() > 256
                || proc_start.chars().any(char::is_control))
        {
            return Err("invalid proc_start".into());
        }
        let limit = query
            .limit
            .unwrap_or(PEERS_DEFAULT_LIMIT)
            .clamp(1, PEERS_MAX_LIMIT);
        let db = self.db()?;
        let now = self.now();
        if let Some(me) = query.me {
            if !valid_agent_id(me) {
                return Err("invalid me".into());
            }
            let updated = db
                .execute(
                    "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
                    params![me, now],
                )
                .map_err(sql_err)?;
            if updated == 0 {
                return Err(StoreError::new(ErrorKind::Unregistered, "unregistered me"));
            }
        }
        let mut stmt = db
            .prepare(
                "WITH latest AS (
                   SELECT agent_id,harness,host,proc_start,last_seen,ended,
                     ROW_NUMBER() OVER (PARTITION BY harness,host,COALESCE(session_id,agent_id)
                                        ORDER BY registered_seq DESC, agent_id DESC) AS pos
                   FROM sessions)
                 SELECT agent_id,harness,host,last_seen,ended FROM latest
                 WHERE pos=1 AND (?1 IS NULL OR host=?1) AND (?2 IS NULL OR agent_id>?2)
                   AND (?3 OR (ended=0 AND last_seen>=?4-?5))
                   AND (?7 IS NULL OR proc_start=?7)
                 ORDER BY agent_id LIMIT ?6",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(
                params![
                    query.host,
                    query.cursor,
                    query.all,
                    now,
                    LEASE_SECONDS,
                    i64::from(limit) + 1,
                    query.proc_start
                ],
                |r| {
                    let seen: i64 = r.get(3)?;
                    let ended: i64 = r.get(4)?;
                    Ok(json!({"agent_id":r.get::<_,String>(0)?,"harness":r.get::<_,String>(1)?,
                        "host":r.get::<_,String>(2)?,"last_seen":seen,
                        "state":lease_state(seen, ended != 0, now)}))
                },
            )
            .map_err(sql_err)?;
        let mut peers = rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
        let next_cursor = if peers.len() > limit as usize {
            peers.truncate(limit as usize);
            peers.last().map(|row| row["agent_id"].clone())
        } else {
            None
        };
        Ok(json!({"peers":peers,"next_cursor":next_cursor}))
    }
    pub fn send(&self, input: &Value) -> StoreResult<SendOutcome> {
        let sender = field(input, "from")?;
        let recipient = field(input, "to")?;
        if !valid_agent_id(sender) || !valid_agent_id(recipient) {
            return Err("invalid agent id".into());
        }
        let body = clean(
            input
                .get("body")
                .and_then(Value::as_str)
                .ok_or("missing body")?,
        );
        if body.len() > 16384 {
            return Err("body exceeds 16 KiB".into());
        }
        let ticket = field(input, "ticket")?;
        if ticket.len() > 64
            || ticket != "none"
                && (!ticket.starts_with("TIN-")
                    || !ticket[4..].chars().all(|c| c.is_ascii_digit())
                    || ticket.len() == 4)
        {
            return Err("invalid ticket".into());
        }
        let directed = input
            .get("operator_directed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if input
            .get("operator_directed")
            .is_some_and(|v| !v.is_boolean())
        {
            return Err("invalid operator_directed".into());
        }
        if directed
            && input
                .get("ruling")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .is_none()
        {
            return Err("operator_directed requires ruling".into());
        }
        if input
            .get("artifacts")
            .and_then(Value::as_array)
            .is_some_and(|v| v.len() > 20)
        {
            return Err("too many artifacts".into());
        }
        if let Some(artifacts) = input.get("artifacts") {
            let items = artifacts.as_array().ok_or("invalid artifacts")?;
            let mut seen = std::collections::HashSet::new();
            for item in items {
                let reference = item
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 2048)
                    .ok_or("invalid artifact reference")?;
                if !seen.insert(reference) {
                    return Err("duplicate artifact reference".into());
                }
            }
        }
        let input_map = input.as_object().ok_or("expected object")?;
        const ALLOWED: &[&str] = &[
            "from",
            "to",
            "ticket",
            "body",
            "msg_id",
            "thread_id",
            "in_reply_to",
            "operator_directed",
            "ruling",
            "reply_expires",
            "reply_format",
            "artifacts",
            "ttl_hours",
            "authority",
            "v",
        ];
        if input_map.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
            return Err("unknown envelope field".into());
        }
        for key in ["msg_id", "thread_id", "reply_expires"] {
            if input.get(key).is_some_and(|value| !value.is_string()) {
                return Err(format!("invalid {key}"));
            }
        }
        if input.get("ttl_hours").is_some_and(|v| !v.is_u64()) {
            return Err("invalid ttl_hours".into());
        }
        if input
            .get("in_reply_to")
            .is_some_and(|v| !v.as_str().is_some_and(valid_ulid))
        {
            return Err("invalid in_reply_to".into());
        }
        for key in ["ruling", "reply_format"] {
            if input.get(key).is_some_and(|v| {
                !v.as_str().is_some_and(|s| {
                    (key != "ruling" || !s.is_empty())
                        && s.len() <= 512
                        && !s.chars().any(char::is_control)
                })
            }) {
                return Err(format!("invalid {key}"));
            }
        }
        let msg_id = input
            .get("msg_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| Ulid::new().to_string());
        let thread_id = input
            .get("thread_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| msg_id.clone());
        if !valid_ulid(&msg_id) || !valid_ulid(&thread_id) {
            return Err("invalid ULID".into());
        }
        let ttl = effective_ttl_hours(
            input
                .get("ttl_hours")
                .and_then(Value::as_u64)
                .map(|n| n.min(u32::MAX as u64) as u32),
        );
        let mut db = self.db()?;
        let now = self.now();
        if let Some(value) = input.get("reply_expires") {
            let stamp = value
                .as_str()
                .filter(|s| valid_rfc3339_shape(s))
                .ok_or("invalid reply_expires")?;
            let parsed: Option<i64> = db
                .query_row("SELECT unixepoch(?1)", [stamp], |row| row.get(0))
                .map_err(sql_err)?;
            if parsed.is_none() {
                return Err("invalid reply_expires".into());
            }
        }
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        let old: Option<(String, String, String)> = tx
            .query_row(
                "SELECT sender,body,envelope FROM messages WHERE msg_id=?1",
                [&msg_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(sql_err)?;
        if let Some((old_sender, old_body, envelope)) = old {
            // SWB-R46: disclose neither the stored receipt nor its metadata on conflict.
            if old_sender != sender || old_body != body {
                return Err(StoreError::new(ErrorKind::Conflict, "msg_id conflict"));
            }
            let envelope = serde_json::from_str(&envelope)
                .map_err(|e| StoreError::new(ErrorKind::Internal, e.to_string()))?;
            tx.execute(
                "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
                params![sender, now],
            )
            .map_err(sql_err)?;
            tx.commit().map_err(sql_err)?;
            return Ok(SendOutcome {
                envelope,
                created: false,
            });
        }
        tx.execute(
            "INSERT INTO thread_counters(thread_id,high_water) VALUES(?1,1)
             ON CONFLICT(thread_id) DO UPDATE SET high_water=high_water+1",
            [&thread_id],
        )
        .map_err(sql_err)?;
        let seq: i64 = tx
            .query_row(
                "SELECT high_water FROM thread_counters WHERE thread_id=?1",
                [&thread_id],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        let mut envelope = json!({
            "from":sender,"to":recipient,"ticket":ticket,"body":body
        });
        let map = envelope.as_object_mut().ok_or("expected object")?;
        for key in [
            "in_reply_to",
            "ruling",
            "reply_expires",
            "reply_format",
            "artifacts",
        ] {
            if let Some(value) = input.get(key) {
                map.insert(key.into(), value.clone());
            }
        }
        map.insert("v".into(), json!(3));
        map.insert("msg_id".into(), json!(msg_id));
        map.insert("thread_id".into(), json!(thread_id));
        map.insert("seq".into(), json!(seq));
        map.insert("authority".into(), json!("peer"));
        map.insert("transport".into(), json!("broker"));
        map.insert("body".into(), json!(body));
        map.insert("operator_directed".into(), json!(directed));
        map.insert("reply_to".into(), json!(format!("ag:{sender}")));
        map.remove("ttl_hours");
        let sent_at: String = tx
            .query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch')",
                [now],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        map.insert("sent_at".into(), json!(sent_at));
        let expires_at: String = tx
            .query_row(
                "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch',?2)",
                params![now, format!("+{} seconds", i64::from(ttl) * 3600)],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        map.insert("expires_at".into(), json!(expires_at));
        tx.execute("INSERT INTO messages(msg_id,thread_id,seq,sender,recipient,body,envelope,created_at,expires_at)
                    VALUES(?1,?2,?3,?4,?5,?6,?7,?9,?9+?8)",
            params![msg_id,thread_id,seq,sender,recipient,body,envelope.to_string(),i64::from(ttl)*3600,now]).map_err(sql_err)?;
        tx.execute("INSERT INTO counters(name,value) VALUES('messages_total',1) ON CONFLICT(name) DO UPDATE SET value=value+1",[]).map_err(sql_err)?;
        tx.execute(
            "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
            params![sender, now],
        )
        .map_err(sql_err)?;
        tx.commit().map_err(sql_err)?;
        Ok(SendOutcome {
            envelope,
            created: true,
        })
    }
    pub fn inbox(&self, me: &str, limit: u32) -> StoreResult<Value> {
        if !valid_agent_id(me) {
            return Err("invalid agent id".into());
        }
        let mut db = self.db()?;
        let now = self.now();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        tx.execute("UPDATE messages SET state='expired' WHERE state!='acked' AND state!='expired' AND expires_at<=?1",[now]).map_err(sql_err)?;
        let messages: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT msg_id,envelope FROM messages WHERE recipient=?1 AND state IN ('queued','notified','fetched') ORDER BY created_at,seq LIMIT ?2").map_err(sql_err)?;
            let rows = stmt
                .query_map(params![me, limit.min(100)], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql_err)?;
            rows.collect::<Result<_, _>>().map_err(sql_err)?
        };
        for (id, _) in &messages {
            tx.execute("UPDATE messages SET state='fetched',delivery_count=delivery_count+1 WHERE msg_id=?1",[id]).map_err(sql_err)?;
        }
        tx.execute(
            "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
            params![me, now],
        )
        .map_err(sql_err)?;
        let values = messages
            .into_iter()
            .map(|(id, body)| {
                let mut v: Value = serde_json::from_str(&body)
                    .map_err(|e| StoreError::new(ErrorKind::Internal, e.to_string()))?;
                let count: i64 = tx
                    .query_row(
                        "SELECT delivery_count FROM messages WHERE msg_id=?1",
                        [&id],
                        |r| r.get(0),
                    )
                    .map_err(sql_err)?;
                v["delivery_count"] = json!(count);
                Ok(v)
            })
            .collect::<StoreResult<Vec<_>>>()?;
        tx.commit().map_err(sql_err)?;
        Ok(json!({"messages":values}))
    }
    pub fn ack(&self, me: &str, msg_id: &str) -> StoreResult<Value> {
        if !valid_agent_id(me) || !valid_ulid(msg_id) {
            return Err("invalid identity or msg_id".into());
        }
        let db = self.db()?;
        let now = self.now();
        let n = db.execute("UPDATE messages SET state='acked',acked_at=?3 WHERE msg_id=?1 AND recipient=?2 AND expires_at>?3 AND state IN ('queued','notified','fetched')",params![msg_id,me,now]).map_err(sql_err)?;
        if n == 0 {
            let prior: Option<String> = db
                .query_row(
                    "SELECT state FROM messages WHERE msg_id=?1 AND recipient=?2",
                    params![msg_id, me],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_err)?;
            if prior.as_deref() != Some("acked") {
                return Err(StoreError::new(
                    ErrorKind::Unavailable,
                    "message unavailable",
                ));
            }
        }
        db.execute(
            "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
            params![me, now],
        )
        .map_err(sql_err)?;
        Ok(json!({"msg_id":msg_id,"state":"acked"}))
    }
    pub fn metrics(&self) -> StoreResult<String> {
        let db = self.db()?;
        let now = self.now();
        let sent: i64 = db
            .query_row(
                "SELECT COALESCE(value,0) FROM counters WHERE name='messages_total'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .unwrap_or(0);
        let unacked: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE state NOT IN ('acked','expired') AND expires_at>?1",
                [now],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        let live: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE ended=0 AND last_seen>=?1-900",
                [now],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        Ok(format!(
            "# TYPE swb_messages_total counter\nswb_messages_total {sent}\n# TYPE swb_mailbox_unacked gauge\nswb_mailbox_unacked {unacked}\n# TYPE swb_sessions gauge\nswb_sessions{{state=\"live\"}} {live}\n"
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// 2023-11-14T22:13:20Z.
    const T0: i64 = 1_700_000_000;

    fn clocked() -> (Store, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock::new(T0));
        (Store::memory_with_clock(clock.clone()).unwrap(), clock)
    }

    fn all() -> PeersQuery<'static> {
        PeersQuery {
            all: true,
            ..PeersQuery::default()
        }
    }

    fn refresh(me: &str) -> PeersQuery<'_> {
        PeersQuery {
            me: Some(me),
            all: true,
            ..PeersQuery::default()
        }
    }

    fn reg(s: &Store, harness: &str, host: &str, pid: u32, session: &str, proc_start: &str) -> Value {
        s.register(&json!({"harness":harness,"host":host,"pid":pid,"session_id":session,"proc_start":proc_start}))
            .unwrap()
    }

    fn row_count(s: &Store) -> i64 {
        s.0.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn system_clock_matches_sqlite_unixepoch() {
        let db = Connection::open_in_memory().unwrap();
        let before = SystemClock.now();
        let sqlite: i64 = db
            .query_row("SELECT unixepoch()", [], |r| r.get(0))
            .unwrap();
        let after = SystemClock.now();
        assert!(before <= sqlite && sqlite <= after);
        assert!((Store::memory().unwrap().now() - SystemClock.now()).abs() <= 2);
    }

    #[test]
    fn manual_clock_moves_only_forward_and_only_when_told() {
        let clock = ManualClock::new(T0);
        assert_eq!(clock.now(), T0);
        assert_eq!(clock.now(), T0);
        assert_eq!(clock.advance(0), T0);
        assert_eq!(clock.advance(901), T0 + 901);
        assert_eq!(clock.advance(u32::MAX), T0 + 901 + i64::from(u32::MAX));
        let shared: Arc<dyn Clock> = Arc::new(ManualClock::new(5));
        assert_eq!(shared.now(), 5);
    }

    #[test]
    fn lease_bands_follow_the_injected_clock() {
        let (s, clock) = clocked();
        let me = s.register(&json!({"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"1"})).unwrap()["agent_id"].as_str().unwrap().to_owned();
        let state = |s: &Store| s.peers(&all()).unwrap()["peers"][0]["state"].clone();
        assert_eq!(s.peers(&all()).unwrap()["peers"][0]["last_seen"], T0);
        clock.advance(900);
        assert_eq!(state(&s), "live");
        clock.advance(1);
        assert_eq!(state(&s), "idle");
        clock.advance(21600 - 901);
        assert_eq!(state(&s), "idle");
        assert!(
            s.metrics()
                .unwrap()
                .contains("swb_sessions{state=\"live\"} 0\n")
        );
        clock.advance(1);
        assert_eq!(state(&s), "gone");
        let renewed = s.peers(&refresh(&me)).unwrap();
        assert_eq!(renewed["peers"][0]["state"], "live");
        assert_eq!(renewed["peers"][0]["last_seen"], T0 + 21601);
        assert!(
            s.metrics()
                .unwrap()
                .contains("swb_sessions{state=\"live\"} 1\n")
        );
    }

    #[test]
    fn envelope_times_and_expiry_follow_the_injected_clock() {
        let (s, clock) = clocked();
        let to = "pi:sting:2:b";
        let sent = s
            .send(&json!({"from":"claude:honey:1:a","to":to,"ticket":"none","body":"hi","ttl_hours":1}))
            .unwrap();
        assert_eq!(sent.envelope["sent_at"], "2023-11-14T22:13:20Z");
        assert_eq!(sent.envelope["expires_at"], "2023-11-14T23:13:20Z");
        let id = sent.envelope["msg_id"].as_str().unwrap().to_owned();
        clock.advance(3599);
        assert_eq!(
            s.inbox(to, 10).unwrap()["messages"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 1\n"));
        clock.advance(1);
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 0\n"));
        assert!(
            s.inbox(to, 10).unwrap()["messages"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(s.ack(to, &id).unwrap_err(), "message unavailable");
    }

    #[test]
    fn retention_follows_the_injected_clock() {
        let (s, clock) = clocked();
        let from = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        let thread = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let send = |s: &Store, body: &str| {
            s.send(&json!({"from":from,"to":to,"ticket":"none","body":body,"thread_id":thread}))
                .unwrap()
                .envelope
        };
        let acked = send(&s, "acked")["msg_id"].as_str().unwrap().to_owned();
        send(&s, "unacked");
        clock.advance(10);
        s.ack(to, &acked).unwrap();
        clock.advance(604_799);
        s.prune().unwrap();
        assert_eq!(row_count(&s), 2);
        clock.advance(1);
        s.prune().unwrap();
        assert_eq!(row_count(&s), 1);
        clock.advance(2_592_000 - 604_810);
        s.prune().unwrap();
        assert_eq!(row_count(&s), 0);
        assert_eq!(send(&s, "after prune")["seq"], 3);
    }

    #[test]
    fn ttl_defaults_and_clamps() {
        assert_eq!(effective_ttl_hours(None), 72);
        assert_eq!(effective_ttl_hours(Some(10_000)), 336);
        assert_eq!(effective_ttl_hours(Some(0)), 1);
    }
    #[test]
    fn end_requires_the_latest_registration_token() {
        let s = Store::memory().unwrap();
        let first = reg(&s, "claude", "honey", 10, "same", "start-1");
        let me = first["agent_id"].as_str().unwrap();
        let token = first["end_token"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), END_TOKEN_HEX_LEN);
        // The listing never carries proc_start or a token.
        let listed = s.peers(&all()).unwrap().to_string();
        assert!(!listed.contains("start-1") && !listed.contains(&token), "{listed}");
        assert!(!listed.contains("proc_start"), "{listed}");
        // A forged or malformed token ends nothing, and says nothing.
        let forged = "0".repeat(END_TOKEN_HEX_LEN);
        assert_eq!(s.end(me, &forged).unwrap()["state"], "unknown");
        assert_eq!(s.end(me, "start-1").unwrap_err(), "invalid end_token");
        assert_eq!(s.end(me, &token.to_uppercase()).unwrap_err(), "invalid end_token");
        assert_eq!(s.end("codex:honey:9:none", &token).unwrap()["state"], "unknown");
        assert_eq!(s.peers(&all()).unwrap()["peers"][0]["state"], "live");
        // Every register rotates the token, even with the same proc_start,
        // so knowing proc_start never retrieves the live token.
        let again = reg(&s, "claude", "honey", 10, "same", "start-1");
        assert_ne!(again["end_token"], token.as_str());
        assert_eq!(s.end(me, &token).unwrap()["state"], "unknown");
        let rotated = reg(&s, "claude", "honey", 10, "same", "start-2");
        assert_ne!(rotated["end_token"], again["end_token"]);
        assert_eq!(s.end(me, again["end_token"].as_str().unwrap()).unwrap()["state"], "unknown");
        let current = rotated["end_token"].as_str().unwrap().to_owned();
        assert_eq!(s.end(me, &current).unwrap()["state"], "ended");
        assert_eq!(s.peers(&all()).unwrap()["peers"][0]["state"], "ended");
        // After end, a register mints a fresh token even with the same proc_start.
        let revived = reg(&s, "claude", "honey", 10, "same", "start-2");
        assert_ne!(revived["end_token"], current.as_str());
        assert_eq!(s.end(me, &current).unwrap()["state"], "unknown");
    }

    #[test]
    fn peers_defaults_to_live_bounded_and_one_row_per_session() {
        let (s, clock) = clocked();
        for pid in 1..=5 {
            reg(&s, "claude", "honey", pid, &format!("s{pid}"), "p");
        }
        clock.advance(901);
        reg(&s, "pi", "sting", 7, "fresh", "p");
        // An older registration of the same session, from another process
        // of the same harness on the same host.
        reg(&s, "codex", "sting", 8, "shared", "p");
        reg(&s, "codex", "sting", 9, "shared", "p");
        // A fixed session label (a Junie IDE channel) on another host, and
        // another harness on the same host, are different sessions.
        reg(&s, "codex", "honey", 10, "shared", "p");
        reg(&s, "junie", "sting", 11, "shared", "p");
        let ids_of = |page: &Value| -> Vec<String> {
            page["peers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p["agent_id"].as_str().unwrap().to_owned())
                .collect()
        };
        let live = s.peers(&PeersQuery::default()).unwrap();
        assert_eq!(
            ids_of(&live),
            vec![
                "codex:honey:10:shared",
                "codex:sting:9:shared",
                "junie:sting:11:shared",
                "pi:sting:7:fresh"
            ]
        );
        assert_eq!(live["next_cursor"], Value::Null);
        // The latest registration of a session wins, whichever is newer.
        reg(&s, "codex", "sting", 8, "shared", "p");
        assert_eq!(
            ids_of(&s.peers(&PeersQuery::default()).unwrap()),
            vec![
                "codex:honey:10:shared",
                "codex:sting:8:shared",
                "junie:sting:11:shared",
                "pi:sting:7:fresh"
            ]
        );
        // A host filter never hides that host's own row behind another
        // host's registration of the same label.
        assert_eq!(
            ids_of(
                &s.peers(&PeersQuery {
                    host: Some("honey"),
                    ..PeersQuery::default()
                })
                .unwrap()
            ),
            vec!["codex:honey:10:shared"]
        );
        // A caller can filter on a proc_start it knows; rows never show it.
        reg(&s, "claude", "honey", 12, "own", "start-x");
        assert_eq!(
            ids_of(
                &s.peers(&PeersQuery {
                    all: true,
                    host: Some("honey"),
                    proc_start: Some("start-x"),
                    ..PeersQuery::default()
                })
                .unwrap()
            ),
            vec!["claude:honey:12:own"]
        );
        assert_eq!(
            s.peers(&PeersQuery {
                proc_start: Some(""),
                ..PeersQuery::default()
            })
            .unwrap_err(),
            "invalid proc_start"
        );
        // all=true pages through every session, idle ones included.
        let mut cursor: Option<String> = None;
        let mut seen = Vec::new();
        loop {
            let page = s
                .peers(&PeersQuery {
                    all: true,
                    limit: Some(2),
                    cursor: cursor.as_deref(),
                    ..PeersQuery::default()
                })
                .unwrap();
            let rows = page["peers"].as_array().unwrap();
            assert!(rows.len() <= 2);
            seen.extend(rows.iter().map(|p| p["agent_id"].as_str().unwrap().to_owned()));
            match page["next_cursor"].as_str() {
                Some(next) => cursor = Some(next.to_owned()),
                None => break,
            }
        }
        assert_eq!(seen.len(), 10, "{seen:?}");
        assert!(seen.windows(2).all(|w| w[0] < w[1]));
        let sting = s
            .peers(&PeersQuery {
                all: true,
                host: Some("sting"),
                ..PeersQuery::default()
            })
            .unwrap();
        assert_eq!(sting["peers"].as_array().unwrap().len(), 3);
        let bounded = s
            .peers(&PeersQuery {
                all: true,
                limit: Some(10_000),
                ..PeersQuery::default()
            })
            .unwrap();
        assert_eq!(bounded["peers"].as_array().unwrap().len(), 10);
        assert_eq!(
            s.peers(&PeersQuery {
                cursor: Some("not an id"),
                ..PeersQuery::default()
            })
            .unwrap_err(),
            "invalid cursor"
        );
        assert_eq!(
            s.peers(&PeersQuery {
                host: Some("a/b"),
                ..PeersQuery::default()
            })
            .unwrap_err(),
            "invalid host"
        );
    }

    #[test]
    fn a_v02_store_gains_the_new_columns_and_its_rows_cannot_be_ended() {
        let path = std::env::temp_dir().join(format!("swb-migrate-{}.sqlite3", Ulid::new()));
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("CREATE TABLE sessions (
                agent_id TEXT PRIMARY KEY, harness TEXT NOT NULL, host TEXT NOT NULL,
                proc_start TEXT NOT NULL, last_seen INTEGER NOT NULL, ended INTEGER NOT NULL DEFAULT 0);
                INSERT INTO sessions VALUES('claude:honey:1:old','claude','honey','p',unixepoch(),0);").unwrap();
        }
        let s = Store::open(&path).unwrap();
        let listed = s.peers(&PeersQuery::default()).unwrap();
        assert_eq!(listed["peers"][0]["agent_id"], "claude:honey:1:old");
        let session: String =
            s.0.lock()
                .unwrap()
                .query_row("SELECT session_id FROM sessions", [], |r| r.get(0))
                .unwrap();
        assert_eq!(session, "old");
        assert_eq!(
            s.end("claude:honey:1:old", &"a".repeat(64)).unwrap()["state"],
            "unknown"
        );
        drop(s);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn send_and_inbox_leave_retention_to_the_interval() {
        let (s, clock) = clocked();
        let to = "pi:sting:2:b";
        let id = s
            .send(&json!({"from":"claude:honey:1:a","to":to,"ticket":"none","body":"x"}))
            .unwrap()
            .envelope["msg_id"]
            .as_str()
            .unwrap()
            .to_owned();
        s.ack(to, &id).unwrap();
        clock.advance(604_800);
        s.send(&json!({"from":"claude:honey:1:a","to":to,"ticket":"none","body":"y"}))
            .unwrap();
        s.inbox(to, 10).unwrap();
        assert_eq!(row_count(&s), 2, "send or inbox pruned");
        s.prune().unwrap();
        assert_eq!(row_count(&s), 1);
    }

    #[test]
    fn errors_carry_a_kind_and_a_code() {
        let s = Store::memory().unwrap();
        assert_eq!(
            s.ack("pi:sting:2:b", "01ARZ3NDEKTSV4RRFFQ69G5FAV")
                .unwrap_err()
                .code(),
            "message_unavailable"
        );
        assert_eq!(s.inbox("nope", 1).unwrap_err().code(), "invalid_argument");
        let input = json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"a","msg_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"});
        s.send(&input).unwrap();
        let mut other = input;
        other["body"] = json!("b");
        assert_eq!(s.send(&other).unwrap_err().code(), "msg_id_conflict");
    }
    #[test]
    fn thread_sequence_survives_pruned_latest_ack() {
        let s = Store::memory().unwrap();
        let from = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        let thread = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let first = s
            .send(&json!({"from":from,"to":to,"ticket":"none","body":"one","thread_id":thread}))
            .unwrap();
        let second = s
            .send(&json!({"from":from,"to":to,"ticket":"none","body":"two","thread_id":thread}))
            .unwrap();
        assert_eq!(first.envelope["seq"], 1);
        assert_eq!(second.envelope["seq"], 2);
        let id = second.envelope["msg_id"].as_str().unwrap();
        s.ack(to, id).unwrap();
        s.0.lock()
            .unwrap()
            .execute(
                "UPDATE messages SET acked_at=unixepoch()-604801 WHERE msg_id=?1",
                [id],
            )
            .unwrap();
        s.prune().unwrap();
        let third = s
            .send(&json!({"from":from,"to":to,"ticket":"none","body":"three","thread_id":thread}))
            .unwrap();
        assert_eq!(third.envelope["seq"], 3);
    }

    #[test]
    fn reopening_prunes_old_bodies_without_mailbox_traffic() {
        let path = std::env::temp_dir().join(format!("swb-retention-{}.sqlite3", Ulid::new()));
        let from = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        let retained;
        {
            let store = Store::open(&path).unwrap();
            let send = |body| {
                store
                    .send(&json!({"from":from,"to":to,"ticket":"none","body":body}))
                    .unwrap()
                    .envelope["msg_id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            };
            let old_acked = send("old acked body");
            let old_unacked = send("old unacked body");
            retained = send("recent body");
            store.ack(to, &old_acked).unwrap();
            let db = store.0.lock().unwrap();
            db.execute(
                "UPDATE messages SET acked_at=unixepoch()-604801 WHERE msg_id=?1",
                [&old_acked],
            )
            .unwrap();
            db.execute(
                "UPDATE messages SET created_at=unixepoch()-2592001 WHERE msg_id=?1",
                [&old_unacked],
            )
            .unwrap();
        }
        let reopened = Store::open(&path).unwrap();
        let db = reopened.0.lock().unwrap();
        let bodies: Vec<String> = db
            .prepare("SELECT body FROM messages ORDER BY created_at")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(bodies, vec!["recent body"]);
        let surviving: String = db
            .query_row("SELECT msg_id FROM messages", [], |row| row.get(0))
            .unwrap();
        assert_eq!(surviving, retained);
        drop(db);
        drop(reopened);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }

    #[test]
    fn sender_and_reader_refresh_only_their_own_lease() {
        let s = Store::memory().unwrap();
        let sender = s.register(&json!({"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"1"})).unwrap();
        let receiver = s
            .register(
                &json!({"harness":"pi","host":"sting","pid":2,"session_id":"b","proc_start":"2"}),
            )
            .unwrap();
        let from = sender["agent_id"].as_str().unwrap();
        let to = receiver["agent_id"].as_str().unwrap();
        s.0.lock()
            .unwrap()
            .execute("UPDATE sessions SET last_seen=unixepoch()-30000", [])
            .unwrap();
        let sent = s
            .send(&json!({"from":from,"to":to,"ticket":"none","body":"hello"}))
            .unwrap();
        let peers = s.peers(&all()).unwrap();
        assert_eq!(peers["peers"][0]["state"], "live");
        assert_eq!(peers["peers"][1]["state"], "gone");
        s.inbox(to, 1).unwrap();
        let peers = s.peers(&all()).unwrap();
        assert_eq!(peers["peers"][1]["state"], "live");
        s.0.lock()
            .unwrap()
            .execute("UPDATE sessions SET last_seen=unixepoch()-30000", [])
            .unwrap();
        s.ack(to, sent.envelope["msg_id"].as_str().unwrap())
            .unwrap();
        let peers = s.peers(&all()).unwrap();
        assert_eq!(peers["peers"][0]["state"], "gone");
        assert_eq!(peers["peers"][1]["state"], "live");
    }

    #[test]
    fn peers_renews_only_registered_explicit_caller() {
        let s = Store::memory().unwrap();
        let caller = "claude:honey:1:a";
        let other = "pi:sting:2:b";
        for (harness, host, pid, session_id) in
            [("claude", "honey", 1, "a"), ("pi", "sting", 2, "b")]
        {
            s.register(&json!({"harness":harness,"host":host,"pid":pid,"session_id":session_id,"proc_start":"1"})).unwrap();
        }
        s.0.lock()
            .unwrap()
            .execute("UPDATE sessions SET last_seen=unixepoch()-30000", [])
            .unwrap();
        let passive = s.peers(&all()).unwrap();
        assert_eq!(passive["peers"][0]["state"], "gone");
        assert_eq!(passive["peers"][1]["state"], "gone");
        assert_eq!(
            s.peers(&refresh("codex:honey:3:unknown")).unwrap_err(),
            "unregistered me"
        );
        assert_eq!(s.peers(&refresh("invalid")).unwrap_err(), "invalid me");
        assert_eq!(
            s.peers(&refresh("codex:honey:3:unknown")).unwrap_err().kind,
            ErrorKind::Unregistered
        );
        let active = s.peers(&refresh(caller)).unwrap();
        assert_eq!(active["peers"][0]["agent_id"], caller);
        assert_eq!(active["peers"][0]["state"], "live");
        assert_eq!(active["peers"][1]["agent_id"], other);
        assert_eq!(active["peers"][1]["state"], "gone");
    }

    #[test]
    fn metrics_excludes_expired_message_without_inbox_poll() {
        let s = Store::memory().unwrap();
        let sent = s.send(&json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"hello"})).unwrap();
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 1\n"));
        s.0.lock()
            .unwrap()
            .execute(
                "UPDATE messages SET expires_at=unixepoch()-1 WHERE msg_id=?1",
                [sent.envelope["msg_id"].as_str().unwrap()],
            )
            .unwrap();
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 0\n"));
    }

    #[test]
    fn rejects_unknown_and_ill_typed_envelope_fields() {
        let s = Store::memory().unwrap();
        let base =
            json!({"from":"claude:honey:1:a","to":"pi:sting:2:b","ticket":"none","body":"hello"});
        let mut state = base.clone();
        state["state"] = json!("operator");
        assert_eq!(s.send(&state).unwrap_err(), "unknown envelope field");
        let mut expires = base.clone();
        expires["reply_expires"] = json!({"unexpected":"object"});
        assert_eq!(s.send(&expires).unwrap_err(), "invalid reply_expires");
        expires["reply_expires"] = json!("now");
        assert_eq!(s.send(&expires).unwrap_err(), "invalid reply_expires");
        let mut ruling = base.clone();
        ruling["ruling"] = json!("");
        assert_eq!(s.send(&ruling).unwrap_err(), "invalid ruling");
        let mut extra = base;
        extra["authority"] = json!("operator");
        let sent = s.send(&extra).unwrap();
        assert_eq!(sent.envelope["authority"], "peer");
        assert!(sent.envelope.get("state").is_none());
    }
    #[test]
    fn round_trip_and_opaque_conflict() {
        let s = Store::memory().unwrap();
        let me = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        s.register(
            &json!({"harness":"claude","host":"honey","pid":1,"session_id":"a","proc_start":"1"}),
        )
        .unwrap();
        let input = json!({"from":me,"to":to,"ticket":"TIN-4655","body":"hello","msg_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"});
        let sent = s.send(&input).unwrap();
        assert_eq!(sent.envelope["authority"], "peer");
        assert!(sent.created);
        let retry = s.send(&input).unwrap();
        assert_eq!(retry.envelope["seq"], 1);
        assert!(!retry.created);
        let conflict=s.send(&json!({"from":me,"to":to,"ticket":"TIN-4655","body":"different","msg_id":"01ARZ3NDEKTSV4RRFFQ69G5FAV"})).unwrap_err();
        assert_eq!(conflict, "msg_id conflict");
        let got = s.inbox(to, 10).unwrap();
        assert_eq!(got["messages"][0]["delivery_count"], 1);
        assert_eq!(s.inbox(to, 10).unwrap()["messages"][0]["delivery_count"], 2);
        assert_eq!(
            s.ack(to, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()["state"],
            "acked"
        );
        assert!(
            s.inbox(to, 10).unwrap()["messages"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn wal_survives_reopen_and_sequences_thread() {
        let path = std::env::temp_dir().join(format!("swb-{}.sqlite3", Ulid::new()));
        let from = "claude:honey:1:a";
        let to = "pi:sting:2:b";
        let thread = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        {
            let s = Store::open(&path).unwrap();
            let mode: String =
                s.0.lock()
                    .unwrap()
                    .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                    .unwrap();
            assert_eq!(mode, "wal");
            let first = s
                .send(
                    &json!({"from":from,"to":to,"ticket":"none","body":"first","thread_id":thread}),
                )
                .unwrap();
            assert_eq!(first.envelope["seq"], 1);
        }
        {
            let s = Store::open(&path).unwrap();
            let second = s.send(&json!({"from":from,"to":to,"ticket":"none","body":"second","thread_id":thread})).unwrap();
            assert_eq!(second.envelope["seq"], 2);
            assert_eq!(
                s.inbox(to, 10).unwrap()["messages"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}-wal", path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    }
}
