{-# LANGUAGE ImportQualifiedPost #-}
{-# LANGUAGE OverloadedRecordDot #-}

module Pluguzu (pluguzuNew, pluguzuParse, pluguzuError) where

import Control.Exception (SomeException, displayException, handle)
import Control.Monad (unless)
import Data.IORef
import Data.Int (Int32)
import Data.IntMap.Strict qualified as IM
import Data.Map.Strict qualified as Map
import Data.Maybe (fromMaybe)
import Data.Traversable (mapAccumM)
import Foreign (Ptr)
import Foreign qualified
import Foreign.C (CSize, CString)
import Foreign.C qualified
import Foreign.Marshal.Array (pokeArray)

-- import System.Posix.Internals (puts)

import PluguzuCore qualified

type CPluguzuState = Foreign.StablePtr PluguzuState

foreign export ccall pluguzuNew :: Word -> Word -> Word -> IO CPluguzuState
foreign export ccall pluguzuParse :: CPluguzuState -> Foreign.C.CString -> IO ()
foreign export ccall pluguzuError :: CPluguzuState -> IO Foreign.C.CString
foreign export ccall pluguzuSoundName :: CPluguzuState -> Int -> IO Foreign.C.CString
foreign export ccall pluguzuRender :: CPluguzuState -> Word -> Word -> Ptr PluguzuCore.MidiEvent -> Ptr Int -> Ptr PluguzuCore.NoteLoc -> Ptr Int -> Ptr PluguzuCore.ParamEvent -> IO CSize

data PluguzuState = PluguzuState
    { maxEvent :: Int
    -- ^ The maximum number of a event per query
    , maxLoc :: Int
    -- ^ The maximum number of locations
    , maxParam :: Int
    -- ^ The maximum number of params
    , error :: IORef (Maybe CString)
    -- ^ The last parse error.
    , pat :: IORef PluguzuCore.ControlPattern
    -- ^ The current pattern
    , sampleID :: IORef (Map.Map String Int32)
    , sampleNames :: IORef (IM.IntMap Foreign.C.CString)
    }

-- | Create a new state.
pluguzuNew :: Word -> Word -> Word -> IO CPluguzuState
pluguzuNew maxEvent maxLoc maxParam = do
    state <- PluguzuState (fromIntegral maxEvent) (fromIntegral maxLoc) (fromIntegral maxParam) <$> newIORef Nothing <*> newIORef mempty <*> newIORef mempty <*> newIORef mempty
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

pluguzuSoundName :: CPluguzuState -> Int -> IO Foreign.C.CString
pluguzuSoundName pState idx = do
    state <- Foreign.deRefStablePtr pState
    sn <- readIORef state.sampleNames
    pure $ fromMaybe Foreign.nullPtr $ IM.lookup idx sn

pluguzuRender :: CPluguzuState -> Word -> Word -> Ptr PluguzuCore.MidiEvent -> Ptr Int -> Ptr PluguzuCore.NoteLoc -> Ptr Int -> Ptr PluguzuCore.ParamEvent -> IO CSize
pluguzuRender pState startArc endArc pEvs pLocCount pLocs pParamCount pParams = handle catchException $ do
    state <- Foreign.deRefStablePtr pState
    pat <- readIORef state.pat
    let start = fromInteger (toInteger startArc)
    let end = fromInteger (toInteger endArc)
    let arc = PluguzuCore.mkArc start end
    let evsBase = PluguzuCore.filterDup $ PluguzuCore.patternToEvents pat arc
    evs <-
        if pParams == Foreign.nullPtr
            then pure evsBase
            else updateNames state evsBase
    pokeArray pEvs $ map (.midi) (take state.maxEvent evs)
    unless (pLocs == Foreign.nullPtr) $ do
        let locs = PluguzuCore.eventsToLoc evs
        pokeArray pLocs (take state.maxLoc locs)
        Foreign.poke pLocCount $ length locs
    unless (pParams == Foreign.nullPtr) $ do
        let params = PluguzuCore.eventsToParam evs
        pokeArray pParams (take state.maxParam params)
        Foreign.poke pParamCount $ length params
    pure (fromInteger (toInteger (length evs)))
  where
    catchException :: SomeException -> IO CSize
    catchException = \(e :: SomeException) -> do
        putStrLn $ "Pluguzu render triggered exception: " <> displayException e
        pure 0
    updateNames state evs = do
        sn <- readIORef state.sampleNames
        si <- readIORef state.sampleID
        ((si', sn'), evs') <- mapAccumM assignNames (si, sn) evs
        writeIORef state.sampleNames sn'
        writeIORef state.sampleID si'
        pure evs'
    assignNames :: (Map.Map String Int32, IM.IntMap Foreign.C.CString) -> PluguzuCore.Event -> IO ((Map.Map String Int32, IM.IntMap Foreign.C.CString), PluguzuCore.Event)
    assignNames s@(si, sn) e = case e.sound of
        Nothing -> pure (s, e)
        Just name -> case Map.lookup name si of
            Nothing -> do
                let newID = Map.size si + 1
                let newID32 = fromIntegral newID
                cname <- Foreign.C.newCString name
                pure
                    (
                        ( Map.insert name newID32 si
                        , IM.insert newID cname sn
                        )
                    , assignID e newID32
                    )
            Just idx -> pure (s, assignID e idx)
    assignID e idx = e{PluguzuCore.midi = e.midi{PluguzuCore.snd = idx}}
