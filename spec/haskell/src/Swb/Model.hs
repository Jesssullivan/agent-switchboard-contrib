-- | A pure, executable model of the broker's coordination state (R-C229).
--
-- It follows crates/swb-store for sessions, threads and messages, and the
-- SWB-R16 text in ADR-0001 for claims, which are not implemented in Rust yet.
-- Time is an abstract clock in seconds that only 'Tick' advances.
--
-- v0.3.0 (R-C944): register returns an end token and only that token ends
-- the session; every register rotates it. peers lists live sessions unless
-- asked for all, one row per (harness, host, session id) (the latest
-- registration wins); every model agent has the same harness and host, so
-- the session id alone decides. Retention runs on an interval, which the
-- model takes at each 'Tick', not inside send or inbox.
--
-- The core invariants (per-thread seq, lease transitions, claim listing)
-- live in Swb.Invariants as LiquidHaskell refinement types (R-C261), and
-- the model runs those definitions.
--
-- The model is one implementation of 'Op' -> 'Res'; the live adapter
-- (Swb.Live) is another. The properties in Swb.Properties judge a trace of
-- operations and results and never look inside either implementation.
module Swb.Model
  ( Agent (..)
  , Thread (..)
  , Subject (..)
  , LeaseState (..)
  , Op (..)
  , Res (..)
  , TokRef (..)
  , ClaimOutcome (..)
  , ClaimView (..)
  , Trace
  , classify
  , runModel
  , sessionOf
  ) where

import Data.List (sortOn)
import qualified Data.Map.Strict as M
import Swb.Constants
import Swb.Invariants

type Time = Integer

-- | Which end token an 'End' presents, from what the caller was given: the
-- latest token its registrations returned, the newest earlier one that
-- differs from it, or one it was never given.
data TokRef = LatestTok | PriorTok | ForgedTok deriving (Eq, Show)

data Op
  = Register Agent Int -- ^ agent, proc_start
  | End Agent TokRef -- ^ agent, the end token presented
  | Peers Agent Bool -- ^ peers(me, all): refreshes me, then lists live (or all) sessions
  | Send Agent Agent Thread (Maybe Integer) -- ^ from, to, thread, ttl_hours
  | Inbox Agent Integer -- ^ me, limit
  | Ack Agent Int -- ^ me, abstract message index (-1: an id never sent)
  | Claim Agent Subject Bool (Maybe Integer) -- ^ holder, subject, exclusive, lease hours
  | ListClaims
  | Tick Integer -- ^ advance the clock (model only)
  deriving (Eq, Show)

data ClaimOutcome
  = Recorded
  | HeldBy Agent
  | Unruled String -- ^ an SWB-R16 "Not yet ruled" case; never asserted
  deriving (Eq, Show)

data Res
  = RRegistered String -- ^ the end token returned
  | REnded Bool -- ^ True when the session moved to ended
  | RPeers [(Agent, LeaseState)]
  | RSent Int Thread Integer -- ^ abstract message index, thread, seq
  | RInbox [(Int, Integer)] -- ^ (message index, delivery_count)
  | RAcked
  | RClaimed ClaimOutcome
  | RClaims [ClaimView]
  | RTicked
  | RUnsupported
  | RErr String
  deriving (Eq, Show)

type Trace = [(Op, Res)]

data Session = Session
  { sSeen :: Time
  , sEnded :: Bool
  , sNonce :: Int -- ^ the end token derives from it; a new one invalidates the old token
  , sReg :: Integer -- ^ registration sequence; the highest wins its session id
  }

data Msg = Msg
  { mId :: Int
  , mThread :: Thread
  , mSeq :: Integer
  , mTo :: Agent
  , mCreated :: Time
  , mExpires :: Time
  , mAcked :: Maybe Time
  , mDeliveries :: Integer
  }

data St = St
  { now :: Time
  , sessions :: M.Map Agent Session
  , msgs :: [Msg] -- creation order
  , threadLogs :: M.Map Thread [Integer] -- seq log per thread, newest first; survives prune, like thread_counters
  , nextId :: Int
  , claims :: [ClaimRec]
  , nextNonce :: Int
  , regSeq :: Integer
  , issued :: M.Map Agent [String] -- ^ tokens each caller was given, newest first
  }

classify :: Constants -> Time -> Time -> Bool -> LeaseState
classify c = classifyAge (cLiveMaxAge c) (cIdleMaxAge c)

