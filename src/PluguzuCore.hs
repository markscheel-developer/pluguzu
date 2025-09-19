{-# LANGUAGE DuplicateRecordFields #-}
{-# LANGUAGE OverloadedRecordDot #-}

-- Try on the REPL with
-- (flip patternToEvents (mkArc 0 1)) <$> parseTidal "silence"

module PluguzuCore (
    patternToEvents,
    mkArc,
    MidiEvent (..),
    NoteLoc,
    ParamEvent,
    Event (..),
    eventsToLoc,
    eventsToParam,
    filterDup,

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
import Data.Bits ((.|.))
import Data.Coerce (coerce)
import Data.Int (Int32)
import Data.List (sortBy)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, mapMaybe)
import Data.Word (Word8)
import Debug.Trace (traceShow, traceShowM)
import Foreign.Storable qualified as Storable
import Foreign.Storable.Generic (GStorable)
import GHC.Float (double2Float)
import GHC.Generics (Generic)

import Sound.Tidal.Parse qualified as Parse
import Sound.Tidal.Pattern qualified as Pattern
import Sound.Tidal.Time qualified as Time

-- import Sound.Tidal.Clock qualified as Clock
-- import Sound.Tidal.Link qualified as Link
-- import Sound.Tidal.Stream.Process qualified as Process

rat2float :: Rational -> Double
rat2float = fromRational

midiNote :: Pattern.Note -> Word8
midiNote (Pattern.Note n) = round $ n + 72

mkArc :: Rational -> Rational -> Time.Arc
mkArc start stop = Time.Arc start stop

{- | The event generated from pattern.
Note that this needs to be kept in sync with the 'midiEvent' defined in Pluguzu.hpp
-}
data MidiEvent = MidiEvent
    { barFraction :: Double
    , stopFraction :: Double
    , snd :: Int32
    , data0 :: Word8
    , data1 :: Word8
    , data2 :: Word8
    }
    deriving (Show, Generic)

instance GStorable MidiEvent

data EventLoc = EventLoc
    { col :: Int
    , row :: Int
    , end :: Int
    }
    deriving (Show)

data NoteLoc = NoteLoc
    { eventNR :: Int
    , col :: Int
    , row :: Int
    , len :: Int
    }
    deriving (Show, Generic)

instance GStorable NoteLoc

data EventParam = EventParam
    { paramID :: Word8
    , paramValue :: Float
    }
    deriving (Show)

data Event = Event
    { midi :: MidiEvent
    , sound :: Maybe String
    , locs :: [EventLoc]
    , params :: [EventParam]
    }
    deriving (Show)

eventsToLoc :: [Event] -> [NoteLoc]
eventsToLoc = concatMap go . zip [0 ..]
  where
    go :: (Int, Event) -> [NoteLoc]
    go (pos, ev) = map mkLoc ev.locs
      where
        mkLoc el = NoteLoc pos el.col (el.row - 1) (el.end - el.col)

data ParamEvent = ParamEvent
    { eventNR :: Int
    , paramID :: Word8
    , paramValue :: Float
    }
    deriving (Show, Generic)

instance GStorable ParamEvent

eventsToParam :: [Event] -> [ParamEvent]
eventsToParam = concatMap go . zip [0 ..]
  where
    go (pos, ev) = map mkLoc ev.params
      where
        mkLoc p = ParamEvent pos p.paramID p.paramValue

filterDup :: [Event] -> [Event]
filterDup [] = []
filterDup (x : xs) = x : filterDup (dropDup xs)
  where
    dropDup [] = []
    dropDup rest@(y : ys)
        | y.midi.barFraction > x.midi.barFraction = rest
        | y.midi.data0 == x.midi.data0 && y.midi.data1 == x.midi.data1 && y.sound == x.sound = dropDup ys
        | otherwise = y : dropDup ys

patternToEvents :: Pattern.ControlPattern -> Time.Arc -> [Event]
patternToEvents pat arc = sortBy compareEvent $ mapMaybe eventToMidi $ Pattern.queryArc pat arc

compareEvent :: Event -> Event -> Ordering
compareEvent e1 e2 = compare e1.midi.barFraction e2.midi.barFraction

eventToMidi :: Pattern.Event Pattern.ValueMap -> Maybe Event
eventToMidi ev = do
    (sound, params, mev) <- soundEvent <|> midiEvent
    pure $ Event mev sound (map mkLoc ev.context.contextPosition) params
  where
    mkLoc :: ((Int, Int), (Int, Int)) -> EventLoc
    mkLoc ((col, row), (end, _)) = EventLoc col row end
    chan = (+ 1) $ round $ max 0 $ min 15 $ fromMaybe 0 $ Pattern.getF =<< Map.lookup "midichan" ev.value
    ccToMidi (ccn, ccv) = MidiEvent (rat2float (Pattern.wholeStart ev)) 0 0 (0xb0 .|. chan) ccn ccv
    midiEvent = do
        mev <- (ccToMidi <$> eventToCC ev) <|> (noteToMidi <$> eventToNote ev)
        pure (Nothing, [], mev)
    soundEvent = do
        s <- Pattern.getS =<< Map.lookup "s" ev.value
        let n = Pattern.getN =<< Map.lookup "n" ev.value
        let note = maybe 0 (round . Pattern.unNote) n
        let params = mapMaybe (getEventParam ev.value) staticParams
        pure (Just s, params, noteToMidi (Pattern.wholeOrPart ev, note, 0))
    noteToMidi (arc, pitch, velocity) =
        let start = rat2float (Pattern.start arc)
            stop_ = rat2float (Pattern.stop arc)
         in MidiEvent start stop_ 0 (0x90 .|. chan) pitch velocity

staticParams :: [(Word8, String)]
staticParams =
    zip
        [0 ..]
        [ "begin"
        , "end"
        , "speed"
        ]

getEventParam :: Pattern.ValueMap -> (Word8, String) -> Maybe EventParam
getEventParam m (paramID, name) = do
    value <- getFloat =<< Map.lookup name m
    pure $ EventParam paramID value
  where
    getFloat v = double2Float <$> Pattern.getF v

eventToCC :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToCC ev = do
    ccn <- Pattern.getF =<< Map.lookup "ccn" ev.value
    ccv <- Pattern.getF =<< Map.lookup "ccv" ev.value
    pure (clamp ccn, clamp ccv)
  where
    clamp = max 0 . min 127 . round

eventToNote :: Pattern.Event Pattern.ValueMap -> Maybe (Time.Arc, Word8, Word8)
eventToNote ev = do
    note <- midiNote <$> noteEvent
    pure (Pattern.wholeOrPart ev, note, v)
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
traceEvent ev = traceShow (showValue <$> ev.value)

showValue :: Pattern.Value -> String
showValue v = case v of
    Pattern.VS s -> s
    Pattern.VF d -> show d
    Pattern.VN n -> show n
    Pattern.VI i -> show i
    Pattern.VR r -> show r
    Pattern.VB b -> show b
    Pattern.VPattern p -> "pattern"
    Pattern.VList xs -> show $ map showValue xs
    Pattern.VState s -> "state"

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
