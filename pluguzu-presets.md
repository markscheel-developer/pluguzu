# The pluguzu presets library

This document contains a library of tidal code for getting started with pluguzu.
After changing the library, run `make render-presets` to update the `src/presets.rs` module. Then rebuild the plugin with `make`.

## Demo

### Calm

```haskell
stack [
  n $ scale "minor" $ "{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>"
, ccv (segment 32 $ range 0 128 $ sine) # ccn 1
]
```

### Mondough

```mondo
$ note <d'min9 <g'min7 a'sevenFlat9>> # clip .8 # release .3
# sub (note 24)
# vib 4:.2
# lpf (sine # range 500 2000 # slow 8)
# lpe 1 # lpd .5 # lpq .1
# s saw # gain .3

$ note <d1 <g1 a1>> # clip .25
# s pulse # pw .4
# lpf 200 # lpe 3 # lpd .1
# off .125 (add (note 12))
# off .25 (add (note 24))
# jux rev
# distort 2:.5
# gain .3
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
