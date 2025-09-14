use pluguzu::binding::PluguzuState;

fn main() {
    if let Some(pat) = std::env::args().nth(1) {
        let mut ps = PluguzuState::new(512, 512);
        if let Some(err) = ps.parse(&pat) {
            println!("Parse failure: {err}");
        } else {
            ps.render(0, 4);
            println!("{:?}", ps.locs);
            let frames_per_fraction = 1000.;
            let mut loc_nr = 0;
            for (nr, ev) in ps.buffer.into_iter().enumerate() {
                let offset = (ev.start * frames_per_fraction) as u32;
                match nih_plug::midi::NoteEvent::<()>::from_midi(offset, &ev.data) {
                    Ok(mev) => println!("=> {:3} {:?}", ev.stop, mev),
                    Err(err) => println!("E> {:?} {:?}", ev, err),
                }
                while loc_nr < ps.locs.len() {
                    let loc = ps.locs[loc_nr];
                    if loc.pos == nr as i64 {
                        println!("POS: {:?}", loc);
                    } else {
                        break;
                    }
                    loc_nr += 1;
                }
            }
        }
    } else {
        nih_plug::prelude::nih_export_standalone::<pluguzu::Pluguzu>();
    }
}
