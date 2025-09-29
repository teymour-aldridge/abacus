use std::collections::HashMap;

use hypertext::prelude::*;

use crate::tournaments::{
    Tournament,
    rounds::draws::{DebateRepr, DrawRepr},
    teams::Team,
};

pub mod create;
pub mod drawalgs;
pub mod edit;
pub mod view;

/// Renders the provided draw as a table.
pub struct DrawTableRenderer<'a, F> {
    pub tournament: &'a Tournament,
    pub repr: &'a DrawRepr,
    pub actions: F,
    pub teams: &'a HashMap<String, Team>,
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
                    @for (i, debate) in self.repr.debates.iter().enumerate() {
                        tr {
                            th scope="row" {
                                (i)
                            }
                            @for debate_team in &debate.teams_of_debate {
                                td {
                                    a href = (format!("/tournaments/{}/teams/{}", &self.tournament.id, debate_team.team_id)) {
                                        (self.teams.get(&debate_team.team_id).unwrap().name)
                                    }
                                }
                            }
                            td {
                                @for debate_judge in &debate.judges_of_debate {
                                    @let judge = &debate.judges.get(&debate_judge.judge_id).unwrap();
                                    (judge.name)
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
