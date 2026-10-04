-- | Five parsimonious trace properties (R-C229). Each one replays a trace of
-- (operation, result) pairs and returns the violations it finds. They judge
-- results only, so the same checks run against the pure model and against
-- the live broker through Swb.Live.
--
--   1. leaseLifecycle      SWB-R09/peers(): observed lease states change only
--                          along legal transitions.
--   2. threadSeqDense      SWB-R14: per-thread seq is 1, 2, 3, ..., even after prune.
--   3. atLeastOnceUntilAck SWB-R09: no ack means redelivered; ack means never again.
--   4. exclusiveRefusal    SWB-R16: the only refusal is a second exclusive claim.
--   5. noOrphanAfterExpiry SWB-R16: listed claims are unexpired, and orphaned
--                          exactly when the holder is gone.
--
-- Unruled SWB-R16 cases are never asserted.
module Swb.Properties
  ( Mode (..)
  , genOps
  , shrinkOps
  , normalizeAcks
  , properties
  ) where

import Data.List (sort)
import qualified Data.Map.Strict as M
import Swb.Constants
import Swb.Model
import Test.QuickCheck hiding (classify)

-- | The live broker has no claims yet, so neither live mode generates claim
-- operations. 'LiveMode' runs against the wall clock and generates no
-- 'Tick'. 'LiveClockMode' runs against a broker started with the R-C262 test
-- clock (`swb_test_clock`, `SWB_TEST_CLOCK=1`) and generates 'Tick', so the
-- live run covers expiry, idle, gone and retention.
data Mode = ModelMode | LiveMode | LiveClockMode deriving (Eq, Show)

genOps :: Mode -> Gen [Op]
genOps mode = normalizeAcks <$> listOf (frequency (common ++ extra))
  where
    agent = Agent <$> choose (0, 2)
    common =
      [ (3, Register <$> agent <*> choose (0, 1))
      , (1, End <$> agent <*> choose (0, 1))
      , (3, Peers <$> agent)
      , (5, Send <$> agent <*> agent <*> (Thread <$> choose (0, 2)) <*> oneof [pure Nothing, Just <$> elements [0, 1, 2, 72, 336, 337, 10000]])
      , (4, Inbox <$> agent <*> elements [1, 2, 3, 20, 100, 101])
      , (4, Ack <$> agent <*> choose (0, 20))
      ]
    tick = (3, Tick <$> elements [1, 60, 899, 900, 901, 3600, 14399, 14400, 21600, 21601, 86400, 259200, 604801, 1296000, 2678400])
    extra = case mode of
      LiveMode -> []
      LiveClockMode -> [tick]
      ModelMode ->
        [ (3, Claim <$> agent <*> (Subject <$> choose (0, 1)) <*> arbitrary <*> oneof [pure Nothing, Just <$> elements [1, 4, 24, 25]])
        , (2, pure ListClaims)
        , tick
        ]

shrinkOps :: [Op] -> [[Op]]
shrinkOps = map normalizeAcks . shrinkList (const [])

-- | Point every 'Ack' at a message that was sent earlier in the list, or at
-- -1 (an id that was never sent) when none was. Every 'Send' creates exactly
-- one message, so the n-th Send is message n-1 in both implementations.
normalizeAcks :: [Op] -> [Op]
normalizeAcks = go 0
  where
    go _ [] = []
    go n (o@Send {} : os) = o : go (n + 1) os
    go n (Ack a k : os) = Ack a (if n == 0 then -1 else k `mod` n) : go n os
    go n (o : os) = o : go n os

-- | Session facts the trace itself reveals: last refresh time and ended.
data Seen = Seen {seenAt :: Integer, seenEnded :: Bool}

-- | The clock before each step, from the 'Tick' operations that succeeded.
clocked :: Trace -> [(Integer, Op, Res)]
clocked = go 0
  where
    go _ [] = []
    go t ((o, r) : rest) = (t, o, r) : go (case (o, r) of (Tick d, RTicked) -> t + d; _ -> t) rest

-- | Which agent's lease an operation refreshed, judged from its result.
refreshed :: Op -> Res -> Maybe Agent
refreshed (Peers a) (RPeers _) = Just a
refreshed (Send a _ _ _) (RSent {}) = Just a
refreshed (Inbox a _) (RInbox _) = Just a
refreshed (Ack a _) RAcked = Just a
refreshed (Claim a _ _ _) (RClaimed _) = Just a
refreshed _ _ = Nothing

