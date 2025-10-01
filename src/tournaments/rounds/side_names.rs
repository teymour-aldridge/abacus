use crate::tournaments::Tournament;

pub fn name_of_side(
    tournament: &Tournament,
    side: i64,
    seq: i64,
    short: bool,
) -> String {
    match (tournament.teams_per_side, side, seq, short) {
        (1, 0, 0, true) => "Gov",
        (1, 0, 0, false) => "Government",
        (1, 1, 0, true) => "Opp",
        (1, 1, 0, false) => "Opposition",
        (2, 0, 0, true) => "OG",
        (2, 0, 0, false) => "Opening Government",
        (2, 1, 0, true) => "OO",
        (2, 1, 0, false) => "Opening Opposition",
        (2, 0, 1, true) => "CG",
        (2, 0, 1, false) => "Closing Government",
        (2, 1, 1, true) => "CO",
        (2, 1, 1, false) => "Closing Opposition",
        _ => {
            return format!(
                "{} {seq}",
                if side == 0 {
                    if short { "Prop" } else { "Proposition" }
                } else if side == 1 {
                    if short { "Opp" } else { "Opposition" }
                } else {
                    unreachable!()
                }
            );
        }
    }
    .into()
}
