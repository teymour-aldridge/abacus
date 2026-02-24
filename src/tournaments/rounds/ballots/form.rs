use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
};
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use hypertext::prelude::*;
use serde::de::DeserializeOwned;

pub struct QsForm<T>(pub T);

#[async_trait]
impl<T, S> FromRequest<S> for QsForm<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request(
        req: Request,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        let config = serde_qs::Config::new(10, false);
        let res = config
            .deserialize_bytes::<T>(&bytes)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        Ok(QsForm(res))
    }
}

use crate::tournaments::{
    RoundKind, Tournament,
    rounds::{
        ballots::manage::edit::BallotForm, draws::DebateRepr,
        side_names::name_of_side,
    },
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

    let expected_version = existing.map(|e| e.expected_version).unwrap_or(0);

    maud! {
        input type="hidden" name="expected_version" value=(expected_version);

        @if debate.motions.len() > 1 {
            div class="card mb-4 border-light shadow-sm" {
                div class="card-body" {
                    label class="form-label fw-bold text-muted small text-uppercase mb-2" { "Motion" }
                    select name="motion_id" class="form-select form-select-lg" {
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
                }
            }
        } @else if let Some(motion) = debate.motions.iter().next() {
            input name="motion_id" hidden type="text" value=(motion.0);
        }

        div class="row g-4 mb-5" {
            @for seq in 0..tournament.teams_per_side {
                @for side in 0..2 {
                    @let no = seq * 2 + side;
                    @let team = debate.team_of_side_and_seq(side, seq);
                    @let team_meta = debate.teams.get(&team.team_id).unwrap();
                    @let existing_team = existing.and_then(|e| e.teams.get(no as usize));

                    div class="col-md-6" {
                        div class="card h-100 shadow-sm border-light" {
                            div class="card-header bg-white py-3 d-flex justify-content-between align-items-center" {
                                h5 class="mb-0 fw-bold" { (team_meta.name) }
                                span class="badge rounded-pill bg-light text-dark border" {
                                    (name_of_side(tournament, side, seq, true))
                                }
                            }
                            div class="card-body p-4" {
                                @if requires_speaker_order {
                                    @for speaker_idx in 0..tournament.substantive_speakers {
                                        @let pos_name = tournament.speaker_position_name(side, seq, speaker_idx);
                                        div class="mb-4" {
                                            label class="form-label d-block fw-bold text-muted small text-uppercase mb-2" { (pos_name) }
                                            div class="row g-2" {
                                                div class="col-7" {
                                                    select name=(format!("teams[{no}][speakers][{speaker_idx}][id]")) class="form-select" {
                                                        @let team_speakers = debate.speakers_of_team.get(&team.team_id).unwrap();
                                                        @for speaker in team_speakers {
                                                            @let selected = existing_team.map(|t| {
                                                                t.speakers.get(speaker_idx as usize)
                                                                    .map(|s| s.id == speaker.id)
                                                                    .unwrap_or(false)
                                                            }).unwrap_or(false);
                                                            option value=(speaker.id) selected=(selected) {
                                                                (speaker.name)
                                                            }
                                                        }
                                                    }
                                                }

                                                @if requires_speaks {
                                                    @let existing_value = existing_team.and_then(|t| {
                                                        t.speakers.get(speaker_idx as usize).and_then(|s| s.score)
                                                    });
                                                    div class="col-5" {
                                                        input type="number"
                                                              class="form-control text-center"
                                                              placeholder="Score"
                                                              value=(existing_value)
                                                              name=(format!("teams[{no}][speakers][{speaker_idx}][score]"))
                                                              min=(tournament.min_substantive_speak().unwrap().to_string())
                                                              max=(tournament.max_substantive_speak().unwrap().to_string())
                                                              step=(tournament.speak_step().unwrap().to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    @if tournament.reply_speakers {
                                        @let reply_idx = tournament.substantive_speakers;
                                        @let pos_name = tournament.speaker_position_name(side, seq, reply_idx);
                                        div class="mb-3" {
                                            label class="form-label d-block fw-bold text-muted small text-uppercase mb-2" { (pos_name) }
                                            div class="row g-2" {
                                                div class="col-7" {
                                                    select name=(format!("teams[{no}][speakers][{reply_idx}][id]")) class="form-select" {
                                                        @let team_speakers = debate.speakers_of_team.get(&team.team_id).unwrap();
                                                        @for speaker in team_speakers {
                                                            @let selected = existing_team.map(|t| {
                                                                t.speakers.get(reply_idx as usize)
                                                                    .map(|s| s.id == speaker.id)
                                                                    .unwrap_or(false)
                                                            }).unwrap_or(false);
                                                            option value=(speaker.id) selected=(selected) {
                                                                (speaker.name) " (Reply)"
                                                            }
                                                        }
                                                    }
                                                }

                                                @if requires_speaks {
                                                    @let existing_value = existing_team.and_then(|t| {
                                                        t.speakers.get(reply_idx as usize).and_then(|s| s.score)
                                                    });
                                                    div class="col-5" {
                                                        input type="number"
                                                              class="form-control text-center"
                                                              placeholder="Score"
                                                              value=(existing_value)
                                                              name=(format!("teams[{no}][speakers][{reply_idx}][score]"))
                                                              min=(tournament.reply_speech_min_speak.map(|v| v.to_string()).unwrap_or_default())
                                                              max=(tournament.reply_speech_max_speak.map(|v| v.to_string()).unwrap_or_default())
                                                              step=(tournament.speak_step().map(|v| v.to_string()).unwrap_or("1".to_string()));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } @else {
                                    @let existing_points = existing_team.and_then(|team| team.points);
                                    @if is_elim {
                                        div class="d-flex gap-3" {
                                            div class="form-check" {
                                                input class="form-check-input" type="radio" id=(format!("win-{no}")) name=(format!("teams[{no}][points]")) value="1" checked[existing_points == Some(1)];
                                                label class="form-check-label" for=(format!("win-{no}")) { "Win" }
                                            }
                                            div class="form-check" {
                                                input class="form-check-input" type="radio" id=(format!("loss-{no}")) name=(format!("teams[{no}][points]")) value="0" checked[existing_points == Some(0)];
                                                label class="form-check-label" for=(format!("loss-{no}")) { "Loss" }
                                            }
                                        }
                                    } @else {
                                        label class="form-label fw-bold text-muted small text-uppercase mb-2" { "Points" }
                                        input type="number" class="form-control" name=(format!("teams[{no}][points]"))
                                            value=(existing_points)
                                            min=(0)
                                            max=(if tournament.teams_per_side == 4 { 0 } else { 1 })
                                            step = 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
