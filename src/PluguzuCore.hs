{-# LANGUAGE OverloadedRecordDot #-}

module PluguzuCore (
    patternToEvents,
    mkArc,
    MidiEvent,
    SoundMap,

    -- * re-export
    Pattern.ControlPattern,
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
import Data.List (sortBy)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, mapMaybe)
import Data.Word (Word8)
import Debug.Trace (traceShow)
import Foreign.Storable qualified as Storable
import Foreign.Storable.Generic (GStorable)
import GHC.Generics (Generic)

import Sound.Tidal.Parse qualified as Parse
import Sound.Tidal.Pattern qualified as Pattern
import Sound.Tidal.Time qualified as Time

-- import Sound.Tidal.Clock qualified as Clock
-- import Sound.Tidal.Link qualified as Link
-- import Sound.Tidal.Stream.Process qualified as Process

type SoundMap = Map.Map String Word8

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
    , data0 :: Word8
    , data1 :: Word8
    , data2 :: Word8
    }
    deriving (Show, Generic)

instance GStorable MidiEvent

patternToEvents :: SoundMap -> Pattern.ControlPattern -> Time.Arc -> [MidiEvent]
patternToEvents sm pat arc = sortBy compareEvent $ mapMaybe (eventToMidi sm) $ Pattern.queryArc pat arc

compareEvent :: MidiEvent -> MidiEvent -> Ordering
compareEvent e1 e2 = compare e1.barFraction e2.barFraction

eventToMidi :: SoundMap -> Pattern.Event Pattern.ValueMap -> Maybe MidiEvent
eventToMidi sm ev = (ccToMidi <$> eventToCC ev) <|> (noteToMidi <$> eventToNote sm ev)
  where
    chan = (+ 1) $ round $ max 0 $ min 15 $ fromMaybe 0 $ Pattern.getF =<< Map.lookup "midichan" ev.value
    ccToMidi (ccn, ccv) = MidiEvent (rat2float (Pattern.wholeStart ev)) 0 (0xb0 .|. chan) ccn ccv
    noteToMidi (arc, pitch, velocity) =
        let start = rat2float (Pattern.start arc)
            stop_ = rat2float (Pattern.stop arc)
         in MidiEvent start stop_ (0x90 .|. chan) pitch velocity

eventToCC :: Pattern.Event Pattern.ValueMap -> Maybe (Word8, Word8)
eventToCC ev = do
    ccn <- Pattern.getF =<< Map.lookup "ccn" ev.value
    ccv <- Pattern.getF =<< Map.lookup "ccv" ev.value
    pure (clamp ccn, clamp ccv)
  where
    clamp = max 0 . min 127 . round

eventToNote :: SoundMap -> Pattern.Event Pattern.ValueMap -> Maybe (Time.Arc, Word8, Word8)
eventToNote sm ev = do
    note <- midiNote <$> noteEvent <|> soundEvent
    pure (Pattern.wholeOrPart ev, note, v)
  where
    noteEvent = Pattern.getN =<< (Map.lookup "n" ev.value <|> Map.lookup "note" ev.value <|> Map.lookup "midinote" ev.value)
    soundEvent = do
        s <- Pattern.getS =<< Map.lookup "s" ev.value
        pure $ fromMaybe 72 $ Map.lookup s sm

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

{-
-- Previous attempt using tidal-link
standalone_ :: IO ()
standalone_ = do
    link <- Link.create 120
    Link.enable link
    state <- Link.createAndCaptureAppSessionState link
    -- bpm <- Clock.getTempo state

    (mem, _st) <- Clock.initClock cconf (\_ _ _ _ _ -> pure ())
    let cref = Clock.clockRef mem

    now <- Link.clock link
    -- Link.forceBeatAtTime state 0 now 1
    eval now cconf cref (0, 2) state pat
  where
    pat = either error id $ Parse.parseTidal "s \"<[bd sd] [sd sd]>\""
    cconf = Clock.defaultConfig -- {Clock.cFrameTimespan = 0}

toMidi :: Process.ProcessedEvent -> Maybe (Word, Process.ProcessedEvent)
toMidi pe = case isSound of
    Just pitch -> Just (pitch, pe)
  where
    isSound = do
        s <- Pattern.getS =<< Map.lookup "s" pe.peEvent.value
        -- TODO: figure out how to map a sound to pitch
        case s of
            "bd" -> Just 42
            "sd" -> Just 43
            _ -> Nothing

eval :: Link.Micros -> Clock.ClockConfig -> Clock.ClockRef -> (Clock.Time, Clock.Time) -> Link.SessionState -> Pattern.ControlPattern -> IO ()
eval now cconf cref (st, end) state pat = do
    pes <- Process.processCps cconf cref (state, state) es
    forM_ (mapMaybe toMidi pes) $ \(pitch, pe) -> do
        print (pitch, pe.peEvent.value, pe.peCps, "cycle", Clock.cyclesToBeat cconf (realToFrac pe.peCycle), round $ pe.peCycle * 10)
  where
    es =
        sortOn (Pattern.start . Pattern.part) $
            Pattern.query
                pat
                ( Pattern.State
                    { Pattern.arc = Pattern.Arc st end
                    , Pattern.controls = mempty
                    }
                )
-}

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
