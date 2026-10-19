//! Advisory claims, release, handoff, heartbeat and thread (SWB-R16,
//! R-C950, P2).
//!
//! Claims are advisory. The one refusal is an `exclusive` request against
//! another session's live claim on the subject (SWB-R16, R-C950):
//!
//! 1. An exclusive request while another session holds any active claim on
//!    the subject (exclusive or not) is refused with `held_by` listing the
//!    holders, and nothing is recorded.
//! 2. A non-exclusive request is always recorded. Every other session's
//!    active claim on the subject, the exclusive holder included, is listed
//!    in `overlaps[]`.
//! 3. An exclusive request from the session that already holds an active
//!    exclusive claim on the subject is an idempotent renewal: the same
//!    `claim_id`, its expiry extended.
//! 4. Ending a holder session (`end` with its token) releases that session's
//!    claims at once; the broker logs each release.
//!
//! A claim whose holder is `gone` is shown `orphaned` and is never revoked.
//! A claim lease defaults to 4 h and is clamped to 1..24 h. Nothing here
//! acts on a process, a pane or another session's claim (R-N11): `release`
//! and `handoff` act only on the caller's own claims, and a handoff moves a
//! claim only when its receiver accepts it.

use super::{
    ErrorKind, LEASE_SECONDS, Store, StoreError, StoreResult, lease_state, sql_err, valid_agent_id,
    valid_ulid,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use ulid::Ulid;

/// SWB-R16: a claim lease defaults to 4 h and is at most 24 h.
pub const CLAIM_LEASE_DEFAULT_HOURS: u32 = 4;
pub const CLAIM_LEASE_MAX_HOURS: u32 = 24;
/// Longest subject, in bytes.
pub const SUBJECT_MAX_BYTES: usize = 256;
/// Longest claim or handoff note, in bytes.
pub const NOTE_MAX_BYTES: usize = 512;
/// Claims and handoffs are kept 7 days after they stop being active.
pub const CLAIM_RETENTION_SECONDS: i64 = 604_800;
/// `claims` page size when the caller names none, and the largest it accepts.
pub const CLAIMS_DEFAULT_LIMIT: u32 = 100;
pub const CLAIMS_MAX_LIMIT: u32 = 500;
/// The most open handoffs `claims {me}` lists; `handoffs_truncated` says
/// when there were more.
pub const HANDOFFS_LISTED: u32 = 100;
/// `thread` page size when the caller names none, and the largest it accepts.
pub const THREAD_DEFAULT_LIMIT: u32 = 20;
pub const THREAD_MAX_LIMIT: u32 = 100;

/// A claim lease in seconds: default 4 h, clamped to 1..24 h.
pub fn effective_claim_lease_seconds(requested: Option<u32>) -> i64 {
    i64::from(
        requested
            .unwrap_or(CLAIM_LEASE_DEFAULT_HOURS)
            .clamp(1, CLAIM_LEASE_MAX_HOURS),
    ) * 3600
}

/// A subject names what is claimed: a ticket (`TIN-4655`), a path
/// (`lab:nix/hosts/sting.nix`) or any other short token. It uses
/// `[A-Za-z0-9._:/#@+-]`, at most 256 bytes, so it is safe in a URL, a shell
/// word and a log label.
pub fn valid_subject(subject: &str) -> bool {
    !subject.is_empty()
        && subject.len() <= SUBJECT_MAX_BYTES
        && subject
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/#@+-".contains(c))
}

/// A claim or handoff note: at most `NOTE_MAX_BYTES`, no control characters.
pub fn valid_note(note: &str) -> bool {
    note.len() <= NOTE_MAX_BYTES && !note.chars().any(char::is_control)
}

fn unavailable(what: &str) -> StoreError {
    StoreError::new(ErrorKind::ClaimUnavailable, format!("{what} unavailable"))
}

/// Reject any field outside `allowed`, so a misspelt option is an error, not
/// silently ignored.
fn only_fields(input: &Value, allowed: &[&str]) -> StoreResult<()> {
    let map = input.as_object().ok_or("expected object")?;
    if map.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("unknown field".into());
    }
    Ok(())
}

fn opt_str<'a>(input: &'a Value, key: &str) -> StoreResult<Option<&'a str>> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("invalid {key}").into()),
    }
}

fn me_of(input: &Value) -> StoreResult<&str> {
    let me = input
        .get("me")
        .and_then(Value::as_str)
        .ok_or("missing me")?;
    if !valid_agent_id(me) {
        return Err("invalid me".into());
    }
    Ok(me)
}

fn stamp(db: &Connection, t: i64) -> StoreResult<String> {
    db.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%SZ',?1,'unixepoch')",
        [t],
        |r| r.get(0),
    )
    .map_err(sql_err)
}

/// Refresh a registered caller's lease (and clear ended), like every other
/// broker operation. An unregistered caller is an error.
fn refresh(db: &Connection, me: &str, now: i64) -> StoreResult<()> {
    let n = db
        .execute(
            "UPDATE sessions SET last_seen=?2, ended=0 WHERE agent_id=?1",
            params![me, now],
        )
        .map_err(sql_err)?;
    if n == 0 {
        return Err(StoreError::new(ErrorKind::Unregistered, "unregistered me"));
    }
    Ok(())
}

/// Whether a holder's session is `gone` (no refresh for 6 hours). A holder
/// with no session row is not orphaned.
fn holder_gone(db: &Connection, holder: &str, now: i64) -> StoreResult<bool> {
    let row: Option<(i64, i64)> = db
        .query_row(
            "SELECT last_seen, ended FROM sessions WHERE agent_id=?1",
            [holder],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql_err)?;
    Ok(row.is_some_and(|(seen, ended)| lease_state(seen, ended != 0, now) == "gone"))
}

/// An active, unexpired claim on a subject.
struct Active {
    claim_id: String,
    holder: String,
    exclusive: bool,
    expires_at: i64,
}

fn active_on(db: &Connection, subject: &str, now: i64) -> StoreResult<Vec<Active>> {
    let mut stmt = db
        .prepare(
            "SELECT claim_id, holder, exclusive, expires_at FROM claims
             WHERE subject=?1 AND state='active' AND expires_at>?2
             ORDER BY created_at, claim_id",
        )
        .map_err(sql_err)?;
    let rows = stmt
        .query_map(params![subject, now], |r| {
            Ok(Active {
                claim_id: r.get(0)?,
                holder: r.get(1)?,
                exclusive: r.get::<_, i64>(2)? != 0,
                expires_at: r.get(3)?,
            })
        })
        .map_err(sql_err)?;
    rows.collect::<Result<_, _>>().map_err(sql_err)
}

