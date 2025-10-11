// Pluguzu presets

use crate::haskell::UzuKind;
use crate::presets::UzuProgram;

pub fn presets() -> Vec<UzuProgram> {
    vec![
      UzuProgram {name: "Calm".into(), lang: UzuKind::Tidal, code: "stack [\n  n $ scale \"minor\" $ \"{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>\"\n, ccv (segment 32 $ range 0 128 $ sine) # ccn 1\n]\n".into(), tags: vec!["Demo".into()]},
      UzuProgram {name: "4 on the floor".into(), lang: UzuKind::Tidal, code: "s(\"bd*4\")\n".into(), tags: vec!["Beat".into()]},
      UzuProgram {name: "Funky line".into(), lang: UzuKind::Tidal, code: "n(\"0 0 - [- 0]\")\n".into(), tags: vec!["Bass".into()]},
      UzuProgram {name: "Secret Of Uzu".into(), lang: UzuKind::Tidal, code: "-- todo\n".into(), tags: vec!["Melody".into()]},
  ]
}
