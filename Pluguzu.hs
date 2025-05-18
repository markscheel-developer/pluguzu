{-# LANGUAGE ImportQualifiedPost #-}
{-# LANGUAGE OverloadedRecordDot #-}

module Pluguzu (pluguzuNew, pluguzuParse, pluguzuError) where

import Data.IORef
import Data.Maybe (fromMaybe)
import Foreign (Ptr)
import Foreign qualified
import Foreign.C (CSize, CString)
import Foreign.C qualified
import Foreign.Marshal.Array (pokeArray)

-- import System.Posix.Internals (puts)

import PluguzuCore qualified

type CPluguzuState = Foreign.StablePtr PluguzuState

foreign export ccall pluguzuNew :: IO CPluguzuState
foreign export ccall pluguzuParse :: CPluguzuState -> Foreign.C.CString -> IO ()
foreign export ccall pluguzuError :: CPluguzuState -> IO Foreign.C.CString
foreign export ccall pluguzuRender :: CPluguzuState -> Word -> Ptr PluguzuCore.MidiEvent -> IO CSize

data PluguzuState = PluguzuState
    { error :: IORef (Maybe CString)
    -- ^ The last parse error.
    , pat :: IORef PluguzuCore.ControlPattern
    -- ^ The current pattern
    }

-- | Create a new state.
pluguzuNew :: IO CPluguzuState
pluguzuNew = do
    state <- PluguzuState <$> newIORef Nothing <*> newIORef mempty
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
    case PluguzuCore.parseTidal str of
        Left err -> do
            cerr <- Foreign.C.newCString err
            writeError state.error (Just cerr)
        Right pat -> do
            writeError state.error Nothing
            writeIORef state.pat pat

-- | Get parse error, return NULL when Nothing.
pluguzuError :: CPluguzuState -> IO Foreign.C.CString
pluguzuError pState = do
    state <- Foreign.deRefStablePtr pState
    mError <- readIORef state.error
    pure $ fromMaybe Foreign.nullPtr mError

pluguzuRender :: CPluguzuState -> Word -> Ptr PluguzuCore.MidiEvent -> IO CSize
pluguzuRender pState startArc pEvs = do
    state <- Foreign.deRefStablePtr pState
    pat <- readIORef state.pat
    let start = fromInteger (toInteger startArc)
    let arc = PluguzuCore.mkArc start (start + 4)
    let evs = PluguzuCore.patternToEvents pat arc
    pokeArray pEvs evs
    pure (fromInteger (toInteger (length evs)))
