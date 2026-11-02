-- Ruled broker constants that the QuickCheck model (spec/haskell) reads.
--
-- Single source of truth for the model: `just spec-dhall` renders this to
-- generated/broker-constants.json and fails if the committed copy is stale;
-- the Haskell suite reads only that JSON. The Rust crates keep their own
-- literals (they are release-protected inputs, SWB-R53); the live adapter is
-- what compares the two. R-C229.
let ClaimRule =
      -- R-C950 (TIN-5770 comment 23fdf965) rules the SWB-R16 cases that were
      -- "Not yet ruled". The model implements each and the properties
      -- exclusiveRefusal and noOrphanAfterExpiry assert them:
      --   exclusive_refused_while_any_other_claim_is_held: an exclusive
      --     request while another session holds any active claim on the
      --     subject is refused with held_by listing the holders; nothing is
      --     recorded.
      --   nonexclusive_recorded_with_exclusive_holder_in_overlaps: a
      --     non-exclusive request is always recorded, and every other holder,
      --     an exclusive one included, is listed in overlaps[].
      --   exclusive_repeat_by_holder_renews: a repeat exclusive request by the
      --     current exclusive holder is an idempotent renewal (same claim id,
      --     expiry extended, never shortened).
      --   end_releases_holder_claims: ending a holder session releases its
      --     claims at once, and the broker logs each release.
      -- SWB-R61 (R-C655, TIN-4655 comment 18c7bd1c) rules the cases R-C950
      -- leaves open; the same two properties assert it:
      --   holder_has_one_claim_per_subject: a repeat request by a holder
      --     updates its one active claim on the subject (same claim id, the
      --     later expiry); an exclusive request over its own advisory claim,
      --     with no other holder, makes it exclusive and is recorded; an
      --     advisory request never drops exclusivity.
      < exclusive_refused_while_any_other_claim_is_held
      | nonexclusive_recorded_with_exclusive_holder_in_overlaps
      | exclusive_repeat_by_holder_renews
      | end_releases_holder_claims
      | holder_has_one_claim_per_subject
      >

let LeaseState =
      -- peers() classification in swb-store: ended wins, then age bands.
      < live | idle | gone | ended >

in  { rulings = [ "SWB-R09", "SWB-R10", "SWB-R14", "SWB-R16", "SWB-R46", "R-C229", "R-C655", "R-C944", "R-C950", "SWB-R61", "SWB-R66", "SWB-R67", "SWB-R68" ]
    , session =
      { lease_seconds = 900
      , live_max_age_seconds = 900
      , idle_max_age_seconds = 21600
      , states = [ LeaseState.live, LeaseState.idle, LeaseState.gone, LeaseState.ended ]
      , -- R-C944 (v0.3.0): register returns a 64-hex end token; end accepts
        -- nothing else. Every register rotates it and invalidates the old one,
        -- except for a bound session as amended under binding below: a
        -- register inside a grace returns the current end token, and while
        -- the replaced session token is in its grace, the end token
        -- replaced with it still ends the session. A rotation that gives
        -- the replaced token no grace (the chain is spent) gives its end
        -- token none either.
        end_token_hex_length = 64
      }
    , peers =
      -- R-C944 (v0.3.0): peers lists live sessions unless asked for all, one
      -- row per (harness, host, session id) (the latest registration wins),
      -- paginated, and never returns proc_start.
      { default_states = [ LeaseState.live ]
      , default_limit = 50
      , max_limit = 200
      , dedup_key = [ "harness", "host", "session_id" ]
      }
    , message =
      { ttl_default_hours = 72
      , ttl_min_hours = 1
      , ttl_max_hours = 336
      , retention_acked_seconds = 604800
      , retention_unacked_seconds = 2592000
      , inbox_default_limit = 20
      , inbox_max_limit = 100
      , body_max_bytes = 16384
      , artifacts_max = 20
      }
    , binding =
      -- SWB-R66 to SWB-R68 (ADR-0003, v0.5.0): opt-in sender binding. A
      -- bound (harness, host, session id) needs its 64-hex session token on
      -- the acting operations below and on any new register of the key;
      -- every register rotates it. A binding lapses when its session ends or
      -- goes (idle_max_age_seconds without a call). The send bucket defaults
      -- are the implementing PR's choice; swb serve reads SWB_SEND_BURST and
      -- SWB_SEND_REFILL_PER_MINUTE, and a burst of 0 turns the limit off.
      -- SWB-R67 as amended 2026-11-02 (Jess, "Short grace for old token",
      -- TIN-7577 comment 41c4704d): the token a rotation replaced stays
      -- valid for session_token_grace_seconds, then never again. Only the
      -- token the latest rotation replaced counts, so a token two
      -- rotations old is refused. Refined the same day ("Grace returns
      -- current token", comment ff9be66f): a register with the replaced
      -- token returns the current session and end tokens and mints
      -- nothing. "Allow one more grace" (comment 2f1439dc): a token a grace
      -- handed back gets one more grace of its own when it is replaced, and
      -- a token handed back in that second grace gets none, so a chain has
      -- session_token_grace_chain graces. A token a minting register
      -- returned starts a new chain. The end token has the same grace
      -- (comment 3955641b). The grace ends
      -- on the wall or the monotonic clock, whichever is first, and a
      -- broker restart ends it. The store fixes the value
      -- (SESSION_TOKEN_GRACE_SECONDS and SESSION_TOKEN_GRACE_CHAIN), a
      -- swb-store test ties both to the rendered broker-constants.json,
      -- and no setting raises either.
      { session_token_hex_length = 64
      , session_token_grace_seconds = 60
      , session_token_grace_chain = 2
      , delivery_key = [ "harness", "host", "session_id" ]
      , token_required_for =
        [ "send", "inbox", "ack", "claim", "release", "handoff", "end", "register" ]
      , send_burst_default = 20
      , send_refill_per_minute_default = 30
      }
    , hook =
      -- SWB-R10: a harness hook never blocks the harness and always exits 0.
      -- Amended 2026-11-02 (Jess, "Raise to 3.5 seconds (Recommended)",
      -- TIN-7577 comment 2f1439dc): `swb hook` bounds its own work at
      -- limit_ms, which leaves room for a bound register and its one retry
      -- (TIN-7577 comment 3955641b), each with the full request_deadline_ms.
      -- The harness kills a hook after harness_timeout_seconds, the hook
      -- entries' "timeout", which must stay above limit_ms. The swb crate
      -- keeps its own literals (HOOK_LIMIT, LIMIT and swb-broker's
      -- HOOK_TIMEOUT_MS), and a swb test ties them to the rendered JSON.
      { request_deadline_ms = 1500
      , limit_ms = 3500
      , harness_timeout_seconds = 4
      }
    , claim =
      { lease_default_hours = 4
      , lease_max_hours = 24
      , rules =
        [ ClaimRule.exclusive_refused_while_any_other_claim_is_held
        , ClaimRule.nonexclusive_recorded_with_exclusive_holder_in_overlaps
        , ClaimRule.exclusive_repeat_by_holder_renews
        , ClaimRule.end_releases_holder_claims
        , ClaimRule.holder_has_one_claim_per_subject
        ]
      }
    }