-- | Track sessions through one step. Register creates; refreshes touch only
-- registered agents; a successful End marks ended.
trackSessions :: Integer -> Op -> Res -> M.Map Agent Seen -> M.Map Agent Seen
trackSessions t o r m = case (o, r) of
  (Register a _, RRegistered) -> M.insert a (Seen t False) m
  (End a _, REnded True) -> M.adjust (\x -> x {seenEnded = True}) a m
  _ -> maybe m (\a -> M.adjust (const (Seen t False)) a m) (refreshed o r)

rank :: LeaseState -> Int
rank Live = 0
rank Idle = 1
rank Gone = 2
rank Ended = 3

-- | 1. Each observed state equals the band its own last refresh or end
-- implies, and between observations with no own activity a state only
-- decays: live -> idle -> gone, ended stays ended.
leaseLifecycle :: Constants -> Trace -> [String]
leaseLifecycle c tr = go M.empty M.empty (clocked tr)
  where
    go _ _ [] = []
    go sess prev ((t, o, r) : rest) =
      let sess' = trackSessions t o r sess
          prev' = case o of
            Register a _ -> M.delete a prev -- a register may revive any state
            _ -> maybe prev (`M.delete` prev) (refreshed o r)
          prevE = case (o, r) of
            (End a _, REnded True) -> M.delete a prev'
            _ -> prev'
       in case r of
            RPeers obs ->
              let bad =
                    [ "agent " ++ show a ++ " observed " ++ show st ++ " expected " ++ show want ++ maybe "" ((" after " ++) . show) (M.lookup a prevE)
                    | (a, st) <- obs
                    , Just x <- [M.lookup a sess']
                    , let want = classify c t (seenAt x) (seenEnded x)
                    , st /= want || maybe False (\p -> not (legal p st)) (M.lookup a prevE)
                    ]
               in bad ++ go sess' (M.union (M.fromList obs) prevE) rest
            _ -> go sess' prevE rest
    legal Ended st = st == Ended
    legal p st = st /= Ended && rank st >= rank p

-- | 2. For every thread, the seq values of created messages are 1, 2, 3, ...
threadSeqDense :: Trace -> [String]
threadSeqDense tr =
  [ "thread " ++ show th ++ " seqs " ++ show sqs
  | (th, sqs) <- M.toList (M.fromListWith (flip (++)) [(th, [sq]) | (_, RSent _ th sq) <- tr])
  , sqs /= [1 .. fromIntegral (length sqs)]
  ]

-- | 3. Inbox returns only pending messages (addressed to me, unacked,
-- unexpired) with delivery_count one higher than last time; when it returns
-- fewer than the limit it returns every pending message. Ack succeeds
-- exactly for my unexpired or already acked messages.
atLeastOnceUntilAck :: Constants -> Trace -> [String]
atLeastOnceUntilAck c tr = go M.empty M.empty M.empty (clocked tr)
  where
    -- sent: index -> (recipient, expires); acked: index -> acked at; dc: deliveries
    go _ _ _ [] = []
    go sent acked dc ((t, o, r) : rest) = case (o, r) of
      (Send _ to _ ttl, RSent i _ _) -> go (M.insert i (to, t + ttlSeconds c ttl) sent) acked dc rest
      (Ack a i, _) ->
        let mine = maybe False ((== a) . fst) (M.lookup i sent)
            ok = mine && (M.member i acked || maybe False ((> t) . snd) (M.lookup i sent))
            -- an acked row may be pruned once it is 7 d old; not asserted then
            prunable = maybe False (\at -> t - at >= cRetentionAcked c) (M.lookup i acked)
            bad = ["ack " ++ show (a, i) ++ " gave " ++ show r ++ " expected success " ++ show ok | not prunable, ok /= (r == RAcked)]
            acked' = if r == RAcked && mine && not (M.member i acked) then M.insert i t acked else acked
         in bad ++ go sent acked' dc rest
      (Inbox a limit, RInbox got) ->
        let isPending i = case M.lookup i sent of
              Just (to, ex) -> to == a && not (M.member i acked) && ex > t
              Nothing -> False
            want = sort [i | i <- M.keys sent, isPending i]
            ids = map fst got
            unsound = ["inbox " ++ show a ++ " returned non-pending " ++ show i | i <- ids, not (isPending i)]
            counts = ["message " ++ show i ++ " delivery_count " ++ show d ++ " expected " ++ show (M.findWithDefault 0 i dc + 1) | (i, d) <- got, d /= M.findWithDefault 0 i dc + 1]
            incomplete =
              [ "inbox " ++ show a ++ " returned " ++ show (sort ids) ++ " of pending " ++ show want
              | fromIntegral (length got) < min limit (cInboxMaxLimit c)
              , sort ids /= want
              ]
         in unsound ++ counts ++ incomplete ++ go sent acked (M.union (M.fromList got) dc) rest
      _ -> go sent acked dc rest

-- | Claims that are recorded and unexpired at time t.
data Rec = Rec {rHolder :: Agent, rSubject :: Subject, rExclusive :: Bool, rUntil :: Integer} deriving (Eq, Show)

replayClaims :: Constants -> Integer -> Op -> Res -> [Rec] -> [Rec]
replayClaims c t (Claim h s ex lease) (RClaimed Recorded) rs = rs ++ [Rec h s ex (t + claimLeaseSeconds c lease)]
replayClaims _ _ _ _ rs = rs

-- | 4. A non-exclusive claim is always recorded. An exclusive claim is
-- refused with held_by naming another unexpired exclusive holder exactly when
-- one exists; otherwise it is recorded. Unruled cases are skipped.
exclusiveRefusal :: Constants -> Trace -> [String]
exclusiveRefusal c tr = go [] (clocked tr)
  where
    go _ [] = []
    go rs ((t, o, r) : rest) =
      let bad = case (o, r) of
            (Claim h s ex _, RClaimed out) ->
              let active = [x | x <- rs, rSubject x == s, rUntil x > t]
                  holders = [rHolder x | x <- active, rExclusive x, rHolder x /= h]
                  unruled = ex && (any (\x -> rExclusive x && rHolder x == h) active || (null holders && not (null active)))
                  ok
                    | unruled = True
                    | not ex = out == Recorded
                    | null holders = out == Recorded
                    | otherwise = case out of HeldBy x -> x `elem` holders; _ -> False
               in ["claim " ++ show (h, s, ex) ++ " gave " ++ show out | not ok]
            _ -> []
       in bad ++ go (replayClaims c t o r rs) rest

-- | 5. ListClaims shows exactly the unexpired recorded claims, and a claim is
-- orphaned when its holder is gone and not orphaned while the holder is live
-- or idle. Ended or unregistered holders are not asserted (SWB-R16 is silent).
noOrphanAfterExpiry :: Constants -> Trace -> [String]
noOrphanAfterExpiry c tr = go M.empty [] (clocked tr)
  where
    go _ _ [] = []
    go sess rs ((t, o, r) : rest) =
      let sess' = trackSessions t o r sess
          bad = case r of
            RClaims views ->
              let active = [(rHolder x, rSubject x, rExclusive x) | x <- rs, rUntil x > t]
                  shown = [(cvHolder v, cvSubject v, cvExclusive v) | v <- views]
                  set = ["claims shown " ++ show (sort shown) ++ " expected " ++ show (sort active) | sort shown /= sort active]
                  orphan =
                    [ "claim " ++ show v ++ " holder " ++ show st
                    | v <- views
                    , Just x <- [M.lookup (cvHolder v) sess']
                    , let st = classify c t (seenAt x) (seenEnded x)
                    , st /= Ended
                    , cvOrphaned v /= (st == Gone)
                    ]
               in set ++ orphan
            _ -> []
       in bad ++ go sess' (replayClaims c t o r rs) rest

-- | Named trace checks. Swb.Live and the test driver wrap them.
properties :: Constants -> [(String, Trace -> [String])]
properties c =
  [ ("leaseLifecycle", leaseLifecycle c)
  , ("threadSeqDense", threadSeqDense)
  , ("atLeastOnceUntilAck", atLeastOnceUntilAck c)
  , ("exclusiveRefusal", exclusiveRefusal c)
  , ("noOrphanAfterExpiry", noOrphanAfterExpiry c)
  ]
