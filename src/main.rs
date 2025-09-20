use pluguzu::binding::PluguzuState;

fn main() {
    if let Some(pat) = std::env::args().nth(1) {
        let mut ps = PluguzuState::default();
        if let Some(err) = ps.parse(&pat) {
            println!("Parse failure: {err}");
        } else {
            ps.render(0, 4);
            for (nr, ev) in ps.events.iter().enumerate() {
                println!("{nr}=> {ev:?}");
            }
        }
    } else {
        nih_plug::prelude::nih_export_standalone::<pluguzu::Pluguzu>();
    }
}
