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
$ pN note <d'min9 <g'min7 a'sevenFlat9>> # clip .8 # release .3
# sub (pN note 24)
# pF vib 4 # pF vibmod .2
# lpf (sine # range 500 2000 # slow 8)
# pF lpe 1 # pF lpd .5 # pF lpq .1
# s saw # gain .3

$ pN note <d1 <g1 a1>> # clip .25
# s pulse # pF pw .4
# lpf 200 # pF lpe 3 # pF lpd .1
# off .125 (add (pN note 12))
# off .25 (add (pN note 24))
# jux rev
# pF distort 2 # pF distortvol .5
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
