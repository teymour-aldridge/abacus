use axum::extract::Path;
use hypertext::prelude::*;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{TournamentRounds, draws::DebateRepr},
    },
    util_resp::{StandardResponse, success},
};

pub async fn view_ballot_set_page(
    Path((tournament_id, debate_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let debate = DebateRepr::fetch(&debate_id, &mut *conn);
    let ballots = debate.ballots(&mut *conn);

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper rounds=(&all_rounds) tournament=(&tournament) {
                    h1 { "Ballots for Debate " (debate.debate.number) }

                    @for ballot in &ballots {
                        div class="card mb-3" {
                            div class="card-header" {
                                "Ballot from " (debate.judges.get(&ballot.ballot().judge_id).unwrap().name) " submitted at " (ballot.ballot().submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
                            }
                            div class="card-body" {
                                table class="table" {
                                    thead {
                                        tr {
                                            th { "Speaker" }
                                            th { "Team" }
                                            th { "Position" }
                                            th { "Score" }
                                        }
                                    }
                                    tbody {
                                        @for team_id in ballot.teams() {
                                            @for score in ballot.scores_of_team(&team_id) {
                                                tr {
                                                    td { (debate.speakers_of_team.get(&team_id).unwrap().iter().find(|s| s.id == score.speaker_id).unwrap().name) }
                                                    td { (debate.teams.get(&score.team_id).unwrap().name) }
                                                    td { (score.speaker_position) }
                                                    td { (score.score) }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