/// One row per holder, in first-claim order: a holder is listed once even
/// if a store from before claims were per-holder kept two rows, and its
/// exclusive claim, when it has one, is the one shown.
fn holder_rows(db: &Connection, claims: &[&Active], now: i64) -> StoreResult<Vec<Value>> {
    let mut picked: Vec<&Active> = Vec::with_capacity(claims.len());
    for c in claims {
        match picked.iter_mut().find(|p| p.holder == c.holder) {
            Some(p) if c.exclusive && !p.exclusive => *p = c,
            Some(_) => {}
            None => picked.push(c),
        }
    }
    picked
        .iter()
        .map(|c| {
            Ok(json!({
                "claim_id": c.claim_id,
                "holder": c.holder,
                "exclusive": c.exclusive,
                "orphaned": holder_gone(db, &c.holder, now)?,
            }))
        })
        .collect()
}

/// Create the claim and handoff tables in place (P2). A v0.3.0 store gains
/// them on open; nothing existing changes.
pub(crate) fn migrate(db: &Connection) -> StoreResult<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS claims (
           claim_id TEXT PRIMARY KEY, subject TEXT NOT NULL, holder TEXT NOT NULL,
           exclusive INTEGER NOT NULL, note TEXT, state TEXT NOT NULL DEFAULT 'active',
           created_at INTEGER NOT NULL, renewed_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
           ended_at INTEGER, end_reason TEXT, handoff_from TEXT);
         CREATE INDEX IF NOT EXISTS claims_by_subject ON claims(subject, state, expires_at);
         CREATE INDEX IF NOT EXISTS claims_by_holder ON claims(holder, state);
         CREATE TABLE IF NOT EXISTS handoffs (
           handoff_id TEXT PRIMARY KEY, claim_id TEXT NOT NULL, subject TEXT NOT NULL,
           sender TEXT NOT NULL, recipient TEXT NOT NULL, note TEXT,
           state TEXT NOT NULL DEFAULT 'offered', created_at INTEGER NOT NULL,
           decided_at INTEGER, accepted_claim_id TEXT);
         CREATE INDEX IF NOT EXISTS handoffs_by_recipient ON handoffs(recipient, state);
         CREATE INDEX IF NOT EXISTS handoffs_by_claim ON handoffs(claim_id, state);",
    )
    .map_err(sql_err)
}

/// Retention for claims and handoffs: 7 days after a claim stops being
/// active (released, handed off or expired), and 7 days after a handoff was
/// offered (a handoff can only be accepted while its claim is active, and a
/// claim lease is at most 24 h).
pub(crate) fn prune(db: &Connection, now: i64) -> StoreResult<()> {
    db.execute(
        "DELETE FROM claims WHERE (state!='active' AND ended_at<=?1-?2)
                               OR (state='active' AND expires_at<=?1-?2)",
        params![now, CLAIM_RETENTION_SECONDS],
    )
    .map_err(sql_err)?;
    db.execute(
        "DELETE FROM handoffs WHERE created_at<=?1-?2",
        params![now, CLAIM_RETENTION_SECONDS],
    )
    .map_err(sql_err)?;
    Ok(())
}

/// R-C950 (4): release every active claim `me` holds, inside the caller's
/// transaction, and cancel the handoffs offered on them. Returns the
/// released claims as `{claim_id, subject}` for the broker's audit log.
pub(crate) fn release_all_held(db: &Connection, me: &str, now: i64) -> StoreResult<Vec<Value>> {
    let released: Vec<(String, String)> = {
        let mut stmt = db
            .prepare(
                "SELECT claim_id, subject FROM claims
                 WHERE holder=?1 AND state='active' AND expires_at>?2
                 ORDER BY created_at, claim_id",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![me, now], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)?
    };
    db.execute(
        "UPDATE handoffs SET state='cancelled', decided_at=?2
         WHERE state='offered' AND claim_id IN
           (SELECT claim_id FROM claims
            WHERE holder=?1 AND state='active' AND expires_at>?2)",
        params![me, now],
    )
    .map_err(sql_err)?;
    db.execute(
        "UPDATE claims SET state='released', ended_at=?2, end_reason='session_ended'
         WHERE holder=?1 AND state='active' AND expires_at>?2",
        params![me, now],
    )
    .map_err(sql_err)?;
    Ok(released
        .into_iter()
        .map(|(claim_id, subject)| json!({"claim_id": claim_id, "subject": subject}))
        .collect())
}

/// One stored claim row, as `claims` reads it.
struct ClaimRow {
    claim_id: String,
    subject: String,
    holder: String,
    exclusive: bool,
    note: Option<String>,
    state: String,
    created_at: i64,
    expires_at: i64,
    ended_at: Option<i64>,
    end_reason: Option<String>,
    handoff_from: Option<String>,
}

/// A `claims` listing.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClaimsQuery<'a> {
    /// A registered caller whose own lease this call refreshes first; its
    /// open handoffs (offered to it or by it) are listed too.
    pub me: Option<&'a str>,
    /// Only claims on this subject.
    pub subject: Option<&'a str>,
    /// Only claims this session holds.
    pub holder: Option<&'a str>,
    /// Include released, handed-off and expired claims still retained.
    pub all: bool,
    /// Page size; clamped to 1..=[`CLAIMS_MAX_LIMIT`].
    pub limit: Option<u32>,
}

impl Store {
    /// Renew a registered session's lease (ADR-0001 `heartbeat`).
    pub fn heartbeat(&self, me: &str) -> StoreResult<Value> {
        if !valid_agent_id(me) {
            return Err("invalid me".into());
        }
        let db = self.db()?;
        refresh(&db, me, self.now())?;
        Ok(json!({"agent_id": me, "state": "live", "lease_seconds": LEASE_SECONDS}))
    }

