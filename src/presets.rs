// Pluguzu presets

pub struct UzuProgram {
    pub name: &'static str,
    pub code: &'static str,
    pub tags: Vec<&'static str>,
}

pub fn presets() -> Vec<UzuProgram> {
    vec![
        UzuProgram {
            name: "Calm",
            code: "stack [\n  n $ scale \"minor\" $ \"{[-12 .. -5]/4, [c4 f3 g3]}%<1 [1 2?]>\"\n, ccv (segment 32 $ range 0 128 $ sine) # ccn 1\n]\n",
            tags: vec!["Demo"],
        },
        UzuProgram {
            name: "4 on the floor",
            code: "s(\"bd*4\")\n",
            tags: vec!["Beat"],
        },
        UzuProgram {
            name: "Funky line",
            code: "n(\"0 0 - [- 0]\")\n",
            tags: vec!["Bass"],
        },
        UzuProgram {
            name: "Secret Of Uzu",
            code: "-- todo\n",
            tags: vec!["Melody"],
        },
    ]
}
