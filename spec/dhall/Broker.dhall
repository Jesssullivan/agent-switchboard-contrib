-- Ruled broker constants that the QuickCheck model (spec/haskell) reads.
--
-- Single source of truth for the model: `just spec-dhall` renders this to
-- generated/broker-constants.json and fails if the committed copy is stale;
-- the Haskell suite reads only that JSON. The Rust crates keep their own
-- literals (they are release-protected inputs, SWB-R53); the live adapter is
-- what compares the two. R-C229.
let Unruled =
      -- SWB-R16 says "Not yet ruled" for the first two; the third is not
      -- addressed by the SWB-R16 text at all. The model answers each of them
      -- with `unruled` and no property asserts any outcome for them. They are
      -- the questions for the P2 claims ruling packet.
      < exclusive_request_while_nonexclusive_claims_exist
      | nonexclusive_claim_lists_exclusive_holder_in_overlaps
      | exclusive_reclaim_by_the_current_exclusive_holder
      >

let LeaseState =
      -- peers() classification in swb-store: ended wins, then age bands.
      < live | idle | gone | ended >

in  { rulings = [ "SWB-R09", "SWB-R14", "SWB-R16", "SWB-R46", "R-C229", "R-C944" ]
    , session =
      { lease_seconds = 900
      , live_max_age_seconds = 900
      , idle_max_age_seconds = 21600
      , states = [ LeaseState.live, LeaseState.idle, LeaseState.gone, LeaseState.ended ]
      , -- R-C944 (v0.3.0): register returns a 64-hex end token; end accepts
        -- nothing else. Every register rotates it and invalidates the old one.
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
    , claim = { lease_default_hours = 4, lease_max_hours = 24 }
    , unruled =
      [ Unruled.exclusive_request_while_nonexclusive_claims_exist
      , Unruled.nonexclusive_claim_lists_exclusive_holder_in_overlaps
      , Unruled.exclusive_reclaim_by_the_current_exclusive_holder
      ]
    }
