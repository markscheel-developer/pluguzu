{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE OverloadedRecordDot #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE StrictData #-}

-- Try on the REPL with
-- (flip patternToEvents (mkArc 0 1)) <$> parseTidal "silence"

module PluguzuCore (
    patternToEvents,
    mkArc,
    Event (..),
    filterDup,
    encodeEvents,

    -- * re-export
    Pattern.ControlPattern,
    Pattern.deltaMini,
    Time.Arc (..),
    Parse.parseTidal,

    -- * presets
    Preset (..),
    readPresets,
    renderPresetData,

    -- * debug
    traceEvent,
)
where

import Control.Applicative ((<|>))
import Control.Monad (forM_, void)
import Data.Aeson (ToJSON (..))
import Data.Aeson qualified as Aeson
import Data.Bits ((.|.))
import Data.ByteString qualified as BS
import Data.ByteString.Lazy qualified as LBS
import Data.Coerce (coerce)
import Data.Int (Int32)
import Data.List (sortBy)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, mapMaybe)
import Data.Word (Word8)
import Debug.Trace (traceShow, traceShowM)
import GHC.Float (double2Float)
import GHC.Generics (Generic)
import Sound.Tidal.Parse qualified as Parse
import Sound.Tidal.Pattern qualified as Pattern
import Sound.Tidal.Show ()
import Sound.Tidal.Time qualified as Time

-- import Sound.Tidal.Clock qualified as Clock
-- import Sound.Tidal.Link qualified as Link
-- import Sound.Tidal.Stream.Process qualified as Process

rat2float :: Rational -> Double
rat2float = realToFrac

midiNote :: Pattern.Note -> Word8
midiNote (Pattern.Note n) = round $ n + 72

mkArc :: Rational -> Rational -> Time.Arc
mkArc start stop = Time.Arc start stop

{- | The event generated from pattern.
Note that this needs to be kept in sync with the 'midiEvent' defined in Pluguzu.hpp
-}
data MidiEvent = MidiEvent
    { data0 :: Word8
    , data1 :: Word8
    , data2 :: Word8
    }
    deriving (Show)

data EventLoc = EventLoc
    { col :: Int
    , row :: Int
    , end :: Int
    }
    deriving (Show)

instance ToJSON EventLoc where
    toJSON ev = toJSON [ev.col, ev.row, ev.end]

data EventKind = EventMidi MidiEvent | EventSound String Int
    deriving (Show)

data EventSpan = EventSpan
    { start :: Double
    , stop :: Double
    }
    deriving (Show)

data Event = Event
    { span :: EventSpan
    , kind :: EventKind
    , locs :: [EventLoc]
    , value :: Maybe Pattern.ValueMap
    }

instance ToJSON Event where
    toJSON ev = toJSON [span, tag, loc, vals]
      where
        span = toJSON [toJSON ev.span.start, toJSON ev.span.stop]
        tag = case ev.kind of
            EventMidi midi -> toJSON [toJSON midi.data0, toJSON midi.data1, toJSON midi.data2]
            EventSound sound n -> toJSON [toJSON sound, toJSON n]
        loc = toJSON $ map toJSON ev.locs
        vals = case ev.value of
            Nothing -> Aeson.Null
            Just m -> toJSON $ (fmap encodeValue) $ m

encodeEvents :: [Event] -> BS.ByteString
encodeEvents = LBS.toStrict . Aeson.encode

filterDup :: [Event] -> [Event]
filterDup [] = []
filterDup (x : xs) = x : filterDup (dropDup xs)
  where
    dropDup [] = []
    dropDup rest@(y : ys)
        | y.span.start > x.span.start = rest
        | EventMidi ym <- y.kind, EventMidi xm <- x.kind, ym.data0 == xm.data0, xm.data1 == xm.data1 = dropDup ys
        | otherwise = y : dropDup ys

patternToEvents :: Pattern.ControlPattern -> Time.Arc -> [Event]
patternToEvents pat arc = sortBy compareEvent $ mapMaybe eventToMidi $ Pattern.queryArc pat arc

compareEvent :: Event -> Event -> Ordering
compareEvent e1 e2 = compare e1.span.start e2.span.start

