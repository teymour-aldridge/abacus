use hypertext::prelude::*;

use crate::tournaments::{
    Tournament,
    participants::TournamentParticipants,
    rounds::draws::{DebateRepr, RoundDrawRepr},
};

pub mod create;
pub mod drawalgs;

/// Renders the provided draw as a table.
pub struct DrawForRound<'a, F> {
    pub tournament: &'a Tournament,
    pub repr: &'a RoundDrawRepr,
    pub actions: F,
    pub participants: &'a TournamentParticipants,
}

impl<'a, F, R> Renderable for DrawForRound<'a, F>
where
    F: Fn(&'a DebateRepr) -> R,
    R: Renderable,
{
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            h3 {
                "Unallocated judges"
            }
            div id="unallocatedJudges" class="container mb-3" {
                @for judge in self.participants.judges.values().filter(|judge| {
                    // todo: could do this using SQLite
                    let is_allocated_on_draw =
                        self.repr.debates.iter().any(|debate| {
                            debate.judges_of_debate.iter().any(|debate_judge| {
                                debate_judge.judge_id == judge.id
                            })
                        });
                    !is_allocated_on_draw
                }) {
                    div class="judge-badge"
                        data-judge-id=(judge.id)
                        data-judge-number=(judge.number)
                        data-role="P"
                        draggable="true"
                    {
                        (judge.name) " (j" (judge.number) ")"
                    }
                }
            }

            RoomsOfRoundTable tournament=(&self.tournament) repr=(&self.repr)
                                participants=(&self.participants)
                                actions=(&self.actions)
                                body_only=(false);

        }
        .render_to(buffer);
    }
}

pub struct RoomsOfRoundTable<'r, F> {
    pub tournament: &'r Tournament,
    pub repr: &'r RoundDrawRepr,
    pub participants: &'r TournamentParticipants,
    pub actions: F,
    /// Whether or not to render the headers of the table as well. This should
    /// be set to false when rendering multiple rooms as part of the same table.
    pub body_only: bool,
}

impl<'a, F, R: Renderable> Renderable for RoomsOfRoundTable<'a, F>
where
    F: Fn(&'a DebateRepr) -> R,
{
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let table_body_contents = maud! {
            @for debate in self.repr.debates.iter() {
                tr {
                    th scope="row" {
                        (debate.debate.number)
                    }
                    @for debate_team in &debate.teams_of_debate {
                        td {
                            a href = (format!("/tournaments/{}/teams/{}", &self.tournament.id, debate_team.team_id)) {
                                ({
                                    let team = self.participants.teams.get(&debate_team.team_id).unwrap();
                                    self.participants.canonical_name_of_team(&team)
                                })
                            }
                        }
                    }
                    td class="debate-judges-container" data-debate-id=(debate.debate.id) {
                        // Chair slot (single)
                        div class="judge-role-section" {
                            label class="role-label" { "Chair:" }
                            div class="chair-slot judge-drop-zone"
                                data-debate-id=(debate.debate.id)
                                data-role="C"
                            {
                                @for debate_judge in debate.judges_of_debate.iter().filter(|dj| dj.status == "C") {
                                    @let judge = &debate.judges.get(&debate_judge.judge_id).unwrap();
                                    div class="judge-badge"
                                        data-judge-id=(judge.id)
                                        data-judge-number=(judge.number)
                                        data-role="C"
                                        draggable="true"
                                    {
                                        span class="judge-name" {
                                            (judge.name) " (j" (judge.number) ")"
                                        }
                                        span class="judge-remove-btn" title="Remove" {
                                            "×"
                                        }
                                    }
                                }
                                @if debate.judges_of_debate.iter().all(|dj| dj.status != "C") {
                                    span class="drop-placeholder" { "Drop chair here" }
                                }
                            }
                        }
                        // Panelists slot (multiple)
                        div class="judge-role-section" {
                            label class="role-label" { "Panelists:" }
                            div class="panelist-slot judge-drop-zone"
                                data-debate-id=(debate.debate.id)
                                data-role="P"
                            {
                                @for debate_judge in debate.judges_of_debate.iter().filter(|dj| dj.status == "P") {
                                    @let judge = &debate.judges.get(&debate_judge.judge_id).unwrap();
                                    div class="judge-badge"
                                        data-judge-id=(judge.id)
                                        data-judge-number=(judge.number)
                                        data-role="P"
                                        draggable="true"
                                    {
                                        span class="judge-name" {
                                            (judge.name) " (j" (judge.number) ")"
                                        }
                                        span class="judge-remove-btn" title="Remove" {
                                            "×"
                                        }
                                    }
                                }
                                @if debate.judges_of_debate.iter().all(|dj| dj.status != "P") {
                                    span class="drop-placeholder" { "Drop panelists here" }
                                }
                            }
                        }
                        // Trainees slot (multiple)
                        div class="judge-role-section" {
                            label class="role-label" { "Trainees:" }
                            div class="trainee-slot judge-drop-zone"
                                data-debate-id=(debate.debate.id)
                                data-role="T"
                            {
                                @for debate_judge in debate.judges_of_debate.iter().filter(|dj| dj.status == "T") {
                                    @let judge = &debate.judges.get(&debate_judge.judge_id).unwrap();
                                    div class="judge-badge"
                                        data-judge-id=(judge.id)
                                        data-judge-number=(judge.number)
                                        data-role="T"
                                        draggable="true"
                                    {
                                        span class="judge-name" {
                                            (judge.name) " (j" (judge.number) ")"
                                        }
                                        span class="judge-remove-btn" title="Remove" {
                                            "×"
                                        }
                                    }
                                }
                                @if debate.judges_of_debate.iter().all(|dj| dj.status != "T") {
                                    span class="drop-placeholder" { "Drop trainees here" }
                                }
                            }
                        }
                    }
                    td {
                        @let rendered = (self.actions)(debate);
                        (rendered)
                    }
                }
            }
        };

        maud! {
            @if !self.body_only {
                table class = "table" {
                    DrawTableHeaders tournament=(&self.tournament);
                    tbody {
                        (table_body_contents)
                    }
                }
            } @else {
                tbody class="table-group-divider" {
                    (table_body_contents)
                }
            }
        }
        .render_to(buffer);
    }
}

pub struct DrawTableHeaders<'r> {
    pub tournament: &'r Tournament,
}

impl Renderable for DrawTableHeaders<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            thead {
                tr {
                    th scope="col" {
                        "#"
                    }
                    @for i in 0..self.tournament.teams_per_side {
                        // todo: should use "OG, OO, CG, CO" where appropriate
                        th scope="col" {
                            "Prop " (i+1)
                        }
                        th scope="col" {
                            "Opp " (i+1)
                        }
                    }
                    th scope="col" {
                        "Judges"
                    }
                    th scope="col" {
                        "Manage"
                    }
                }
            }
        }
        .render_to(buffer);
    }
}
