# The pluguzu presets library

This document contains a library of tidal code for getting started with pluguzu.
After changing the library, run `make render-presets` to update the `pluguzu-presets.h` header and rebuild the plugin.

## Demo

### Calm

```haskell
stack [
  n $ scale "minor" $ "{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>"
, ccv (segment 32 $ range 0 128 $ sine) # ccn 1
]
```

## Beat

### 4 on the floor

```haskell
s("bd*4")
```

## Bass

### Funky line

```haskell
n("0 0 - [- 0]")
```

## Melody

### Secret Of Uzu

```haskell
-- todo
```
