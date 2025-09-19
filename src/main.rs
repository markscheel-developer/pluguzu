use pluguzu::binding::PluguzuState;

fn main() {
    if let Some(pat) = std::env::args().nth(1) {
        let mut ps = PluguzuState::default();
        if let Some(err) = ps.parse(&pat) {
            println!("Parse failure: {err}");
        } else {
            ps.render(0, 4);
            let frames_per_fraction = 1000.;
            for (nr, ev) in ps.events.iter().enumerate() {
                let offset = (ev.midi.start * frames_per_fraction) as u32;
                match nih_plug::midi::NoteEvent::<()>::from_midi(offset, &ev.midi.data()) {
                    Ok(mev) => println!("{nr}=> {ev:?} {mev:?}"),
                    Err(err) => println!("{nr}=> {ev:?} {err:?}"),
                }
            }
        }
    } else {
        nih_plug::prelude::nih_export_standalone::<pluguzu::Pluguzu>();
    }
}
