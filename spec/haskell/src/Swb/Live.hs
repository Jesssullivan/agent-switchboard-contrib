{-# LANGUAGE OverloadedStrings #-}

-- | Thin adapter that runs the same operations against a running broker's
-- REST API (/v1/register, /v1/end, /v1/peers, /v1/send, /v1/inbox, /v1/ack),
-- so the properties in Swb.Properties can judge the real implementation.
--
-- It only issues HTTP requests to a broker someone else started; it never
-- starts, stops or signals a process (R-N11). It refuses any URL that is not
-- loopback, so it cannot write test traffic into a shared broker.
--
-- 'Tick' posts to /v1/test/clock, which exists only on a broker built with
-- the R-C262 `test-clock` feature and started with SWB_TEST_CLOCK=1;
-- 'probeClock' checks for it before a clocked run. The claim operations
-- return 'RUnsupported', and no live mode generates them, because the broker
-- has no claims yet (SWB-R16, P2).
module Swb.Live
  ( loopbackOnly
  , probeClock
  , runLive
  ) where

import Control.Monad (foldM)
import Data.Aeson (Value (..), eitherDecode, encode, object, (.=))
import qualified Data.Aeson.Key as K
import qualified Data.Aeson.KeyMap as KM
import Data.Bits (shiftL, shiftR, (.&.))
import qualified Data.ByteString.Char8 as B8
import Data.List (isPrefixOf)
import qualified Data.Map.Strict as M
import Data.Maybe (mapMaybe)
import Data.Scientific (toBoundedInteger)
import qualified Data.Text as T
import Data.Time.Clock.POSIX (getPOSIXTime)
import qualified Data.Vector as V
import Network.HTTP.Client
import Network.HTTP.Types.Status (statusCode)
import Swb.Model
import System.Random (randomRIO)

loopbackOnly :: String -> Either String String
loopbackOnly url
  | any (`isPrefixOf` url) ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"] = Right (reverse (dropWhile (== '/') (reverse url)))
  | otherwise = Left ("refusing non-loopback broker URL " ++ show url ++ "; start a disposable broker on 127.0.0.1")

-- | Read the broker's test clock with a zero advance. Fails unless the broker
-- serves /v1/test/clock, so a clocked run never silently drops its ticks.
probeClock :: Manager -> String -> IO Integer
probeClock mgr base = do
  req <- parseRequest (base ++ "/v1/test/clock")
  resp <- httpLbs req {method = "POST", requestBody = RequestBodyLBS (encode (object ["advance_seconds" .= (0 :: Int)])), requestHeaders = [("Content-Type", "application/json")]} mgr
  case (statusCode (responseStatus resp), eitherDecode (responseBody resp)) of
    (200, Right v) | Just t <- int "now" v -> pure t
    (code, _) -> ioError (userError ("broker at " ++ base ++ " has no test clock (HTTP " ++ show code ++ "); start bazel-bin/crates/swb/swb_test_clock serve with SWB_TEST_CLOCK=1"))

data LiveSt = LiveSt
  { lsThreads :: M.Map Thread T.Text
  , lsIds :: M.Map Int T.Text -- abstract message index -> msg_id
  , lsIndex :: M.Map T.Text Int -- msg_id -> abstract message index
  , lsNext :: Int
  }

-- | Run one generated case. The host part of every agent id is a fresh
-- random tag, so cases never see each other's sessions or messages.
runLive :: Manager -> String -> [Op] -> IO Trace
runLive mgr base ops = do
  tag <- ("swbspec-" ++) . show <$> (randomRIO (0, 2 ^ (40 :: Int)) :: IO Integer)
  let host = T.pack tag
      pid (Agent a) = 40000 + a
      agentId x@(Agent a) = T.concat ["claude:", host, ":", T.pack (show (pid x)), ":s", T.pack (show a)]
      ours = M.fromList [(agentId (Agent a), Agent a) | a <- [0 .. 2]]
      post path body = call =<< fmap (\r -> r {method = "POST", requestBody = RequestBodyLBS (encode body), requestHeaders = [("Content-Type", "application/json")]}) (parseRequest (base ++ path))
      get path qs = call . setQueryString [(k, Just (B8.pack (T.unpack v))) | (k, v) <- qs] =<< parseRequest (base ++ path)
      call req = do
        resp <- httpLbs req mgr
        pure (statusCode (responseStatus resp), either (const Null) id (eitherDecode (responseBody resp)))
      stepLive (st, acc) op = do
        (r, st') <- case op of
          Register a p -> do
            (code, _) <- post "/v1/register" (object ["harness" .= ("claude" :: T.Text), "host" .= host, "pid" .= pid a, "session_id" .= T.pack ("s" ++ show (unAgent a)), "proc_start" .= procStart p])
            pure (if code == 200 then RRegistered else RErr (show code), st)
          End a p -> do
            (code, v) <- post "/v1/end" (object ["me" .= agentId a, "proc_start" .= procStart p])
            pure (if code == 200 then REnded (str "state" v == Just "ended") else RErr (show code), st)
          Peers a -> do
            (code, v) <- get "/v1/peers" [("me", agentId a)]
            let row p = (,) <$> (str "agent_id" p >>= (`M.lookup` ours)) <*> (str "state" p >>= leaseState)
            pure (if code == 200 then RPeers (mapMaybe row (arr "peers" v)) else RErr (show code), st)
          Send from to th ttl -> do
            (tid, st1) <- threadId th st
            (code, v) <- post "/v1/send" (object (["from" .= agentId from, "to" .= agentId to, "ticket" .= ("none" :: T.Text), "body" .= ("swb spec" :: T.Text), "thread_id" .= tid] ++ maybe [] (\h -> ["ttl_hours" .= h]) ttl))
            case (code, str "msg_id" v, int "seq" v) of
              (200, Just mid, Just sq) ->
                let i = lsNext st1
                 in pure (RSent i th sq, st1 {lsIds = M.insert i mid (lsIds st1), lsIndex = M.insert mid i (lsIndex st1), lsNext = i + 1})
              _ -> pure (RErr (show code), st1)
          Inbox a limit -> do
            (code, v) <- get "/v1/inbox" [("me", agentId a), ("limit", T.pack (show limit))]
            let row m = (,) <$> (str "msg_id" m >>= (`M.lookup` lsIndex st)) <*> int "delivery_count" m
                rows = arr "messages" v
                got = mapMaybe row rows
            pure (if code == 200 && length got == length rows then RInbox got else RErr (show code), st)
          Ack a i -> do
            mid <- maybe newUlid pure (M.lookup i (lsIds st))
            (code, _) <- post "/v1/ack" (object ["me" .= agentId a, "msg_id" .= mid])
            pure (if code == 200 then RAcked else RErr (show code), st)
          Tick d -> do
            (code, _) <- post "/v1/test/clock" (object ["advance_seconds" .= d])
            pure (if code == 200 then RTicked else RErr (show code), st)
          _ -> pure (RUnsupported, st)
        pure (st', (op, r) : acc)
  (_, acc) <- foldM stepLive (LiveSt M.empty M.empty M.empty 0, []) ops
  pure (reverse acc)
  where
    unAgent (Agent a) = a
    procStart p = T.pack ("2026-10-04T00:00:0" ++ show p ++ "Z")
    threadId th st = case M.lookup th (lsThreads st) of
      Just t -> pure (t, st)
      Nothing -> do
        t <- newUlid
        pure (t, st {lsThreads = M.insert th t (lsThreads st)})

leaseState :: T.Text -> Maybe LeaseState
leaseState s = lookup s [("live", Live), ("idle", Idle), ("gone", Gone), ("ended", Ended)]

field :: T.Text -> Value -> Maybe Value
field k (Object o) = KM.lookup (K.fromText k) o
field _ _ = Nothing

str :: T.Text -> Value -> Maybe T.Text
str k v = case field k v of
  Just (String s) -> Just s
  _ -> Nothing

int :: T.Text -> Value -> Maybe Integer
int k v = case field k v of
  Just (Number n) -> toInteger <$> (toBoundedInteger n :: Maybe Int)
  _ -> Nothing

arr :: T.Text -> Value -> [Value]
arr k v = case field k v of
  Just (Array xs) -> V.toList xs
  _ -> []

-- | A fresh ULID: 48-bit millisecond time and 80 random bits, Crockford
-- base32, 26 characters (the broker requires a canonical ULID).
newUlid :: IO T.Text
newUlid = do
  ms <- floor . (* 1000) <$> getPOSIXTime
  r <- randomRIO (0, 2 ^ (80 :: Int) - 1)
  let n = (ms `shiftL` 80) + r :: Integer
      alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
  pure (T.pack [alphabet !! fromInteger ((n `shiftR` (5 * k)) .&. 31) | k <- [25, 24 .. 0]])