    /// The messages of one thread that `me` sent or received, in seq order
    /// after `after_seq`. Reading a thread changes no delivery state; it
    /// renews the caller's lease. Pruned messages are absent (SWB-R25).
    pub fn thread(
        &self,
        me: &str,
        thread_id: &str,
        after_seq: Option<u64>,
        limit: Option<u32>,
    ) -> StoreResult<Value> {
        if !valid_agent_id(me) {
            return Err("invalid me".into());
        }
        if !valid_ulid(thread_id) {
            return Err("invalid thread_id".into());
        }
        let limit = limit
            .unwrap_or(THREAD_DEFAULT_LIMIT)
            .clamp(1, THREAD_MAX_LIMIT);
        let after = after_seq.unwrap_or(0).min(i64::MAX as u64) as i64;
        let db = self.db()?;
        let now = self.now();
        refresh(&db, me, now)?;
        let mut stmt = db
            .prepare(
                "SELECT envelope, state, delivery_count, seq FROM messages
                 WHERE thread_id=?1 AND (sender=?2 OR recipient=?2) AND seq>?3
                 ORDER BY seq LIMIT ?4",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map(params![thread_id, me, after, i64::from(limit) + 1], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(sql_err)?;
        let mut rows = rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next = if more {
            rows.last().map(|row| row.3)
        } else {
            None
        };
        let messages = rows
            .into_iter()
            .map(|(envelope, state, count, _)| {
                let mut v: Value = serde_json::from_str(&envelope)
                    .map_err(|e| StoreError::new(ErrorKind::Internal, e.to_string()))?;
                v["state"] = json!(state);
                v["delivery_count"] = json!(count);
                Ok(v)
            })
            .collect::<StoreResult<Vec<_>>>()?;
        Ok(json!({"thread_id": thread_id, "messages": messages, "next_after_seq": next}))
    }

    /// Claim a subject (SWB-R16, R-C950). See the module docs for the four
    /// ruled outcomes. With `handoff_id`, the caller accepts a handoff
    /// offered to it: the source claim becomes `handed_off` and the
    /// caller's claim is recorded in the same transaction, or neither
    /// happens.
    pub fn claim(&self, input: &Value) -> StoreResult<Value> {
        only_fields(
            input,
            &[
                "me",
                "subject",
                "exclusive",
                "lease_hours",
                "note",
                "handoff_id",
            ],
        )?;
        let me = me_of(input)?;
        let requested_exclusive = match input.get("exclusive") {
            None | Some(Value::Null) => None,
            Some(Value::Bool(b)) => Some(*b),
            Some(_) => return Err("invalid exclusive".into()),
        };
        let lease_hours = match input.get("lease_hours") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|n| *n >= 1)
                    .ok_or("invalid lease_hours")?
                    .min(u64::from(u32::MAX)) as u32,
            ),
        };
        let note = opt_str(input, "note")?;
        if note.is_some_and(|n| !valid_note(n)) {
            return Err("invalid note".into());
        }
        let handoff_id = opt_str(input, "handoff_id")?;
        if handoff_id.is_some_and(|h| !valid_ulid(h)) {
            return Err("invalid handoff_id".into());
        }
        let subject_arg = opt_str(input, "subject")?;
        if subject_arg.is_some_and(|s| !valid_subject(s)) {
            return Err("invalid subject".into());
        }
        if subject_arg.is_none() && handoff_id.is_none() {
            return Err("missing subject".into());
        }
        let lease = effective_claim_lease_seconds(lease_hours);

        let mut db = self.db()?;
        let now = self.now();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        refresh(&tx, me, now)?;

        // Accepting a handoff: the offer must be open, addressed to me, and
        // its source claim still active in its offerer's hands.
        let mut handoff_from = None;
        let mut source_exclusive = false;
        let subject = if let Some(hid) = handoff_id {
            let offer: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT claim_id, subject, sender, recipient FROM handoffs
                     WHERE handoff_id=?1 AND state='offered'",
                    [hid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(sql_err)?;
            let Some((source, subject, sender, recipient)) = offer else {
                return Err(unavailable("handoff"));
            };
            if recipient != me || subject_arg.is_some_and(|s| s != subject) {
                return Err(unavailable("handoff"));
            }
            let source_row: Option<i64> = tx
                .query_row(
                    "SELECT exclusive FROM claims
                     WHERE claim_id=?1 AND holder=?2 AND state='active' AND expires_at>?3",
                    params![source, sender, now],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_err)?;
            let Some(source_ex) = source_row else {
                return Err(unavailable("handoff"));
            };
            source_exclusive = source_ex != 0;
            tx.execute(
                "UPDATE claims SET state='handed_off', ended_at=?2, end_reason='handoff'
                 WHERE claim_id=?1",
                params![source, now],
            )
            .map_err(sql_err)?;
            handoff_from = Some(source);
            subject
        } else {
            subject_arg.unwrap_or_default().to_owned()
        };
        // An accepted handoff keeps the source's exclusivity unless the
        // receiver asks otherwise; a plain claim is non-exclusive by default.
        // An accepted exclusive handoff is an exclusive request like any
        // other: R-C950 (1) makes no exception for it, so it is refused while
        // another session holds an active claim on the subject, and the offer
        // stays open (the receiver may accept it with `exclusive: false`).
        let exclusive = requested_exclusive.unwrap_or(source_exclusive);

        // A session holds at most one active claim per subject: a repeat
        // request updates that claim in place.
        let active = active_on(&tx, &subject, now)?;
        let own = active
            .iter()
            .find(|c| c.holder == me && c.exclusive)
            .or_else(|| active.iter().find(|c| c.holder == me));
        let others: Vec<&Active> = active.iter().filter(|c| c.holder != me).collect();
        let overlaps = holder_rows(&tx, &others, now)?;
        let own_exclusive = own.is_some_and(|c| c.exclusive);

        if exclusive && !own_exclusive && !others.is_empty() {
            // R-C950 (1) and SWB-R16: refused, nothing recorded. The lease
            // refresh still counts, like every other call by a registered
            // session; a handoff acceptance is undone with the rest.
            tx.rollback().map_err(sql_err)?;
            refresh(&db, me, now)?;
            return Ok(json!({
                "outcome": "refused", "recorded": false, "subject": subject,
                "exclusive": true, "held_by": overlaps,
            }));
        }

        let (claim_id, outcome, expires_at, exclusive) = match own {
            // R-C950 (3): a repeat exclusive request by the exclusive holder
            // is an idempotent renewal. A repeat non-exclusive request, or an
            // exclusive one over the caller's own advisory claim, updates the
            // same claim too: it is still recorded (R-C950 (2)), but never a
            // second row, so a holder appears once in overlaps and held_by.
            // Exclusivity is only ever added here, never dropped.
            Some(held) => {
                let expires = held.expires_at.max(now + lease);
                let keeps = held.exclusive || exclusive;
                tx.execute(
                    "UPDATE claims SET renewed_at=?2, expires_at=?3, note=COALESCE(?4, note),
                            exclusive=?5, handoff_from=COALESCE(?6, handoff_from)
                     WHERE claim_id=?1",
                    params![held.claim_id, now, expires, note, keeps, handoff_from],
                )
                .map_err(sql_err)?;
                let outcome = if held.exclusive && exclusive {
                    "renewed"
                } else {
                    "recorded"
                };
                (held.claim_id.clone(), outcome, expires, keeps)
            }
            // R-C950 (2) and an unopposed exclusive request: recorded.
            None => {
                let id = Ulid::new().to_string();
                tx.execute(
                    "INSERT INTO claims(claim_id,subject,holder,exclusive,note,created_at,renewed_at,expires_at,handoff_from)
                     VALUES(?1,?2,?3,?4,?5,?6,?6,?7,?8)",
                    params![id, subject, me, exclusive, note, now, now + lease, handoff_from],
                )
                .map_err(sql_err)?;
                tx.execute(
                    "INSERT INTO counters(name,value) VALUES('claims_total',1)
                     ON CONFLICT(name) DO UPDATE SET value=value+1",
                    [],
                )
                .map_err(sql_err)?;
                (id, "recorded", now + lease, exclusive)
            }
        };
        if let Some(hid) = handoff_id {
            tx.execute(
                "UPDATE handoffs SET state='accepted', decided_at=?2, accepted_claim_id=?3
                 WHERE handoff_id=?1",
                params![hid, now, claim_id],
            )
            .map_err(sql_err)?;
            // Other open offers on the same source can no longer be accepted.
            tx.execute(
                "UPDATE handoffs SET state='cancelled', decided_at=?2
                 WHERE claim_id=?1 AND state='offered'",
                params![handoff_from, now],
            )
            .map_err(sql_err)?;
        }
        let expires_text = stamp(&tx, expires_at)?;
        tx.commit().map_err(sql_err)?;
        let mut result = json!({
            "outcome": outcome, "recorded": true, "claim_id": claim_id,
            "subject": subject, "holder": me, "exclusive": exclusive,
            "state": "active", "expires_at": expires_text,
            "lease_seconds": expires_at - now, "overlaps": overlaps,
        });
        if let Some(hid) = handoff_id {
            result["handoff_id"] = json!(hid);
            result["handoff_from"] = json!(handoff_from);
        }
        Ok(result)
    }

    /// Release one of the caller's own claims. Releasing it again, or one
    /// that already expired or was handed off, changes nothing and reports
    /// its final state. Another session's claim, or an unknown id, answers
    /// `claim_unavailable` without saying which (R-N11: never acts on
    /// another session's claim).
    pub fn release(&self, me: &str, claim_id: &str) -> StoreResult<Value> {
        if !valid_agent_id(me) {
            return Err("invalid me".into());
        }
        if !valid_ulid(claim_id) {
            return Err("invalid claim_id".into());
        }
        let mut db = self.db()?;
        let now = self.now();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        let row: Option<(String, String, String, i64)> = tx
            .query_row(
                "SELECT holder, subject, state, expires_at FROM claims WHERE claim_id=?1",
                [claim_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(sql_err)?;
        let Some((_, subject, state, expires_at)) = row.filter(|r| r.0 == me) else {
            return Err(unavailable("claim"));
        };
        let released = state == "active" && expires_at > now;
        if released {
            tx.execute(
                "UPDATE claims SET state='released', ended_at=?2, end_reason='released'
                 WHERE claim_id=?1",
                params![claim_id, now],
            )
            .map_err(sql_err)?;
            tx.execute(
                "UPDATE handoffs SET state='cancelled', decided_at=?2
                 WHERE claim_id=?1 AND state='offered'",
                params![claim_id, now],
            )
            .map_err(sql_err)?;
        }
        refresh(&tx, me, now)?;
        tx.commit().map_err(sql_err)?;
        let shown = if released {
            "released"
        } else if state == "active" {
            "expired"
        } else {
            state.as_str()
        };
        Ok(json!({
            "claim_id": claim_id, "subject": subject, "holder": me,
            "state": shown, "released": released,
        }))
    }

    /// Offer one of the caller's active claims to another session. The offer
    /// is a receipt in `offered`; the receiver accepts it with
    /// `claim {handoff_id}`, which marks the source `handed_off`. Offering
    /// the same claim to the same receiver again returns the open offer.
    /// The Linear receipt comment (SWB-R15) is not posted by this broker
    /// yet.
    pub fn handoff(&self, input: &Value) -> StoreResult<Value> {
        only_fields(input, &["me", "claim_id", "to", "note"])?;
        let me = me_of(input)?;
        let claim_id = input
            .get("claim_id")
            .and_then(Value::as_str)
            .ok_or("missing claim_id")?;
        if !valid_ulid(claim_id) {
            return Err("invalid claim_id".into());
        }
        let to = input
            .get("to")
            .and_then(Value::as_str)
            .ok_or("missing to")?;
        if !valid_agent_id(to) {
            return Err("invalid to".into());
        }
        if to == me {
            return Err("cannot hand off to self".into());
        }
        let note = opt_str(input, "note")?;
        if note.is_some_and(|n| !valid_note(n)) {
            return Err("invalid note".into());
        }
        let mut db = self.db()?;
        let now = self.now();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(sql_err)?;
        refresh(&tx, me, now)?;
        let source: Option<(String, bool)> = tx
            .query_row(
                "SELECT subject, exclusive FROM claims
                 WHERE claim_id=?1 AND holder=?2 AND state='active' AND expires_at>?3",
                params![claim_id, me, now],
                |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)),
            )
            .optional()
            .map_err(sql_err)?;
        let Some((subject, exclusive)) = source else {
            return Err(unavailable("claim"));
        };
        let open: Option<String> = tx
            .query_row(
                "SELECT handoff_id FROM handoffs
                 WHERE claim_id=?1 AND recipient=?2 AND state='offered'",
                params![claim_id, to],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let (handoff_id, created) = match open {
            Some(id) => (id, false),
            None => {
                let id = Ulid::new().to_string();
                tx.execute(
                    "INSERT INTO handoffs(handoff_id,claim_id,subject,sender,recipient,note,created_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![id, claim_id, subject, me, to, note, now],
                )
                .map_err(sql_err)?;
                tx.execute(
                    "INSERT INTO counters(name,value) VALUES('handoffs_total',1)
                     ON CONFLICT(name) DO UPDATE SET value=value+1",
                    [],
                )
                .map_err(sql_err)?;
                (id, true)
            }
        };
        tx.commit().map_err(sql_err)?;
        Ok(json!({
            "handoff_id": handoff_id, "claim_id": claim_id, "subject": subject,
            "exclusive": exclusive, "from": me, "to": to, "state": "offered",
            "created": created,
        }))
    }

    /// List claims (ADR-0001 `claims`): active, unexpired claims by default,
    /// each marked `orphaned` while its holder is gone; `all` adds released,
    /// handed-off and expired claims still retained. With `me`, the caller's
    /// lease is renewed and its open handoffs are listed too.
    pub fn claims(&self, query: &ClaimsQuery) -> StoreResult<Value> {
        if let Some(subject) = query.subject
            && !valid_subject(subject)
        {
            return Err("invalid subject".into());
        }
        if let Some(holder) = query.holder
            && !valid_agent_id(holder)
        {
            return Err("invalid holder".into());
        }
        if let Some(me) = query.me
            && !valid_agent_id(me)
        {
            return Err("invalid me".into());
        }
        let limit = query
            .limit
            .unwrap_or(CLAIMS_DEFAULT_LIMIT)
            .clamp(1, CLAIMS_MAX_LIMIT);
        let db = self.db()?;
        let now = self.now();
        if let Some(me) = query.me {
            refresh(&db, me, now)?;
        }
        let rows: Vec<ClaimRow> = {
            let mut stmt = db
                .prepare(
                    "SELECT claim_id, subject, holder, exclusive, note, state, created_at,
                            expires_at, ended_at, end_reason, handoff_from
                     FROM claims
                     WHERE (?1 IS NULL OR subject=?1) AND (?2 IS NULL OR holder=?2)
                       AND (?3 OR (state='active' AND expires_at>?4))
                     ORDER BY created_at, claim_id LIMIT ?5",
                )
                .map_err(sql_err)?;
            let mapped = stmt
                .query_map(
                    params![
                        query.subject,
                        query.holder,
                        query.all,
                        now,
                        i64::from(limit) + 1
                    ],
                    |r| {
                        Ok(ClaimRow {
                            claim_id: r.get(0)?,
                            subject: r.get(1)?,
                            holder: r.get(2)?,
                            exclusive: r.get::<_, i64>(3)? != 0,
                            note: r.get(4)?,
                            state: r.get(5)?,
                            created_at: r.get(6)?,
                            expires_at: r.get(7)?,
                            ended_at: r.get(8)?,
                            end_reason: r.get(9)?,
                            handoff_from: r.get(10)?,
                        })
                    },
                )
                .map_err(sql_err)?;
            mapped.collect::<Result<_, _>>().map_err(sql_err)?
        };
        // One row past the page says whether the listing was cut short.
        let mut rows = rows;
        let truncated = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let mut claims = Vec::with_capacity(rows.len());
        for c in rows {
            let live = c.state == "active" && c.expires_at > now;
            let orphaned = live && holder_gone(&db, &c.holder, now)?;
            let shown = match (live, c.state.as_str()) {
                (true, _) if orphaned => "orphaned",
                (true, _) => "active",
                (false, "active") => "expired",
                (false, other) => other,
            };
            let mut row = json!({
                "claim_id": c.claim_id, "subject": c.subject, "holder": c.holder,
                "exclusive": c.exclusive, "state": shown, "orphaned": orphaned,
                "created_at": stamp(&db, c.created_at)?, "expires_at": stamp(&db, c.expires_at)?,
            });
            if let Some(note) = c.note {
                row["note"] = json!(note);
            }
            if let Some(ended) = c.ended_at {
                row["ended_at"] = json!(stamp(&db, ended)?);
            }
            if let Some(reason) = c.end_reason {
                row["end_reason"] = json!(reason);
            }
            if let Some(from) = c.handoff_from {
                row["handoff_from"] = json!(from);
            }
            claims.push(row);
        }
        let mut result = json!({"claims": claims, "truncated": truncated});
        if let Some(me) = query.me {
            let mut stmt = db
                .prepare(
                    "SELECT h.handoff_id, h.claim_id, h.subject, h.sender, h.recipient, h.note
                     FROM handoffs h JOIN claims c ON c.claim_id=h.claim_id
                     WHERE h.state='offered' AND (h.recipient=?1 OR h.sender=?1)
                       AND c.state='active' AND c.expires_at>?2
                     ORDER BY h.created_at, h.handoff_id LIMIT ?3",
                )
                .map_err(sql_err)?;
            let mut offers = stmt
                .query_map(params![me, now, i64::from(HANDOFFS_LISTED) + 1], |r| {
                    let note: Option<String> = r.get(5)?;
                    Ok(json!({
                        "handoff_id": r.get::<_, String>(0)?, "claim_id": r.get::<_, String>(1)?,
                        "subject": r.get::<_, String>(2)?, "from": r.get::<_, String>(3)?,
                        "to": r.get::<_, String>(4)?, "state": "offered", "note": note,
                    }))
                })
                .map_err(sql_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sql_err)?;
            result["handoffs_truncated"] = json!(offers.len() > HANDOFFS_LISTED as usize);
            offers.truncate(HANDOFFS_LISTED as usize);
            result["handoffs"] = json!(offers);
        }
        Ok(result)
    }

    /// Active unexpired claims that share their subject with another
    /// holder's active claim, for `/metrics` (`swb_claim_overlaps`).
    pub(crate) fn claim_overlaps(db: &Connection, now: i64) -> StoreResult<i64> {
        db.query_row(
            "SELECT COUNT(*) FROM claims c WHERE c.state='active' AND c.expires_at>?1
               AND EXISTS (SELECT 1 FROM claims o WHERE o.subject=c.subject
                 AND o.holder!=c.holder AND o.state='active' AND o.expires_at>?1)",
            [now],
            |r| r.get(0),
        )
        .map_err(sql_err)
    }

    /// Active unexpired claims, for `/metrics`.
    pub(crate) fn claims_active(db: &Connection, now: i64) -> StoreResult<i64> {
        db.query_row(
            "SELECT COUNT(*) FROM claims WHERE state='active' AND expires_at>?1",
            [now],
            |r| r.get(0),
        )
        .map_err(sql_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManualClock, PeersQuery};
    use std::sync::Arc;

    const T0: i64 = 1_700_000_000;
    const A: &str = "claude:honey:1:a";
    const B: &str = "pi:sting:2:b";
    const C: &str = "codex:neo:3:c";

    fn clocked() -> (Store, Arc<ManualClock>) {
        let clock = Arc::new(ManualClock::new(T0));
        let s = Store::memory_with_clock(clock.clone()).unwrap();
        for id in [A, B, C] {
            reg(&s, id);
        }
        (s, clock)
    }

    /// Register an agent id and return its end token.
    fn reg(s: &Store, id: &str) -> String {
        let p: Vec<&str> = id.split(':').collect();
        s.register(&json!({"harness":p[0],"host":p[1],"pid":p[2].parse::<u32>().unwrap(),"session_id":p[3],"proc_start":"p"}))
            .unwrap()["end_token"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn claim(s: &Store, me: &str, subject: &str, exclusive: bool) -> Value {
        s.claim(&json!({"me":me,"subject":subject,"exclusive":exclusive}))
            .unwrap()
    }

    fn holders(rows: &Value) -> Vec<String> {
        let mut v: Vec<String> = rows
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["holder"].as_str().unwrap().to_owned())
            .collect();
        v.sort();
        v
    }

    fn listed(s: &Store, subject: &str) -> Vec<Value> {
        s.claims(&ClaimsQuery {
            subject: Some(subject),
            ..ClaimsQuery::default()
        })
        .unwrap()["claims"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn two_advisory_claims_both_record_and_report_the_overlap() {
        let (s, _) = clocked();
        let first = claim(&s, A, "TIN-1", false);
        assert_eq!(first["outcome"], "recorded");
        assert_eq!(first["overlaps"], json!([]));
        let second = claim(&s, B, "TIN-1", false);
        assert_eq!(second["outcome"], "recorded");
        assert_eq!(holders(&second["overlaps"]), [A]);
        assert_eq!(listed(&s, "TIN-1").len(), 2);
    }

    #[test]
    fn exclusive_is_refused_while_any_other_claim_is_held() {
        let (s, _) = clocked();
        claim(&s, A, "TIN-1", false);
        claim(&s, B, "TIN-1", false);
        // R-C950 (1): refused, held_by lists every holder, nothing recorded.
        let refused = claim(&s, C, "TIN-1", true);
        assert_eq!(refused["outcome"], "refused");
        assert_eq!(refused["recorded"], false);
        assert!(refused.get("claim_id").is_none());
        assert_eq!(holders(&refused["held_by"]), [A, B]);
        assert_eq!(listed(&s, "TIN-1").len(), 2);
        // SWB-R16: a second exclusive claim is refused too.
        claim(&s, A, "TIN-2", true);
        let second = claim(&s, B, "TIN-2", true);
        assert_eq!(second["outcome"], "refused");
        assert_eq!(holders(&second["held_by"]), [A]);
        assert_eq!(second["held_by"][0]["exclusive"], true);
    }

    #[test]
    fn nonexclusive_beside_an_exclusive_holder_records_and_names_it() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", true);
        let advisory = claim(&s, B, "TIN-1", false);
        // R-C950 (2): recorded, exclusive holder in overlaps[].
        assert_eq!(advisory["outcome"], "recorded");
        assert_eq!(advisory["overlaps"][0]["holder"], A);
        assert_eq!(advisory["overlaps"][0]["exclusive"], true);
        assert_eq!(advisory["overlaps"][0]["claim_id"], held["claim_id"]);
    }

    #[test]
    fn a_repeat_exclusive_claim_renews_the_same_claim() {
        let (s, clock) = clocked();
        let first = claim(&s, A, "TIN-1", true);
        clock.advance(3600);
        let again = s
            .claim(&json!({"me":A,"subject":"TIN-1","exclusive":true,"lease_hours":8}))
            .unwrap();
        // R-C950 (3): same claim id, expiry extended.
        assert_eq!(again["outcome"], "renewed");
        assert_eq!(again["claim_id"], first["claim_id"]);
        assert_eq!(again["lease_seconds"], 8 * 3600);
        assert!(again["expires_at"].as_str() > first["expires_at"].as_str());
        assert_eq!(listed(&s, "TIN-1").len(), 1);
        // A shorter lease never shortens a renewal.
        let short = s
            .claim(&json!({"me":A,"subject":"TIN-1","exclusive":true,"lease_hours":1}))
            .unwrap();
        assert_eq!(short["expires_at"], again["expires_at"]);
    }

    #[test]
    fn own_advisory_claim_does_not_block_own_exclusive_request() {
        let (s, _) = clocked();
        let first = claim(&s, A, "TIN-1", false);
        let own = claim(&s, A, "TIN-1", true);
        assert_eq!(own["outcome"], "recorded");
        assert_eq!(own["overlaps"], json!([]));
        // The advisory claim becomes the exclusive one; no second row.
        assert_eq!(own["claim_id"], first["claim_id"]);
        assert_eq!(own["exclusive"], true);
        let rows = listed(&s, "TIN-1");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["exclusive"], true);
        assert_eq!(claim(&s, B, "TIN-1", true)["outcome"], "refused");
    }

    #[test]
    fn a_repeat_advisory_claim_updates_the_same_claim() {
        let (s, clock) = clocked();
        let first = claim(&s, A, "TIN-1", false);
        clock.advance(60);
        let again = claim(&s, A, "TIN-1", false);
        assert_eq!(again["outcome"], "recorded");
        assert_eq!(again["claim_id"], first["claim_id"]);
        assert_eq!(again["lease_seconds"], 4 * 3600);
        assert_eq!(listed(&s, "TIN-1").len(), 1);
        // Another holder sees A once, in overlaps and in held_by.
        let b = claim(&s, B, "TIN-1", false);
        assert_eq!(holders(&b["overlaps"]), [A]);
        let refused = claim(&s, C, "TIN-1", true);
        assert_eq!(holders(&refused["held_by"]), [A, B]);
        // An advisory request never drops an exclusive claim's guard.
        let (s, _) = clocked();
        let ex = claim(&s, A, "TIN-2", true);
        let weaker = claim(&s, A, "TIN-2", false);
        assert_eq!(weaker["claim_id"], ex["claim_id"]);
        assert_eq!(weaker["exclusive"], true);
        assert_eq!(claim(&s, B, "TIN-2", true)["outcome"], "refused");
        assert!(s.metrics().unwrap().contains("swb_claims_active 1"));
    }

    #[test]
    fn an_exclusive_handoff_is_refused_while_another_session_holds_a_claim() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", true);
        // An advisory claim beside the exclusive holder is recorded (R-C950 (2)).
        claim(&s, C, "TIN-1", false);
        let source = held["claim_id"].as_str().unwrap();
        let hid = s
            .handoff(&json!({"me":A,"claim_id":source,"to":B}))
            .unwrap()["handoff_id"]
            .clone();
        // Accepting keeps the source's exclusivity, so it is an exclusive
        // request, and R-C950 (1) refuses it while C holds a claim: nothing
        // moves and the offer stays open.
        let refused = s.claim(&json!({"me":B,"handoff_id":hid})).unwrap();
        assert_eq!(refused["outcome"], "refused");
        assert_eq!(holders(&refused["held_by"]), [C]);
        assert_eq!(holders(&json!(listed(&s, "TIN-1"))), [A, C]);
        // The receiver may accept it as advisory instead.
        let accepted = s
            .claim(&json!({"me":B,"handoff_id":hid,"exclusive":false}))
            .unwrap();
        assert_eq!(accepted["outcome"], "recorded");
        assert_eq!(accepted["exclusive"], false);
        assert_eq!(holders(&accepted["overlaps"]), [C]);
        assert_eq!(holders(&json!(listed(&s, "TIN-1"))), [B, C]);
    }

    #[test]
    fn an_unopposed_exclusive_handoff_keeps_the_guard() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", true);
        let hid = s
            .handoff(&json!({"me":A,"claim_id":held["claim_id"],"to":B}))
            .unwrap()["handoff_id"]
            .clone();
        let accepted = s.claim(&json!({"me":B,"handoff_id":hid})).unwrap();
        assert_eq!(accepted["outcome"], "recorded");
        assert_eq!(accepted["exclusive"], true);
        assert_eq!(claim(&s, A, "TIN-1", true)["outcome"], "refused");
    }

    #[test]
    fn end_does_not_release_claims_that_already_expired() {
        let (s, clock) = clocked();
        let token = reg(&s, A);
        let old = s
            .claim(&json!({"me":A,"subject":"TIN-1","lease_hours":1}))
            .unwrap();
        clock.advance(3601);
        let live = claim(&s, A, "TIN-2", false);
        let ended = s.end(A, &token).unwrap();
        let released: Vec<&str> = ended["released_claims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["claim_id"].as_str().unwrap())
            .collect();
        assert_eq!(released, [live["claim_id"].as_str().unwrap()]);
        // The expired claim keeps its own end: not released, no ended_at.
        let history = s
            .claims(&ClaimsQuery {
                holder: Some(A),
                all: true,
                ..ClaimsQuery::default()
            })
            .unwrap();
        let row = history["claims"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["claim_id"] == old["claim_id"])
            .unwrap()
            .clone();
        assert_eq!(row["state"], "expired");
        assert!(row.get("ended_at").is_none());
    }

    #[test]
    fn accepting_a_handoff_onto_an_own_claim_records_where_it_came_from() {
        let (s, _) = clocked();
        let a = claim(&s, A, "TIN-1", false);
        let mine = claim(&s, B, "TIN-1", false);
        let hid = s
            .handoff(&json!({"me":A,"claim_id":a["claim_id"],"to":B}))
            .unwrap()["handoff_id"]
            .clone();
        let accepted = s.claim(&json!({"me":B,"handoff_id":hid})).unwrap();
        assert_eq!(accepted["claim_id"], mine["claim_id"]);
        assert_eq!(accepted["handoff_from"], a["claim_id"]);
        let rows = listed(&s, "TIN-1");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["handoff_from"], a["claim_id"]);
    }

    #[test]
    fn a_short_page_says_it_was_cut() {
        let (s, _) = clocked();
        claim(&s, A, "TIN-1", false);
        claim(&s, B, "TIN-1", false);
        let page = |limit| {
            s.claims(&ClaimsQuery {
                limit: Some(limit),
                ..ClaimsQuery::default()
            })
            .unwrap()
        };
        assert_eq!(page(1)["claims"].as_array().unwrap().len(), 1);
        assert_eq!(page(1)["truncated"], true);
        assert_eq!(page(2)["truncated"], false);
    }

    #[test]
    fn a_lease_expires_on_its_own() {
        let (s, clock) = clocked();
        claim(&s, A, "TIN-1", true);
        clock.advance(4 * 3600 - 1);
        assert_eq!(listed(&s, "TIN-1").len(), 1);
        clock.advance(1);
        assert!(listed(&s, "TIN-1").is_empty());
        assert_eq!(claim(&s, B, "TIN-1", true)["outcome"], "recorded");
        let everything = s
            .claims(&ClaimsQuery {
                subject: Some("TIN-1"),
                all: true,
                ..ClaimsQuery::default()
            })
            .unwrap();
        let states: Vec<&str> = everything["claims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["state"].as_str().unwrap())
            .collect();
        assert_eq!(states, ["expired", "active"]);
    }

    #[test]
    fn lease_hours_default_and_clamp() {
        assert_eq!(effective_claim_lease_seconds(None), 4 * 3600);
        assert_eq!(effective_claim_lease_seconds(Some(25)), 24 * 3600);
        assert_eq!(effective_claim_lease_seconds(Some(1)), 3600);
        let (s, _) = clocked();
        assert_eq!(
            s.claim(&json!({"me":A,"subject":"x","lease_hours":0}))
                .unwrap_err(),
            "invalid lease_hours"
        );
    }

    #[test]
    fn ending_the_holder_releases_its_claims_at_once() {
        let (s, _) = clocked();
        let token = reg(&s, A);
        let one = claim(&s, A, "TIN-1", true);
        let two = claim(&s, A, "TIN-2", false);
        claim(&s, B, "TIN-2", false);
        let ended = s.end(A, &token).unwrap();
        assert_eq!(ended["state"], "ended");
        let released: Vec<&str> = ended["released_claims"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["claim_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            released,
            [
                one["claim_id"].as_str().unwrap(),
                two["claim_id"].as_str().unwrap()
            ]
        );
        assert!(listed(&s, "TIN-1").is_empty());
        assert_eq!(holders(&json!(listed(&s, "TIN-2"))), [B]);
        assert_eq!(claim(&s, C, "TIN-1", true)["outcome"], "recorded");
        let history = s
            .claims(&ClaimsQuery {
                holder: Some(A),
                all: true,
                ..ClaimsQuery::default()
            })
            .unwrap();
        for c in history["claims"].as_array().unwrap() {
            assert_eq!(c["state"], "released");
            assert_eq!(c["end_reason"], "session_ended");
        }
        // A wrong token ends nothing and releases nothing.
        let held = claim(&s, B, "TIN-3", true);
        let wrong = s.end(B, &"0".repeat(64)).unwrap();
        assert_eq!(wrong["state"], "unknown");
        assert!(wrong.get("released_claims").is_none());
        assert_eq!(listed(&s, "TIN-3")[0]["claim_id"], held["claim_id"]);
    }

    #[test]
    fn release_acts_only_on_the_callers_own_claim() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", true);
        let id = held["claim_id"].as_str().unwrap();
        let other = s.release(B, id).unwrap_err();
        assert_eq!(other.kind, ErrorKind::ClaimUnavailable);
        assert_eq!(listed(&s, "TIN-1").len(), 1);
        let done = s.release(A, id).unwrap();
        assert_eq!(done["state"], "released");
        assert_eq!(done["released"], true);
        let again = s.release(A, id).unwrap();
        assert_eq!(again["state"], "released");
        assert_eq!(again["released"], false);
        assert!(listed(&s, "TIN-1").is_empty());
        assert_eq!(
            s.release(A, &Ulid::new().to_string()).unwrap_err().kind,
            ErrorKind::ClaimUnavailable
        );
    }

    #[test]
    fn a_handoff_moves_the_claim_only_when_the_receiver_accepts() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", true);
        let source = held["claim_id"].as_str().unwrap();
        let offer = s
            .handoff(&json!({"me":A,"claim_id":source,"to":B,"note":"yours"}))
            .unwrap();
        assert_eq!(offer["state"], "offered");
        let hid = offer["handoff_id"].as_str().unwrap();
        // The same offer again is the open receipt, not a second one.
        let repeat = s
            .handoff(&json!({"me":A,"claim_id":source,"to":B}))
            .unwrap();
        assert_eq!(repeat["handoff_id"], hid);
        assert_eq!(repeat["created"], false);
        // The offer alone moves nothing.
        assert_eq!(listed(&s, "TIN-1")[0]["holder"], A);
        let open = s
            .claims(&ClaimsQuery {
                me: Some(B),
                ..ClaimsQuery::default()
            })
            .unwrap();
        assert_eq!(open["handoffs"][0]["handoff_id"], hid);
        // Only the named receiver can accept.
        let stolen = s
            .claim(&json!({"me":C,"handoff_id":hid,"exclusive":true}))
            .unwrap_err();
        assert_eq!(stolen.kind, ErrorKind::ClaimUnavailable);
        let accepted = s
            .claim(&json!({"me":B,"handoff_id":hid,"exclusive":true}))
            .unwrap();
        assert_eq!(accepted["outcome"], "recorded");
        assert_eq!(accepted["subject"], "TIN-1");
        assert_eq!(accepted["handoff_from"], source);
        assert_eq!(accepted["overlaps"], json!([]));
        let now = listed(&s, "TIN-1");
        assert_eq!(now.len(), 1);
        assert_eq!(now[0]["holder"], B);
        assert_eq!(now[0]["handoff_from"], source);
        assert_eq!(
            s.claim(&json!({"me":B,"handoff_id":hid})).unwrap_err().kind,
            ErrorKind::ClaimUnavailable
        );
        assert_eq!(s.release(A, source).unwrap()["state"], "handed_off");
        // Handing off another session's claim is refused.
        let theirs = accepted["claim_id"].as_str().unwrap();
        assert_eq!(
            s.handoff(&json!({"me":A,"claim_id":theirs,"to":C}))
                .unwrap_err()
                .kind,
            ErrorKind::ClaimUnavailable
        );
    }

    #[test]
    fn a_refused_acceptance_leaves_the_source_claim_in_place() {
        let (s, _) = clocked();
        let held = claim(&s, A, "TIN-1", false);
        claim(&s, C, "TIN-1", false);
        let source = held["claim_id"].as_str().unwrap();
        let hid = s
            .handoff(&json!({"me":A,"claim_id":source,"to":B}))
            .unwrap()["handoff_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let refused = s
            .claim(&json!({"me":B,"handoff_id":hid,"exclusive":true}))
            .unwrap();
        assert_eq!(refused["outcome"], "refused");
        assert_eq!(holders(&refused["held_by"]), [C]);
        assert_eq!(holders(&json!(listed(&s, "TIN-1"))), [A, C]);
        // The offer stays open; a non-exclusive acceptance then succeeds.
        let ok = s.claim(&json!({"me":B,"handoff_id":hid})).unwrap();
        assert_eq!(ok["outcome"], "recorded");
        assert_eq!(holders(&json!(listed(&s, "TIN-1"))), [C, B]);
        // Releasing a claim cancels its open offers.
        let mine = ok["claim_id"].as_str().unwrap();
        let offer = s.handoff(&json!({"me":B,"claim_id":mine,"to":A})).unwrap();
        s.release(B, mine).unwrap();
        assert_eq!(
            s.claim(&json!({"me":A,"handoff_id":offer["handoff_id"]}))
                .unwrap_err()
                .kind,
            ErrorKind::ClaimUnavailable
        );
    }

    #[test]
    fn a_gone_holder_shows_orphaned_and_still_holds() {
        let (s, clock) = clocked();
        s.claim(&json!({"me":A,"subject":"TIN-1","exclusive":true,"lease_hours":24}))
            .unwrap();
        clock.advance(21_601);
        reg(&s, B);
        let row = &listed(&s, "TIN-1")[0];
        assert_eq!(row["state"], "orphaned");
        assert_eq!(row["orphaned"], true);
        let refused = claim(&s, B, "TIN-1", true);
        assert_eq!(refused["outcome"], "refused");
        assert_eq!(refused["held_by"][0]["orphaned"], true);
        // The holder's own call revives it and clears orphaned.
        s.heartbeat(A).unwrap();
        assert_eq!(listed(&s, "TIN-1")[0]["state"], "active");
    }

    #[test]
    fn claim_needs_a_registered_caller_and_valid_fields() {
        let (s, _) = clocked();
        let unregistered = s
            .claim(&json!({"me":"pi:x:9:z","subject":"TIN-1"}))
            .unwrap_err();
        assert_eq!(unregistered.kind, ErrorKind::Unregistered);
        for bad in [
            json!({"me":A}),
            json!({"me":A,"subject":"has space"}),
            json!({"me":A,"subject":"x".repeat(257)}),
            json!({"me":A,"subject":"TIN-1","exclusive":"yes"}),
            json!({"me":A,"subject":"TIN-1","bogus":1}),
            json!({"me":A,"subject":"TIN-1","note":"a\u{7}b"}),
            json!({"me":A,"subject":"TIN-1","handoff_id":"nope"}),
        ] {
            assert_eq!(s.claim(&bad).unwrap_err().kind, ErrorKind::Invalid, "{bad}");
        }
        assert!(valid_subject("lab:nix/hosts/sting.nix#L10"));
    }

    #[test]
    fn a_claim_refreshes_the_callers_lease() {
        let (s, clock) = clocked();
        clock.advance(901);
        claim(&s, A, "TIN-1", false);
        let peers = s
            .peers(&PeersQuery {
                all: true,
                ..PeersQuery::default()
            })
            .unwrap();
        let state = |id: &str| {
            peers["peers"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["agent_id"] == id)
                .unwrap()["state"]
                .clone()
        };
        assert_eq!(state(A), "live");
        assert_eq!(state(B), "idle");
    }

    #[test]
    fn heartbeat_and_thread_need_a_registered_caller() {
        let (s, _) = clocked();
        assert_eq!(
            s.heartbeat("pi:x:9:z").unwrap_err().kind,
            ErrorKind::Unregistered
        );
        assert_eq!(s.heartbeat(A).unwrap()["state"], "live");
        let first = s
            .send(&json!({"from":A,"to":B,"ticket":"none","body":"one"}))
            .unwrap()
            .envelope;
        let thread = first["thread_id"].as_str().unwrap().to_owned();
        s.send(&json!({"from":B,"to":A,"ticket":"none","body":"two","thread_id":thread}))
            .unwrap();
        s.send(&json!({"from":C,"to":B,"ticket":"none","body":"other","thread_id":thread}))
            .unwrap();
        let read = s.thread(A, &thread, None, None).unwrap();
        let bodies: Vec<&str> = read["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["body"].as_str().unwrap())
            .collect();
        // C's message to B is not A's to read.
        assert_eq!(bodies, ["one", "two"]);
        assert_eq!(read["messages"][0]["state"], "queued");
        let page = s.thread(B, &thread, None, Some(2)).unwrap();
        assert_eq!(page["messages"].as_array().unwrap().len(), 2);
        assert_eq!(page["next_after_seq"], 2);
        let rest = s.thread(B, &thread, Some(2), Some(2)).unwrap();
        assert_eq!(rest["messages"][0]["body"], "other");
        assert_eq!(rest["next_after_seq"], Value::Null);
        assert_eq!(
            s.thread("pi:x:9:z", &thread, None, None).unwrap_err().kind,
            ErrorKind::Unregistered
        );
    }

    #[test]
    fn retention_prunes_finished_claims_after_seven_days() {
        let (s, clock) = clocked();
        let id = claim(&s, A, "TIN-1", false)["claim_id"]
            .as_str()
            .unwrap()
            .to_owned();
        s.release(A, &id).unwrap();
        claim(&s, B, "TIN-2", false);
        // The released claim ended now; the other expires in 4 h.
        clock.advance(604_800 + 4 * 3600);
        s.prune().unwrap();
        let all = s
            .claims(&ClaimsQuery {
                all: true,
                ..ClaimsQuery::default()
            })
            .unwrap();
        assert!(all["claims"].as_array().unwrap().is_empty());
        assert!(s.metrics().unwrap().contains("swb_claims_active 0"));
    }

    #[test]
    fn metrics_count_active_claims_and_handoffs() {
        let (s, _) = clocked();
        let id = claim(&s, A, "TIN-1", true)["claim_id"]
            .as_str()
            .unwrap()
            .to_owned();
        s.handoff(&json!({"me":A,"claim_id":id,"to":B})).unwrap();
        let text = s.metrics().unwrap();
        assert!(text.contains("swb_claims_active 1"), "{text}");
        assert!(text.contains("swb_handoffs_total 1"), "{text}");
        assert!(text.contains("swb_claim_overlaps 0"), "{text}");
        claim(&s, B, "TIN-1", false);
        assert!(s.metrics().unwrap().contains("swb_claim_overlaps 2"));
    }
}
