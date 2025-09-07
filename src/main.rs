use pluguzu::binding::PluguzuState;

fn main() {
    if let Some(pat) = std::env::args().nth(1) {
        let mut ps = PluguzuState::new(12);
        if let Some(err) = ps.parse(&pat) {
            println!("Parse failure: {err}");
        } else {
            ps.render(0, 4);
            let frames_per_fraction = 1000.;
            for ev in ps.buffer {
                let offset = (ev.start * frames_per_fraction) as u32;
                match nih_plug::midi::NoteEvent::<()>::from_midi(offset, &ev.data) {
                    Ok(mev) => println!("=> {:3} {:?}", ev.stop, mev),
                    Err(err) => println!("E> {:?} {:?}", ev, err),
                }
            }
        }
    } else {
        nih_plug::prelude::nih_export_standalone::<pluguzu::Pluguzu>();
    }
}
