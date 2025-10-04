use hypertext::prelude::*;

use crate::tournaments::{
    Tournament,
    participants::TournamentParticipants,
    rounds::draws::{DebateRepr, RoundDrawRepr},
};

pub mod create;
pub mod drawalgs;

/// Renders the provided draw as a table.
pub struct DrawTableRenderer<'a, F> {
    pub tournament: &'a Tournament,
    pub repr: &'a RoundDrawRepr,
    pub actions: F,
    pub participants: &'a TournamentParticipants,
}

impl<'a, F, R> Renderable for DrawTableRenderer<'a, F>
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
            table class = "table" {
                thead {
                    tr {
                        th scope="col" {
                            "#"
                        }
                        @for i in 0..self.tournament.teams_per_side {
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
                tbody {
                    @for debate in self.repr.debates.iter() {
                        tr {
                            th scope="row" {
                                (debate.debate.number)
                            }
                            @for debate_team in &debate.teams_of_debate {
                                td {
                                    a href = (format!("/tournaments/{}/teams/{}", &self.tournament.id, debate_team.team_id)) {
                                        (self.participants.teams.get(&debate_team.team_id).unwrap().name)
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
                }
            }
        }.render_to(buffer);
    }
}