runModel :: Constants -> [Op] -> Trace
runModel c = go (St 0 M.empty [] M.empty 0 [] 0 0 M.empty)
  where
    go _ [] = []
    go s (o : os) = let (r, s') = step c o s in (o, r) : go s' os

-- | Every broker operation by a registered agent refreshes its lease and
-- clears ended (the `UPDATE sessions SET last_seen=..., ended=0` in each
-- swb-store mutation). Claims are assumed to do the same.
refresh :: Agent -> St -> St
refresh a s = s {sessions = M.adjust (\x -> x {sSeen = now s, sEnded = False}) a (sessions s)}

-- | swb-store prune(): acked rows after 7 d, every other row 30 d after creation.
prune :: Constants -> St -> St
prune c s = s {msgs = filter keep (msgs s)}
  where
    keep m = case mAcked m of
      Just at -> at > now s - cRetentionAcked c
      Nothing -> mCreated m > now s - cRetentionUnacked c

-- | The session id part of an agent id. Agents 0 and 1 share one (and, like
-- every model agent, a harness and host), so a re-registration from another
-- process supersedes the earlier row in peers.
sessionOf :: Agent -> Int
sessionOf (Agent a) = a `div` 2

token :: Agent -> Int -> String
token (Agent a) n = "t" ++ show a ++ "-" ++ show n

-- | The token an 'End' presents, resolved against what the caller holds.
resolveTok :: TokRef -> [String] -> Maybe String
resolveTok LatestTok (t : _) = Just t
resolveTok PriorTok (t : older) = case filter (/= t) older of
  (p : _) -> Just p
  [] -> Just t
resolveTok _ _ = Nothing

-- | peers(): one row per session id, the highest registration sequence
-- winning; then live only unless all.
visible :: Constants -> Bool -> St -> [(Agent, LeaseState)]
visible c everything s =
  [ (b, st)
  | (b, x) <- M.toList (sessions s)
  , all (\(b', y) -> sessionOf b' /= sessionOf b || sReg y <= sReg x) (M.toList (sessions s))
  , let st = classify c (now s) (sSeen x) (sEnded x)
  , everything || st == Live
  ]

pending :: St -> Agent -> Msg -> Bool
pending s a m = mTo m == a && mAcked m == Nothing && mExpires m > now s

step :: Constants -> Op -> St -> (Res, St)
step c op s = case op of
  Register a _ -> -- proc_start does not matter: every register rotates the token
    let n = nextNonce s
        s1 = s {nextNonce = n + 1}
        sq = regSeq s1 + 1
        tok = token a n
     in ( RRegistered tok
        , s1
            { sessions = M.insert a (Session (now s1) False n sq) (sessions s1)
            , regSeq = sq
            , issued = M.insertWith (++) a [tok] (issued s1)
            }
        )
  End a ref -> case (M.lookup a (sessions s), resolveTok ref (M.findWithDefault [] a (issued s))) of
    (Just x, Just t) | t == token a (sNonce x) -> (REnded True, s {sessions = M.insert a x {sEnded = True} (sessions s)})
    _ -> (REnded False, s)
  Peers a everything
    | M.member a (sessions s) ->
        let s' = refresh a s
         in (RPeers (visible c everything s'), s')
    | otherwise -> (RErr "unregistered me", s)
  Send from to th ttl ->
    let s1 = s
        lg = appendSeq (M.findWithDefault [] th (threadLogs s1))
        sq = newest lg
        m = Msg (nextId s1) th sq to (now s1) (now s1 + ttlSeconds c ttl) Nothing 0
        s2 = s1 {msgs = msgs s1 ++ [m], threadLogs = M.insert th lg (threadLogs s1), nextId = nextId s1 + 1}
     in (RSent (mId m) th sq, refresh from s2)
  Inbox a limit ->
    let s1 = s
        picked = take (fromInteger (min limit (cInboxMaxLimit c))) (sortOn (\m -> (mCreated m, mSeq m, mId m)) (filter (pending s1 a) (msgs s1)))
        ids = map mId picked
        bump m = if mId m `elem` ids then m {mDeliveries = mDeliveries m + 1} else m
        s2 = s1 {msgs = map bump (msgs s1)}
        counts = [(mId m, mDeliveries m) | m <- msgs s2, mId m `elem` ids]
        ordered = [(i, d) | i <- ids, Just d <- [lookup i counts]]
     in (RInbox ordered, refresh a s2)
  Ack a i -> case [m | m <- msgs s, mId m == i, mTo m == a] of
    [m]
      | pending s a m -> (RAcked, refresh a s {msgs = map (\x -> if mId x == i then x {mAcked = Just (now s)} else x) (msgs s)})
      | mAcked m /= Nothing -> (RAcked, refresh a s)
    _ -> (RErr "message unavailable", s)
  Claim h subj ex lease ->
    let active = [r | r <- claims s, crSubject r == subj, crUntil r > now s]
        others = [crHolder r | r <- active, crExclusive r, crHolder r /= h]
        record = s {claims = claims s ++ [ClaimRec h subj ex (now s + claimLeaseSeconds c lease)]}
        outcome
          | not ex = (Recorded, record)
          | any (\r -> crExclusive r && crHolder r == h) active = (Unruled "exclusive_reclaim_by_the_current_exclusive_holder", s)
          | (x : _) <- others = (HeldBy x, s)
          | not (null active) = (Unruled "exclusive_request_while_nonexclusive_claims_exist", s)
          | otherwise = (Recorded, record)
     in (RClaimed (fst outcome), refresh h (snd outcome))
  ListClaims ->
    let holder h = (\x -> classify c (now s) (sSeen x) (sEnded x)) <$> M.lookup h (sessions s)
     in (RClaims [viewClaim (holder (crHolder r)) r | r <- activeClaims (now s) (claims s)], s)
  Tick d -> (RTicked, prune c s {now = now s + d})
