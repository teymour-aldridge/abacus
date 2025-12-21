use axum::extract::Path;
use diesel::prelude::*;
use hypertext::{maud, prelude::*};

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
};

pub async fn view_tournament_rounds_page(
    Path((tid, rid)): Path<(String, i64)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(&tid))
        .filter(tournament_rounds::seq.eq(rid))
        .load::<Round>(&mut *conn)
        .unwrap();

    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let current_rounds = Round::current_rounds(&tournament.id, &mut *conn);

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) active_page=(None) selected_seq=(Some(rid)) {
                    h1 {
                        "Rounds "
                            @for (i, round) in rounds.iter().enumerate() {
                                @if i > 0 {
                                    ", "
                                }
                                (round.name)
                            }
                    }
                    a href=(format!("/tournaments/{}/rounds/{}/setup",
                            tournament.id,
                            rid)) class="btn btn-primary" {
                        "Go to Setup"
                    }
                }
            })
            .render(),
    )
}
