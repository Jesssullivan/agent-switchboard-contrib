//! SQLite coordination state. Mutations and their receipts commit in one writer
//! transaction (SWB-R02, SWB-R09, SWB-R14, SWB-R46).
//!
//! Every time the store reads or writes comes from an injected [`Clock`]
//! (R-C262). Production uses [`SystemClock`], which reads the same Unix
//! seconds SQLite `unixepoch()` returned before the seam existed. Tests and
//! the spec live adapter use [`ManualClock`], which moves only when told to.

use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::Sha256;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ulid::Ulid;

pub const RETENTION_ACKED_DAYS: u32 = 7;
pub const RETENTION_UNACKED_DAYS: u32 = 30;
pub const DEFAULT_TTL_HOURS: u32 = 72;
pub const MAX_TTL_DAYS: u32 = 14;

pub fn effective_ttl_hours(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_TTL_HOURS)
        .clamp(1, MAX_TTL_DAYS * 24)
}

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

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing {key}"))
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
        && !parts[1].is_empty()
        && parts[2].parse::<u32>().is_ok()
        && !parts[3].is_empty()
        && parts[1..].iter().all(|p| {
            p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
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
fn sql_err(e: rusqlite::Error) -> String {
    e.to_string()
}

impl Store {
    /// The store's current time, in Unix seconds.
    pub fn now(&self) -> i64 {
        self.1.now()
    }
    pub fn prune(&self) -> Result<(), String> {
        let now = self.now();
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
        db.execute("DELETE FROM messages WHERE (state='acked' AND acked_at<=?1-604800) OR (state!='acked' AND created_at<=?1-2592000)", [now]).map_err(sql_err)?;
        Ok(())
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Self::open_with_clock(path, Arc::new(SystemClock))
    }
    pub fn open_with_clock(path: impl AsRef<Path>, clock: Arc<dyn Clock>) -> Result<Self, String> {
        let db = Connection::open(path).map_err(sql_err)?;
        let store = Self::init(db, clock)?;
        store.prune()?;
        Ok(store)
    }
    pub fn memory() -> Result<Self, String> {
        Self::memory_with_clock(Arc::new(SystemClock))
    }
    pub fn memory_with_clock(clock: Arc<dyn Clock>) -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(sql_err)?, clock)
    }
    fn init(db: Connection, clock: Arc<dyn Clock>) -> Result<Self, String> {
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
        let mut key = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut key))
            .map_err(|e| format!("audit key generation: {e}"))?;
        db.execute(
            "INSERT OR IGNORE INTO settings(name,value) VALUES('audit_hmac_key',?1)",
            [&key[..]],
        )
        .map_err(sql_err)?;
        Ok(Self(Mutex::new(db), clock))
    }
    pub fn body_hmac(&self, body: &str) -> Result<String, String> {
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
        let key: Vec<u8> = db
            .query_row(
                "SELECT value FROM settings WHERE name='audit_hmac_key'",
                [],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|e| e.to_string())?;
        mac.update(body.as_bytes());
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
    pub fn register(&self, input: &Value) -> Result<Value, String> {
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
        let id = format!("{harness}:{host}:{pid}:{session_id}");
        if !valid_agent_id(&id) {
            return Err("invalid agent id".into());
        }
        let now = self.now();
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
        db.execute("INSERT INTO sessions(agent_id,harness,host,proc_start,last_seen) VALUES(?1,?2,?3,?4,?5)
                    ON CONFLICT(agent_id) DO UPDATE SET proc_start=excluded.proc_start,last_seen=excluded.last_seen,ended=0",
            params![id,harness,host,proc_start,now]).map_err(sql_err)?;
        Ok(json!({"agent_id":id,"lease_seconds":900}))
    }
    pub fn end(&self, me: &str, proc_start: &str) -> Result<Value, String> {
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
        let changed = db
            .execute(
                "UPDATE sessions SET ended=1 WHERE agent_id=?1 AND proc_start=?2",
                params![me, proc_start],
            )
            .map_err(sql_err)?;
        Ok(json!({"agent_id":me,"state":if changed == 1 {"ended"} else {"unknown"}}))
    }
    pub fn peers(&self, me: Option<&str>) -> Result<Value, String> {
        let now = self.now();
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
        if let Some(me) = me {
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
                return Err("unregistered me".into());
            }
        }
        let mut stmt = db.prepare("SELECT agent_id,harness,host,proc_start,last_seen,ended FROM sessions ORDER BY agent_id").map_err(sql_err)?;
        let rows = stmt.query_map([], |r| {
            let seen: i64 = r.get(4)?; let ended: i64 = r.get(5)?;
            Ok(json!({"agent_id":r.get::<_,String>(0)?,"harness":r.get::<_,String>(1)?,"host":r.get::<_,String>(2)?,
                "proc_start":r.get::<_,String>(3)?,"last_seen":seen,
                "state":if ended != 0 {"ended"} else if seen >= now-900 {"live"} else if seen >= now-21600 {"idle"} else {"gone"}}))
        }).map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map(|v| json!({"peers":v}))
            .map_err(sql_err)
    }
    pub fn send(&self, input: &Value) -> Result<SendOutcome, String> {
        self.prune()?;
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
        let now = self.now();
        let mut db = self.0.lock().map_err(|_| "store lock poisoned")?;
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
                return Err("msg_id conflict".into());
            }
            let envelope = serde_json::from_str(&envelope).map_err(|e| e.to_string())?;
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
    pub fn inbox(&self, me: &str, limit: u32) -> Result<Value, String> {
        self.prune()?;
        if !valid_agent_id(me) {
            return Err("invalid agent id".into());
        }
        let now = self.now();
        let mut db = self.0.lock().map_err(|_| "store lock poisoned")?;
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
                let mut v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
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
            .collect::<Result<Vec<_>, String>>()?;
        tx.commit().map_err(sql_err)?;
        Ok(json!({"messages":values}))
    }
    pub fn ack(&self, me: &str, msg_id: &str) -> Result<Value, String> {
        if !valid_agent_id(me) || !valid_ulid(msg_id) {
            return Err("invalid identity or msg_id".into());
        }
        let now = self.now();
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
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
                return Err("message unavailable".into());
            }
        }
        db.execute(
            "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
            params![me, now],
        )
        .map_err(sql_err)?;
        Ok(json!({"msg_id":msg_id,"state":"acked"}))
    }
    pub fn metrics(&self) -> Result<String, String> {
        let now = self.now();
        let db = self.0.lock().map_err(|_| "store lock poisoned")?;
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
        let state = |s: &Store| s.peers(None).unwrap()["peers"][0]["state"].clone();
        assert_eq!(s.peers(None).unwrap()["peers"][0]["last_seen"], T0);
        clock.advance(900);
        assert_eq!(state(&s), "live");
        clock.advance(1);
        assert_eq!(state(&s), "idle");
        clock.advance(21600 - 901);
        assert_eq!(state(&s), "idle");
        assert!(s.metrics().unwrap().contains("swb_sessions{state=\"live\"} 0\n"));
        clock.advance(1);
        assert_eq!(state(&s), "gone");
        let renewed = s.peers(Some(&me)).unwrap();
        assert_eq!(renewed["peers"][0]["state"], "live");
        assert_eq!(renewed["peers"][0]["last_seen"], T0 + 21601);
        assert!(s.metrics().unwrap().contains("swb_sessions{state=\"live\"} 1\n"));
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
        assert_eq!(s.inbox(to, 10).unwrap()["messages"].as_array().unwrap().len(), 1);
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 1\n"));
        clock.advance(1);
        assert!(s.metrics().unwrap().contains("swb_mailbox_unacked 0\n"));
        assert!(s.inbox(to, 10).unwrap()["messages"].as_array().unwrap().is_empty());
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
    fn end_requires_matching_process_start() {
        let s = Store::memory().unwrap();
        let registration = s
            .register(&json!({
                "harness":"claude","host":"honey","pid":10,
                "session_id":"same","proc_start":"new"
            }))
            .unwrap();
        let me = registration["agent_id"].as_str().unwrap();
        assert_eq!(s.end(me, "old").unwrap()["state"], "unknown");
        assert_eq!(s.peers(None).unwrap()["peers"][0]["state"], "live");
        assert_eq!(s.end(me, "new").unwrap()["state"], "ended");
        assert_eq!(s.peers(None).unwrap()["peers"][0]["state"], "ended");
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
        let peers = s.peers(None).unwrap();
        assert_eq!(peers["peers"][0]["state"], "live");
        assert_eq!(peers["peers"][1]["state"], "gone");
        s.inbox(to, 1).unwrap();
        let peers = s.peers(None).unwrap();
        assert_eq!(peers["peers"][1]["state"], "live");
        s.0.lock()
            .unwrap()
            .execute("UPDATE sessions SET last_seen=unixepoch()-30000", [])
            .unwrap();
        s.ack(to, sent.envelope["msg_id"].as_str().unwrap())
            .unwrap();
        let peers = s.peers(None).unwrap();
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
        let passive = s.peers(None).unwrap();
        assert_eq!(passive["peers"][0]["state"], "gone");
        assert_eq!(passive["peers"][1]["state"], "gone");
        assert_eq!(
            s.peers(Some("codex:honey:3:unknown")).unwrap_err(),
            "unregistered me"
        );
        assert_eq!(s.peers(Some("invalid")).unwrap_err(), "invalid me");
        let active = s.peers(Some(caller)).unwrap();
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