eventToMidi :: Pattern.Event Pattern.ValueMap -> Maybe Event
eventToMidi ev = do
    (kind, value) <- soundEvent <|> midiEvent
    pure $ Event span kind (map mkLoc ev.context.contextPosition) value
  where
    arc = Pattern.wholeOrPart ev
    start = rat2float (Pattern.start arc)
    stop_ = rat2float (Pattern.stop arc)
    span = EventSpan start stop_
    mkLoc :: ((Int, Int), (Int, Int)) -> EventLoc
    mkLoc ((col, row), (end, _)) = EventLoc col (row - 1) (end - col)
    chan = (+ 1) $ round $ max 0 $ min 15 $ fromMaybe 0 $ Pattern.getF =<< Map.lookup "midichan" ev.value
    noteToMidi (pitch, velocity) = MidiEvent (0x90 .|. chan) pitch velocity
    ccToMidi (ccn, ccv) = MidiEvent (0xb0 .|. chan) ccn ccv
    midiEvent = do
        mev <- (ccToMidi <$> eventToCC ev) <|> (noteToMidi <$> eventToNote ev)
        pure (EventMidi mev, Nothing)
    soundEvent = do
        s <- Pattern.getS =<< Map.lookup "s" ev.value
        let n = maybe 0 round (Pattern.getN =<< Map.lookup "n" ev.value)
        let params = Map.filterWithKey (\k _ -> k /= "s" && k /= "n") ev.value
        pure (EventSound s n, Just params)

eventToCC :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToCC ev = do
    ccn <- Pattern.getF =<< Map.lookup "ccn" ev.value
    ccv <- Pattern.getF =<< Map.lookup "ccv" ev.value
    pure (clamp ccn, clamp ccv)
  where
    clamp = max 0 . min 127 . round

eventToNote :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToNote ev = do
    note <- midiNote <$> noteEvent
    pure (note, v)
  where
    noteEvent = Pattern.getN =<< (Map.lookup "n" ev.value <|> Map.lookup "note" ev.value <|> Map.lookup "midinote" ev.value)

    v = fromIntegral $ max 0 $ min 127 $ round $ fromMaybe 100 mVelocity
    mVelocity :: Maybe Double
    mVelocity = getF "velocity" <|> (ampToVel <$> getF "amp") <|> (gainToVel <$> getF "gain")
    getF name = Map.lookup name ev.value >>= Pattern.getF

    -- Based on https://tidalcycles.org/docs/configuration/MIDIOSC/midi/#velocity
    ampToVel amp = 127 * amp
    gainToVel gain = 127 * (gain - 0.3) ** 2.5

firstBar :: Time.Arc
firstBar = Time.Arc 0 4

testPattern :: Pattern.ControlPattern
testPattern = either error id $ Parse.parseTidal "n \"<[c3 f3] [c4 f4]>\""

traceEvent :: Pattern.Event Pattern.ValueMap -> a -> a
traceEvent ev = traceShow (show . encodeValue <$> ev.value)

encodeValue :: Pattern.Value -> Aeson.Value
encodeValue v = case v of
    Pattern.VS str -> toJSON str
    Pattern.VI i -> toJSON i
    Pattern.VF f -> toJSON f
    Pattern.VN num -> toJSON $ show num
    Pattern.VR r -> toJSON $ show r
    Pattern.VB b -> toJSON b
    Pattern.VX xs -> toJSON xs
    Pattern.VPattern pat -> toJSON $ show pat
    Pattern.VState f -> toJSON $ show $ f Map.empty
    Pattern.VList vs -> toJSON $ map encodeValue vs

data Preset = Preset
    { category :: String
    , name :: String
    , code :: String
    }
    deriving (Show)

readPresets :: FilePath -> IO [Preset]
readPresets fp = parsePresets "" "" . lines <$> readFile fp
  where
    parsePresets :: String -> String -> [String] -> [Preset]
    parsePresets _ _ [] = []
    parsePresets _ _ (('#' : '#' : ' ' : cat) : xs) = parsePresets cat "" xs
    parsePresets cat _ (('#' : '#' : '#' : ' ' : name) : xs) = parsePresets cat name xs
    parsePresets cat name ("```haskell" : xs) =
        let (code, rest) = break (== "```") xs
         in Preset cat name (unlines code) : parsePresets cat "" rest
    parsePresets cat name (_ : xs) = parsePresets cat name xs

renderPresetData :: IO ()
renderPresetData = do
    presets <- readPresets "pluguzu-presets.md"
    writeFile "src/presets.rs" $ unlines $ renderHeader presets
  where
    renderHeader presets =
        [ "// Pluguzu presets"
        , ""
        , "pub struct UzuProgram {"
        , "  pub name: &'static str,"
        , "  pub code: &'static str,"
        , "  pub tags: Vec<&'static str>,"
        , "}"
        , ""
        , "pub fn presets() -> Vec<UzuProgram> {"
        , "  vec![" ++ unlines (map mkProgram presets) ++ "]"
        , "}"
        ]
    mkProgram preset = "  UzuProgram {name: " ++ show preset.name ++ ", code: " ++ show preset.code ++ ", tags: vec![" ++ show preset.category ++ "]},"
