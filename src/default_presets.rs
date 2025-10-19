// Pluguzu presets

use crate::haskell::UzuKind;
use crate::presets::UzuProgram;

pub fn presets() -> Vec<UzuProgram> {
    vec![
      UzuProgram {name: "Calm".into(), lang: UzuKind::Tidal, code: "stack [\n  n $ scale \"minor\" $ \"{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>\"\n, ccv (segment 32 $ range 0 128 $ sine) # ccn 1\n]\n".into(), tags: vec!["Demo".into()]},
      UzuProgram {name: "Mondough".into(), lang: UzuKind::Mondo, code: "$ pN note <d'min9 <g'min7 a'sevenFlat9>> # clip .8 # release .3\n# sub (pN note 24)\n# pF vib 4 # pF vibmod .2\n# lpf (sine # range 500 2000 # slow 8)\n# pF lpe 1 # pF lpd .5 # pF lpq .1\n# s saw # gain .3\n\n$ pN note <d1 <g1 a1>> # clip .25\n# s pulse # pF pw .4\n# lpf 200 # pF lpe 3 # pF lpd .1\n# off .125 (add (pN note 12))\n# off .25 (add (pN note 24))\n# jux rev\n# pF distort 2 # pF distortvol .5\n# gain .3\n".into(), tags: vec!["Demo".into()]},
      UzuProgram {name: "4 on the floor".into(), lang: UzuKind::Tidal, code: "s(\"bd*4\")\n".into(), tags: vec!["Beat".into()]},
      UzuProgram {name: "Funky line".into(), lang: UzuKind::Tidal, code: "n(\"0 0 - [- 0]\")\n".into(), tags: vec!["Bass".into()]},
      UzuProgram {name: "Secret Of Uzu".into(), lang: UzuKind::Tidal, code: "-- todo\n".into(), tags: vec!["Melody".into()]},
  ]
}
