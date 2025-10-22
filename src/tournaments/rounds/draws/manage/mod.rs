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
            div class="row" {
                @for judge in self.participants.judges.values().filter(|judge| {
                    // todo: could do this using SQLite
                    let allocated_on_draw =
                        self.repr.debates.iter().any(|debate| {
                            debate.judges_of_debate.iter().any(|debate_judge| {
                                debate_judge.judge_id == judge.id
                            })
                        });
                    !allocated_on_draw
                }) {
                    div class="col" {
                        p {
                            (judge.name) "(j" (judge.number) ")"
                        }
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
                    td {
                        @for debate_judge in &debate.judges_of_debate {
                            @let judge = &debate.judges.get(&debate_judge.judge_id).unwrap();
                            div class="card m-1" {
                                div class="card-body" {
                                    p class="card-text" {
                                        (judge.name) " (" (debate_judge.status) ", " "j" (judge.number) ")"
                                    }
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
