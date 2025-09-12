{-# LANGUAGE ImportQualifiedPost #-}
{-# LANGUAGE OverloadedRecordDot #-}

module Pluguzu (pluguzuNew, pluguzuParse, pluguzuError) where

import Control.Exception (SomeException, displayException, handle)
import Control.Monad (unless)
import Data.IORef
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe)
import Foreign (Ptr)
import Foreign qualified
import Foreign.C (CSize, CString)
import Foreign.C qualified
import Foreign.Marshal.Array (pokeArray)
import System.Directory qualified

-- import System.Posix.Internals (puts)

import PluguzuCore qualified

type CPluguzuState = Foreign.StablePtr PluguzuState

foreign export ccall pluguzuNew :: Word -> IO CPluguzuState
foreign export ccall pluguzuParse :: CPluguzuState -> Foreign.C.CString -> IO ()
foreign export ccall pluguzuError :: CPluguzuState -> IO Foreign.C.CString
foreign export ccall pluguzuRender :: CPluguzuState -> Word -> Word -> Ptr PluguzuCore.MidiEvent -> Ptr Int -> Ptr PluguzuCore.NoteLoc -> IO CSize
foreign export ccall pluguzuSoundMap :: CPluguzuState -> CString -> Word -> IO ()
foreign export ccall pluguzuNewConfigDir :: IO CString
foreign export ccall pluguzuFreeConfigDir :: CString -> IO ()

data PluguzuState = PluguzuState
    { maxEvent :: Int
    -- ^ The maximum number of a event per query
    , error :: IORef (Maybe CString)
    -- ^ The last parse error.
    , pat :: IORef PluguzuCore.ControlPattern
    -- ^ The current pattern
    , soundMap :: IORef PluguzuCore.SoundMap
    }

pluguzuNewConfigDir :: IO CString
pluguzuNewConfigDir = do
    path <- System.Directory.getXdgDirectory System.Directory.XdgConfig "pluguzu"
    System.Directory.createDirectoryIfMissing True path
    Foreign.C.newCString path

pluguzuFreeConfigDir :: CString -> IO ()
pluguzuFreeConfigDir = Foreign.free

-- | Create a new state.
pluguzuNew :: Word -> IO CPluguzuState
pluguzuNew maxEvent = do
    state <- PluguzuState (fromIntegral maxEvent) <$> newIORef Nothing <*> newIORef mempty <*> newIORef mempty
    Foreign.newStablePtr state

writeError :: IORef (Maybe CString) -> Maybe CString -> IO ()
writeError ref new = do
    mPrev <- readIORef ref
    mapM_ Foreign.free mPrev
    writeIORef ref new

-- | Parse and load a tidal pattern.
pluguzuParse :: CPluguzuState -> Foreign.C.CString -> IO ()
pluguzuParse pState cStr = do
    state <- Foreign.deRefStablePtr pState
    str <- Foreign.C.peekCString cStr
    -- The following pattern raises an error: `n "<0.>"`.
    -- TODO: report the bug upstream: Can't happen, feet are pre-processed.
    let catchException = \(e :: SomeException) -> do
            cerr <- Foreign.C.newCString ("Error: " <> displayException e)
            writeError state.error (Just cerr)
    -- putStrLn (PluguzuCore.deltaMini str)
    handle catchException $ case PluguzuCore.parseTidal (PluguzuCore.deltaMini str) of
        Left err -> do
            cerr <- Foreign.C.newCString err
            writeError state.error (Just cerr)
        Right pat -> do
            writeIORef state.pat $! pat
            writeError state.error Nothing

-- | Get parse error, return NULL when Nothing.
pluguzuError :: CPluguzuState -> IO Foreign.C.CString
pluguzuError pState = do
    state <- Foreign.deRefStablePtr pState
    mError <- readIORef state.error
    pure $ fromMaybe Foreign.nullPtr mError

-- | Add a new sound mapping to midi note
pluguzuSoundMap :: CPluguzuState -> CString -> Word -> IO ()
pluguzuSoundMap pState sound note = do
    state <- Foreign.deRefStablePtr pState
    str <- Foreign.C.peekCString sound
    modifyIORef' state.soundMap (Map.insert str (fromIntegral note))

pluguzuRender :: CPluguzuState -> Word -> Word -> Ptr PluguzuCore.MidiEvent -> Ptr Int -> Ptr PluguzuCore.NoteLoc -> IO CSize
pluguzuRender pState startArc endArc pEvs pLocCount pLocs = handle catchException $ do
    state <- Foreign.deRefStablePtr pState
    pat <- readIORef state.pat
    sm <- readIORef state.soundMap
    let start = fromInteger (toInteger startArc)
    let end = fromInteger (toInteger endArc)
    let arc = PluguzuCore.mkArc start end
    let evs = PluguzuCore.patternToEvents sm pat arc
    pokeArray pEvs $ map (.midi) (take state.maxEvent evs)
    let locs = PluguzuCore.eventsToLoc evs
    unless (pLocs == Foreign.nullPtr) $ pokeArray pLocs $ take (state.maxEvent * 2) $ locs
    Foreign.poke pLocCount $ length locs
    pure (fromInteger (toInteger (length evs)))
  where
    catchException :: SomeException -> IO CSize
    catchException = \(e :: SomeException) -> do
        putStrLn $ "Pluguzu render triggered exception: " <> displayException e
        pure 0
