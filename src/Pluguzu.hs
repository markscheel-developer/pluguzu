module Pluguzu (someFunc) where

import System.Posix.Internals (puts)

foreign export ccall someFunc :: IO ()

someFunc :: IO ()
someFunc = puts "pluguzu go!"
