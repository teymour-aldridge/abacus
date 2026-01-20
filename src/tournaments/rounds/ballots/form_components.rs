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

/// Renders a single speaker score input field
pub struct SpeakerInput<'a> {
    pub team_name: &'a str,
    pub speaker_position: usize,
    pub speakers: &'a [Speaker],
    pub selected_speaker_id: Option<&'a str>,
    pub score: Option<f32>,
    pub min_score: f32,
    pub max_score: f32,
    pub score_step: f32,
    pub speaker_field_name: &'a str,
    pub score_field_name: &'a str,
}

impl<'a> Renderable for SpeakerInput<'a> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let placeholder =
            format!("Score ({}-{})", self.min_score, self.max_score);

        maud! {
            div class="mb-2" {
                label class="form-label text-uppercase fw-bold" {
                    (self.team_name) " - Speaker " (self.speaker_position + 1)
                }
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
                @if let Some(score_value) = self.score {
                    input
                        required
                        name=(self.score_field_name)
                        type="number"
                        class="form-control"
                        min=(self.min_score)
                        max=(self.max_score)
                        step=(self.score_step)
                        placeholder=(placeholder)
                        value=(score_value);
                } @else {
                    input
                        required
                        name=(self.score_field_name)
                        type="number"
                        class="form-control"
                        min=(self.min_score)
                        max=(self.max_score)
                        step=(self.score_step)
                        placeholder=(placeholder);
                }
            }
        }.render_to(buffer);
    }
}

/// Helper to get score bounds for a speaker type
pub fn get_score_bounds(tournament: &Tournament, is_reply: bool) -> (f32, f32) {
    if is_reply {
        (
            tournament
                .reply_speech_min_speak
                .unwrap_or(tournament.substantive_speech_min_speak),
            tournament
                .reply_speech_max_speak
                .unwrap_or(tournament.substantive_speech_max_speak),
        )
    } else {
        (
            tournament.substantive_speech_min_speak,
            tournament.substantive_speech_max_speak,
        )
    }
}
