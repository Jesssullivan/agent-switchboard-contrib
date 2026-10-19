{-@ LIQUID "--reflection" @-}
{-@ LIQUID "--ple" @-}

-- | The pure model's core invariants as LiquidHaskell refinement types
-- (R-C261). Swb.Model runs these definitions, so the code LiquidHaskell
-- checks is the code the QuickCheck properties exercise.
--
-- This module imports only base, so `just spec-liquid` (the flake check
-- `spec-liquid`) can check it alone with the LiquidHaskell GHC plugin. A plain
-- GHC build reads the annotations as comments.
--
-- What is proved here, and what is not:
--
--   * Per-thread seq. A thread's log is strictly decreasing newest first, and
--     its newest seq equals its length. So 'appendSeq' always appends a seq
--     above every earlier seq on that thread, and the seqs run 1, 2, 3, ....
--     The model keeps one log per thread and never prunes it, like
--     swb-store's thread_counters.
--   * Lease transitions. With no activity of its own, an agent's lease only
--     decays (live -> idle -> gone) and ended stays ended ('decayLemma').
--     End always yields ended ('endedLemma'); a refresh yields live
--     ('reviveLemma'). Those are the only edges the model's classify allows.
--   * Claims. A listed claim is unexpired ('activeClaims'), and its view is
--     orphaned exactly when its holder is gone ('viewClaim'). So a listed
--     claim's holder is not gone, or the claim is marked orphaned for
--     release. A claim operation refreshes its holder, so a holder is live
--     when the claim is recorded ('reviveLemma'). Ending a session releases
--     its claims (R-C950); the model removes them in 'End', outside this
--     module, so the QuickCheck property noOrphanAfterExpiry covers it.
--
-- Not proved: that Swb.Model threads its state through these functions
-- correctly (it is not LiquidHaskell-checked; the QuickCheck trace
-- properties cover it), and anything about the live broker.
module Swb.Invariants
  ( Agent (..)
  , Thread (..)
  , Subject (..)
  , LeaseState (..)
  , ClaimRec (..)
  , ClaimView (..)
  , lrank
  , classifyAge
  , decays
  , decayLemma
  , endedLemma
  , reviveLemma
  , newest
  , appendSeq
  , holderGone
  , activeClaims
  , viewClaim
  ) where

newtype Agent = Agent Int deriving (Eq, Ord, Show)

newtype Thread = Thread Int deriving (Eq, Ord, Show)

newtype Subject = Subject Int deriving (Eq, Ord, Show)

-- | peers() classification: ended wins, then the live and idle age bands.
data LeaseState = Live | Idle | Gone | Ended deriving (Eq, Ord, Show)

{-@ measure lrank @-}
lrank :: LeaseState -> Int
lrank Live = 0
lrank Idle = 1
lrank Gone = 2
lrank Ended = 3

-- * Lease transitions

{-@ reflect classifyAge @-}
classifyAge :: Integer -> Integer -> Integer -> Integer -> Bool -> LeaseState
classifyAge liveMax idleMax t seen ended
  | ended = Ended
  | t - seen <= liveMax = Live
  | t - seen <= idleMax = Idle
  | otherwise = Gone

-- | A legal edge with no activity of the agent's own: ended stays ended, and
-- otherwise the state only moves along live -> idle -> gone.
{-@ reflect decays @-}
decays :: LeaseState -> LeaseState -> Bool
decays p q = (lrank p == 3 && lrank q == 3) || (lrank p <= lrank q && lrank q <= 2)

{-@ decayLemma
      :: liveMax:Integer -> idleMax:Integer -> seen:Integer -> ended:Bool
      -> t1:Integer -> {t2:Integer | t1 <= t2}
      -> {decays (classifyAge liveMax idleMax t1 seen ended) (classifyAge liveMax idleMax t2 seen ended)} @-}
decayLemma :: Integer -> Integer -> Integer -> Bool -> Integer -> Integer -> ()
decayLemma liveMax idleMax seen ended t1 t2
  | ended = ()
  | t1 - seen <= liveMax = bands t2
  | t1 - seen <= idleMax = bands t2
  | otherwise = bands t2
  where
    -- Split on the t2 band too, so PLE can unfold both classifyAge calls.
    bands t
      | t - seen <= liveMax = ()
      | t - seen <= idleMax = ()
      | otherwise = ()

{-@ endedLemma
      :: liveMax:Integer -> idleMax:Integer -> t:Integer -> seen:Integer
      -> {lrank (classifyAge liveMax idleMax t seen True) == 3} @-}
endedLemma :: Integer -> Integer -> Integer -> Integer -> ()
endedLemma _ _ _ _ = ()

{-@ reviveLemma
      :: {liveMax:Integer | 0 <= liveMax} -> idleMax:Integer -> t:Integer
      -> {lrank (classifyAge liveMax idleMax t t False) == 0} @-}
reviveLemma :: Integer -> Integer -> Integer -> ()
reviveLemma _ _ _ = ()

-- * Per-thread seq

-- Reflected, not a measure: a list measure attaches to (:) at every element
-- type, and PLE then fails to elaborate it on [ClaimRec].
{-@ reflect newest @-}
newest :: [Integer] -> Integer
newest [] = 0
newest (x : _) = x

-- | A thread's seq log, newest first: strictly decreasing, and the newest
-- seq is the log's length.
{-@ type SeqLog = {l:[{s:Integer | 1 <= s}]<{\newer older -> older < newer}> | newest l == len l} @-}

{-@ appendSeq :: l:SeqLog -> {v:SeqLog | newest v == newest l + 1 && len v == len l + 1} @-}
appendSeq :: [Integer] -> [Integer]
appendSeq [] = [1]
appendSeq (x : xs) = (x + 1) : x : xs

-- * Claims

{-@ data ClaimRec = ClaimRec {crHolder :: Agent, crSubject :: Subject, crExclusive :: Bool, crUntil :: Integer} @-}
data ClaimRec = ClaimRec {crHolder :: Agent, crSubject :: Subject, crExclusive :: Bool, crUntil :: Integer}

{-@ data ClaimView = ClaimView {cvHolder :: Agent, cvSubject :: Subject, cvExclusive :: Bool, cvOrphaned :: Bool} @-}
data ClaimView = ClaimView
  { cvHolder :: Agent
  , cvSubject :: Subject
  , cvExclusive :: Bool
  , cvOrphaned :: Bool
  }
  deriving (Eq, Show)

-- | The holder's classified lease, or Nothing when it never registered.
{-@ reflect holderGone @-}
holderGone :: Maybe LeaseState -> Bool
holderGone Nothing = False
holderGone (Just s) = lrank s == 2

-- | The claims that are still listed at time t: unexpired, in record order.
{-@ activeClaims :: t:Integer -> [ClaimRec] -> [{r:ClaimRec | t < crUntil r}] @-}
activeClaims :: Integer -> [ClaimRec] -> [ClaimRec]
activeClaims _ [] = []
activeClaims t (r : rs)
  | crUntil r > t = r : activeClaims t rs
  | otherwise = activeClaims t rs

{-@ viewClaim
      :: st:Maybe LeaseState -> r:ClaimRec
      -> {v:ClaimView | cvHolder v == crHolder r && cvSubject v == crSubject r
                        && cvExclusive v == crExclusive r && (cvOrphaned v <=> holderGone st)} @-}
viewClaim :: Maybe LeaseState -> ClaimRec -> ClaimView
viewClaim st r = ClaimView (crHolder r) (crSubject r) (crExclusive r) (holderGone st)
