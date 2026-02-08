use hypertext::prelude::*;

use crate::tournaments::{
    Tournament,
    participants::Speaker,
    rounds::{
        Motion, Round,
        ballots::{BallotRepr, form_components::get_score_bounds},
        draws::DebateRepr,
    },
};

use super::form_components::{
    AdvancingTeamSelector, MotionSelector, SpeakerInput,
};

/// Renders the form fields for a single ballot.
///
/// This component is reused by both the public submission page and the admin
/// edit page. The `field_prefix` controls the form field name prefix (e.g.
/// `""` for a single ballot or `"ballots[0]."` when editing multiple ballots).
pub struct BallotFormFields<'a> {
    pub tournament: &'a Tournament,
    pub debate: &'a DebateRepr,
    pub round: &'a Round,
    pub motions: &'a [Motion],
    pub current_values: Option<&'a BallotRepr>,
    pub field_prefix: &'a str,
    /// For elimination rounds: how many teams advance. `None` for prelim
    /// rounds.
    pub num_advancing: Option<usize>,
}

impl<'a> hypertext::Renderable for BallotFormFields<'a> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let show_speakers =
            self.tournament.round_requires_speaker_order(self.round);
        let show_scores = self.tournament.round_requires_speaks(self.round);
        let is_elim = self.round.is_elim();

        let substantive_speakers =
            self.tournament.substantive_speakers as usize;
        let reply_speakers = if self.tournament.reply_speakers { 1 } else { 0 };
        let total_speakers = substantive_speakers + reply_speakers;

        maud! {
            @if self.motions.len() > 1 {
                (MotionSelector {
                    motions: self.motions,
                    selected_motion_id: self.current_values
                        .map(|b| b.metadata.motion_id.as_str()),
                    field_name: &format!("{}motion_id", self.field_prefix),
                })
            } @else if let Some(motion) = self.motions.first() {
                input type="hidden"
                    name=(format!("{}motion_id", self.field_prefix))
                    value=(motion.id);
            }

            // Advancing team selector for elimination rounds
            @if is_elim {
                @let num_advancing = self.num_advancing.unwrap_or(1);
                @let teams: Vec<(String, String)> = self.debate.teams_of_debate
                    .iter()
                    .map(|dt| {
                        let name = self.debate.teams
                            .get(&dt.team_id)
                            .map(|t| t.name.clone())
                            .unwrap_or_default();
                        (dt.team_id.clone(), name)
                    })
                    .collect();
                @let selected: Vec<String> = self.current_values
                    .map(|b| {
                        b.team_ranks.iter()
                            .filter(|tr| tr.points == 1)
                            .map(|tr| tr.team_id.clone())
                            .collect()
                    })
                    .unwrap_or_default();

                (AdvancingTeamSelector {
                    teams: &teams,
                    num_advancing,
                    field_prefix: self.field_prefix,
                    selected,
                })
            }

            // Speaker positions and scores (conditional)
            @if show_speakers {
                section class="mb-5" {
                    h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                        @if show_scores {
                            "Speaker Scores"
                        } @else {
                            "Speaker Positions"
                        }
                    }

                    @for side in 0..2i64 {
                        div class="mb-5" {
                            @for seq in 0..self.tournament.teams_per_side {
                                @let team_idx = (2 * seq + side) as usize;
                                @let debate_team = self.debate
                                    .team_of_side_and_seq(side, seq);
                                @let team = self.debate.teams
                                    .get(&debate_team.team_id).unwrap();
                                @let speakers: &[Speaker] = self.debate
                                    .speakers_of_team
                                    .get(&debate_team.team_id)
                                    .map(|v| v.as_slice())
                                    .unwrap_or(&[]);
                                @let scores = self.current_values
                                    .map(|b| b.scores_of_team(&debate_team.team_id));

                                @if seq > 0 {
                                    hr class="my-3";
                                }

                                div class="mb-3" {
                                    h3 class="h5 text-muted" { (team.name) }

                                    @for speaker_row in 0..total_speakers {
                                        @let is_reply = speaker_row >= substantive_speakers;
                                        @let (min_score, max_score) =
                                            get_score_bounds(self.tournament, is_reply);
                                        @let existing_score = scores.as_ref()
                                            .and_then(|s| s.get(speaker_row));

                                        (SpeakerInput {
                                            team_name: &team.name,
                                            speaker_position: speaker_row,
                                            speakers,
                                            selected_speaker_id: existing_score
                                                .map(|s| s.speaker_id.as_str()),
                                            score: existing_score
                                                .and_then(|s| s.score),
                                            min_score,
                                            max_score,
                                            score_step: self.tournament
                                                .substantive_speech_step,
                                            speaker_field_name: &format!(
                                                "{}teams[{}].speakers[{}].id",
                                                self.field_prefix, team_idx,
                                                speaker_row
                                            ),
                                            score_field_name: &format!(
                                                "{}teams[{}].speakers[{}].score",
                                                self.field_prefix, team_idx,
                                                speaker_row
                                            ),
                                            show_speaker_select: true,
                                            show_score: show_scores,
                                        })
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        .render_to(buffer);
    }
}
