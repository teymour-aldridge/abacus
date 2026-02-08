//! Shared form components for ballot submission and editing
//!
//! This module contains simple, reusable rendering functions for ballot forms.

use hypertext::{Renderable, maud, prelude::*};

use crate::tournaments::{Tournament, participants::Speaker, rounds::Motion};

/// Renders a motion selection dropdown
pub struct MotionSelector<'a> {
    pub motions: &'a [Motion],
    pub selected_motion_id: Option<&'a str>,
    pub field_name: &'a str,
}

impl<'a> Renderable for MotionSelector<'a> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            section class="mb-5" {
                h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                    "Motion"
                }
                select class="form-select" name=(self.field_name) required {
                    @if self.selected_motion_id.is_none() {
                        option selected value="" { "Select motion" }
                    } @else {
                        option value="" { "Select motion" }
                    }
                    @for motion in self.motions {
                        @if Some(motion.id.as_str()) == self.selected_motion_id {
                            option selected value=(motion.id) { (motion.motion) }
                        } @else {
                            option value=(motion.id) { (motion.motion) }
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}

/// Renders a single speaker input row with optional score field.
pub struct SpeakerInput<'a> {
    pub team_name: &'a str,
    pub speaker_position: usize,
    pub speakers: &'a [Speaker],
    pub selected_speaker_id: Option<&'a str>,
    pub score: Option<f32>,
    pub min_score: Option<f32>,
    pub max_score: Option<f32>,
    pub score_step: Option<f32>,
    pub speaker_field_name: &'a str,
    pub score_field_name: &'a str,
    /// Whether to render the speaker selection dropdown.
    pub show_speaker_select: bool,
    /// Whether to render the score input field.
    pub show_score: bool,
}

impl<'a> Renderable for SpeakerInput<'a> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let placeholder = match (self.min_score, self.max_score) {
            (Some(min), Some(max)) => format!("Score ({min}-{max})"),
            _ => "Score".to_string(),
        };

        let min_str = self.min_score.map(|v| v.to_string());
        let max_str = self.max_score.map(|v| v.to_string());
        let step_str = self.score_step.map(|v| v.to_string());

        maud! {
            div class="mb-2" {
                label class="form-label text-uppercase fw-bold" {
                    (self.team_name) " - Speaker " (self.speaker_position + 1)
                }
                @if self.show_speaker_select {
                    select class="form-select mb-2" name=(self.speaker_field_name) required {
                        @if self.selected_speaker_id.is_none() {
                            option selected value="" { "Select speaker" }
                        } @else {
                            option value="" { "Select speaker" }
                        }
                        @for speaker in self.speakers {
                            @if Some(speaker.id.as_str()) == self.selected_speaker_id {
                                option selected value=(speaker.id) { (speaker.name) }
                            } @else {
                                option value=(speaker.id) { (speaker.name) }
                            }
                        }
                    }
                }
                @if self.show_score {
                    input
                        required
                        name=(self.score_field_name)
                        type="number"
                        class="form-control"
                        placeholder=(placeholder)
                        min=[min_str.as_deref()]
                        max=[max_str.as_deref()]
                        step=[step_str.as_deref()]
                        value=[self.score];
                }
            }
        }.render_to(buffer);
    }
}

/// Renders a selector for which teams advance in an elimination round.
///
/// For 1 advancing team, renders radio buttons (single `winning_team_id`
/// field).  For multiple advancing teams, renders checkboxes (indexed
/// `advancing_team_ids[N]` fields).
///
/// The corresponding `SingleBallot` struct has both a `winning_team_id` and
/// an `advancing_team_ids` field; the handler merges them.
pub struct AdvancingTeamSelector<'a> {
    /// `(team_id, team_name)` pairs for all teams in the debate.
    pub teams: &'a [(String, String)],
    /// How many teams should advance (1 for 2-team or final, 2 for 4-team
    /// non-final).
    pub num_advancing: usize,
    /// Form field name prefix, e.g. `""` or `"ballots[0]."`.
    pub field_prefix: &'a str,
    /// Currently selected advancing team IDs (for editing existing ballots).
    pub selected: Vec<String>,
}

impl<'a> Renderable for AdvancingTeamSelector<'a> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let use_radio = self.num_advancing == 1;

        maud! {
            section class="mb-5" {
                h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                    @if self.num_advancing == 1 {
                        "Winning Team"
                    } @else {
                        "Advancing Teams (select " (self.num_advancing) ")"
                    }
                }

                @for (idx, (team_id, team_name)) in self.teams.iter().enumerate() {
                    @let is_selected = self.selected.contains(team_id);

                    div class="form-check mb-2" {
                        @if use_radio {
                            // Radio buttons share a single name so they are
                            // mutually exclusive.  Submitted as a single
                            // value: `winning_team_id=<id>`.
                            input
                                class="form-check-input"
                                type="radio"
                                name=(format!("{}winning_team_id", self.field_prefix))
                                value=(team_id)
                                id=(format!("advancing_{}", team_id))
                                checked[is_selected];
                        } @else {
                            // Checkboxes use indexed names so multiple values
                            // are submitted as a Vec.
                            input
                                class="form-check-input"
                                type="checkbox"
                                name=(format!("{}advancing_team_ids[{}]", self.field_prefix, idx))
                                value=(team_id)
                                id=(format!("advancing_{}", team_id))
                                checked[is_selected];
                        }
                        label
                            class="form-check-label"
                            for=(format!("advancing_{}", team_id))
                        {
                            (team_name)
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}

/// Helper to get score bounds for a speaker type
pub fn get_score_bounds(
    tournament: &Tournament,
    is_reply: bool,
) -> (Option<f32>, Option<f32>) {
    if is_reply {
        (
            tournament
                .reply_speech_min_speak
                .or(tournament.substantive_speech_min_speak),
            tournament
                .reply_speech_max_speak
                .or(tournament.substantive_speech_max_speak),
        )
    } else {
        (
            tournament.substantive_speech_min_speak,
            tournament.substantive_speech_max_speak,
        )
    }
}
