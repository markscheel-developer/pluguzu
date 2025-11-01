// Pluguzu presets

use crate::haskell::UzuKind;
use crate::presets::UzuProgram;

pub fn presets() -> Vec<UzuProgram> {
    vec![
      UzuProgram {name: "Calm".into(), lang: UzuKind::Tidal, code: "stack [\n  n $ scale \"minor\" $ \"{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>\"\n, ccv (segment 32 $ range 0 128 $ sine) # ccn 1\n]\n".into(), tags: vec!["Demo".into()]},
      UzuProgram {name: "Mondough".into(), lang: UzuKind::Mondo, code: "$ note <\"d'min9\" <\"g'min7\" \"a'sevenFlat9\">>\n# clip .8 # release .3\n# sub (note 24)\n# vib 4:.2\n# lpf (sine # range 500 2000 # slow 8)\n# lpe 1 # lpd .5 # lpq .1\n# s saw # gain .3\n\n$ note <d1 <g1 a1>> # clip .25\n# s pulse # pw .4\n# lpf 200 # lpe 3 # lpd .1\n# off .125 (add (note 12))\n# off .25 (add (note 24))\n# jux rev\n# distort 2:.5\n# gain .3\n".into(), tags: vec!["Demo".into()]},
      UzuProgram {name: "4 on the floor".into(), lang: UzuKind::Tidal, code: "s(\"bd*4\")\n".into(), tags: vec!["Beat".into()]},
      UzuProgram {name: "Funky line".into(), lang: UzuKind::Tidal, code: "n(\"0 0 - [- 0]\")\n".into(), tags: vec!["Bass".into()]},
      UzuProgram {name: "Secret Of Uzu".into(), lang: UzuKind::Tidal, code: "-- todo\n".into(), tags: vec!["Melody".into()]},
  ]
}
