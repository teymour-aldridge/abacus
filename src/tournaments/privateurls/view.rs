use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    schema::{
        tournament_debate_teams, tournament_debates, tournament_draws,
        tournament_team_speakers,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{Judge, Participant, Speaker},
        rounds::{
            Round,
            draws::{DebateRepr, DebateTeam},
            side_names::name_of_side,
        },
    },
    util_resp::{StandardResponse, success},
};

#[get("/tournaments/<tournament_id>/privateurls/<private_url>")]
pub async fn private_url_page(
    tournament_id: &str,
    private_url: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let participant = Participant::of_private_url_and_tournament(
        tournament_id,
        private_url,
        &mut *conn,
    )?;

    match participant {
        Participant::Speaker(speaker) => {
            private_url_page_of_speaker(tournament, speaker, user, &mut *conn)
        }
        Participant::Judge(judge) => {
            private_url_page_of_judge(tournament, judge, user, &mut *conn)
        }
    }
}

fn private_url_page_of_speaker(
    tournament: Tournament,
    speaker: Speaker,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> StandardResponse {
    let current_rounds = Round::current_rounds(&tournament.id, conn);

    let current_debate_info = if !current_rounds.is_empty() {
        // todo: should make sure that teams can only ever be allocated to ONE
        // round per sequence number

        // todo: make sure that when speakers are on multiple teams, only one
        // of those teams can be marked available (and placed on the draw)
        // across all rounds (!)

        let team_debate = tournament_debate_teams::table
            .filter(
                tournament_debate_teams::team_id.eq_any(
                    tournament_team_speakers::table
                        .filter(
                            tournament_team_speakers::speaker_id
                                .eq(&speaker.id),
                        )
                        .select(tournament_team_speakers::team_id),
                ),
            )
            .filter(tournament_debate_teams::debate_id.eq_any({
                let (main, sq) = diesel::alias!(
                    tournament_draws as main,
                    tournament_draws as sq
                );

                tournament_debates::table
                    .inner_join(
                        main.on(main.field(tournament_draws::round_id).eq_any(
                            current_rounds
                                .iter()
                                .map(|round| round.id.clone())
                                .collect::<Vec<_>>(),
                        )),
                    )
                    .filter(main.field(tournament_draws::version).eq({
                        sq.filter(
                            sq.field(tournament_draws::round_id)
                                .eq(main.field(tournament_draws::round_id)),
                        )
                        .select(diesel::dsl::max(
                            sq.field(tournament_draws::version),
                        ))
                        .single_value()
                        .assume_not_null()
                    }))
                    .select(tournament_debates::id)
            }))
            // .select(tournament_debate_teams::all_columns)
            .first::<DebateTeam>(conn)
            .optional()
            .unwrap();

        if let Some(team_debate) = team_debate {
            let tournament = tournament.clone();
            let debate = DebateRepr::fetch(&team_debate.debate_id, conn);

            Some(maud! {
                div class="card" {
                    div class="card-body" {
                        h5 class="card-title" {
                            "You are debating as the "
                            (name_of_side(&tournament, team_debate.side, team_debate.seq, false))
                            " team "
                            @match &debate.room {
                                Some(room) => {
                                    @if let Some(url) = &room.url {
                                        a href=(url) {
                                            (room.name)
                                        }
                                    } @else {
                                        (room.name)
                                    }
                                }
                                None => " (note: no room assigned)"
                            }
                        }

                    }
                }
            })
        } else {
            None
        }
    } else {
        None
    };

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                // todo: layout with columns
                (current_debate_info)
            })
            .render(),
    )
}

fn private_url_page_of_judge(
    tournament: Tournament,
    judge: Judge,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> StandardResponse {
    todo!()
}
