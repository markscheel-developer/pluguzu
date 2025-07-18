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
)
where

import Control.Applicative ((<|>))
import Control.Monad (forM_)
import Data.Coerce (coerce)
import Data.Int (Int8)
import Data.List (sortOn)
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe, mapMaybe)
import Foreign.Storable qualified as Storable
import Foreign.Storable.Generic (GStorable)
import GHC.Generics (Generic)

import Sound.Tidal.Parse qualified as Parse
import Sound.Tidal.Pattern qualified as Pattern
import Sound.Tidal.Time qualified as Time

-- import Sound.Tidal.Clock qualified as Clock
-- import Sound.Tidal.Link qualified as Link
-- import Sound.Tidal.Stream.Process qualified as Process

type SoundMap = Map.Map String Int8

rat2float :: Rational -> Double
rat2float = fromRational

midiNote :: Pattern.Note -> Int8
midiNote (Pattern.Note n) = round $ n + 72

mkArc :: Rational -> Rational -> Time.Arc
mkArc start stop = Time.Arc start stop

{- | The event generated from pattern.
Note that this needs to be kept in sync with the 'midiEvent' defined in Pluguzu.hpp
-}
data MidiEvent = MidiEvent
    { barFraction :: Double
    , pitch :: Int8
    , velocity :: Int8
    }
    deriving (Show, Generic)

instance GStorable MidiEvent

patternToEvents :: SoundMap -> Pattern.ControlPattern -> Time.Arc -> [MidiEvent]
patternToEvents sm pat arc = sortOn barFraction $ concatMap (eventToMidi sm) $ Pattern.queryArc pat arc

eventToMidi :: SoundMap -> Pattern.Event Pattern.ValueMap -> [MidiEvent]
eventToMidi sm ev = case eventToNote sm ev of
    Nothing -> []
    Just (arc, pitch, velocity) ->
        let start = rat2float (Pattern.start arc)
            stop_ = rat2float (Pattern.stop arc)
         in [ MidiEvent start pitch velocity
            , MidiEvent stop_ pitch 0
            ]

eventToNote :: SoundMap -> Pattern.Event Pattern.ValueMap -> Maybe (Time.Arc, Int8, Int8)
eventToNote sm ev = do
    note <- midiNote <$> noteEvent <|> soundEvent
    pure (Pattern.wholeOrPart ev, note, v)
  where
    noteEvent = Pattern.getN =<< Map.lookup "n" ev.value
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
