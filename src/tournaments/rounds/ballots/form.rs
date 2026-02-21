use diesel::{connection::LoadConnection, sqlite::Sqlite};
use hypertext::prelude::*;

use crate::tournaments::{
    RoundKind, Tournament,
    rounds::{ballots::manage::edit::BallotForm, draws::DebateRepr},
};

/// Used to generate the form fields (not the actual <form> tag) for the ballot
/// submission and editing forms.
pub fn fields_of_single_ballot_form(
    tournament: &Tournament,
    debate: &DebateRepr,
    existing: Option<&BallotForm>,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> impl Renderable {
    let requires_speaker_order =
        tournament.current_round_requires_speaker_order(conn);
    let requires_speaks = tournament.current_round_requires_speaks(conn);
    let is_elim =
        matches!(tournament.current_round_type(conn), RoundKind::Elim);

    assert!(!debate.motions.is_empty());

    maud! {
        @if debate.motions.len() > 1 {
            select name="motion" {
                // todo: load these in alphabetical order (HashMap -> IndexMap)
                @for motion in &debate.motions {
                    @let selected = match existing {
                        Some(existing) if &existing.motion_id == motion.0 => {
                            true
                        },
                        _ => false
                    };
                    option value=(motion.0) selected=(selected) {
                        (motion.1.motion)
                    }
                }
            }
        } @else {
            // todo: could remove this
            input hidden type="text" value=(debate.motions.iter().next().unwrap().0);
        }


        @for side in 0..2 {
            @for seq in 0..tournament.teams_per_side {
                @let no = side * 2 + seq;

                @let team = debate.team_of_side_and_seq(side, seq);
                @let existing_team = existing.map(|existing| {
                    existing.teams.get((side * 2 + seq) as usize).map(|team| {
                        team
                    })
                }).flatten();

                @if requires_speaker_order {
                    @for speaker_idx in 0..tournament.substantive_speakers {
                        select name=(format!("teams[{no}].speakers[{speaker_idx}].id")) {
                            // todo: compute options and insert here
                            @let team_speakers = &debate.speakers_of_team.get(&team.id).unwrap();
                            @for speaker in team_speakers.iter() {
                                @let selected = existing_team.map(|team| {
                                    team.speakers
                                        .get(speaker_idx as usize)
                                        .map(|existing_speaker| {
                                            existing_speaker.id == speaker.id
                                        })
                                        .unwrap_or(false)
                                }).unwrap_or(false);
                                // todo: do we need to sort (maybe require this
                                // upstream when we get the DebateRepr)
                                option value=(speaker.id) selected=(selected) {
                                    (speaker.name)
                                }
                            }
                        }

                        @if requires_speaks {
                            @let existing_value = existing_team.map(|team| {
                                team.speakers
                                    .get(speaker_idx as usize)
                                    .map(|existing_speaker| {
                                        existing_speaker.score
                                    })
                                    .unwrap_or(None)
                            }).flatten();
                            input type="number"
                                  value=(existing_value)
                                  name=(format!("teams[{no}].speakers[{speaker_idx}].score"))
                                  min=(tournament.min_substantive_speak().unwrap().to_string())
                                  max=(tournament.max_substantive_speak().unwrap().to_string())
                                  step=(tournament.speak_step().unwrap().to_string());
                        }
                    }
                    @if tournament.reply_speakers {
                        @let reply_speaker_idx = tournament.substantive_speakers;
                        select name=(format!("teams[{no}].speakers[{reply_speaker_idx}].id")) {
                            @let team_speakers = &debate.speakers_of_team.get(&team.id).unwrap();
                            @for speaker in team_speakers.iter() {
                                @let selected = existing_team.map(|team| {
                                    team.speakers
                                        .get(reply_speaker_idx as usize)
                                        .map(|existing_speaker| {
                                            existing_speaker.id == speaker.id
                                        })
                                        .unwrap_or(false)
                                }).unwrap_or(false);
                                option value=(speaker.id) selected=(selected) {
                                    (speaker.name) " (Reply)"
                                }
                            }
                        }

                        @if requires_speaks {
                            @let existing_value = existing_team.map(|team| {
                                team.speakers
                                    .get(reply_speaker_idx as usize)
                                    .map(|existing_speaker| {
                                        existing_speaker.score
                                    })
                                    .unwrap_or(None)
                            }).flatten();
                            input type="number"
                                  value=(existing_value)
                                  name=(format!("teams[{no}].speakers[{reply_speaker_idx}].score"))
                                  min=(tournament.reply_speech_min_speak.map(|v| v.to_string()).unwrap_or_default())
                                  max=(tournament.reply_speech_max_speak.map(|v| v.to_string()).unwrap_or_default())
                                  step=(tournament.speak_step().map(|v| v.to_string()).unwrap_or("1".to_string()));
                        }
                    }
                } @else {
                    @if is_elim {
                        @let existing_points = existing_team.and_then(|team| team.points);
                        input type="radio" name=(format!("teams[{no}].points")) value="1" checked[existing_points == Some(1)];
                        label { "Win" }
                        input type="radio" name=(format!("teams[{no}].points")) value="0" checked[existing_points == Some(0)];
                        label { "Loss" }
                    } @else {
                        @let existing_points = existing_team.and_then(|team| team.points);
                        input type="number" name=(format!("teams[{no}].points"))
                            value=(existing_points)
                            min=(0)
                            max=(if tournament.teams_per_side == 4 {
                                    0
                                } else {
                                    assert!(tournament.teams_per_side == 2);
                                    1
                                })
                            step = 1;
                    }
                }
            }
        }
    }
}
