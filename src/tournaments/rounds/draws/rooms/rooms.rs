use axum::{Extension, extract::Path};
use diesel::prelude::*;
use diesel::result::Error as DieselError;
use hypertext::{Renderable, maud};
use serde::Deserialize;
use tokio::sync::broadcast::Sender;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{tournament_debates, tournament_rooms},
    state::Conn,
    tournaments::{Tournament, rooms::Room, rounds::draws::Debate},
    util_resp::{FailureResponse, StandardResponse, bad_request, success},
};

#[derive(Deserialize, Debug)]
pub struct MoveRoomForm {
    room_id: String,
    to_debate_id: Option<String>,
    rounds: Vec<String>,
}

pub async fn move_room(
    Path(tournament_id): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<MoveRoomForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids = form.rounds;

    let room = match tournament_rooms::table
        .filter(tournament_rooms::id.eq(&form.room_id))
        .filter(tournament_rooms::tournament_id.eq(&tournament.id))
        .first::<Room>(&mut *conn)
    {
        Ok(r) => r,
        Err(DieselError::NotFound) => {
            return bad_request(maud! { "Room not found" }.render());
        }
        Err(e) => {
            return Err(FailureResponse::from(e));
        }
    };

    let transaction_result: Result<StandardResponse, DieselError> = conn
        .transaction(|conn| {
            // Unassign room from any debate in the rounds
            diesel::update(
                tournament_debates::table.filter(
                    tournament_debates::round_id
                        .eq_any(&round_ids)
                        .and(tournament_debates::room_id.eq(&room.id)),
                ),
            )
            .set(tournament_debates::room_id.eq(None::<String>))
            .execute(conn)?;

            if let Some(to_debate_id) =
                form.to_debate_id.filter(|s| !s.is_empty())
            {
                let debate =
                    match tournament_debates::table
                        .filter(tournament_debates::id.eq(&to_debate_id).and(
                            tournament_debates::round_id.eq_any(&round_ids),
                        ))
                        .first::<Debate>(conn)
                    {
                        Ok(d) => d,
                        Err(DieselError::NotFound) => {
                            // This will rollback the transaction
                            return Err(diesel::result::Error::NotFound);
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    };

                // Unassign any existing room from the debate
                diesel::update(
                    tournament_debates::table
                        .filter(tournament_debates::id.eq(&debate.id)),
                )
                .set(tournament_debates::room_id.eq(None::<String>))
                .execute(conn)?;

                diesel::update(
                    tournament_debates::table
                        .filter(tournament_debates::id.eq(&debate.id)),
                )
                .set(tournament_debates::room_id.eq(Some(room.id.clone())))
                .execute(conn)?;
            }
            Ok(success(Default::default()))
        });

    let res = match transaction_result {
        Ok(res) => res,
        Err(DieselError::NotFound) => {
            return bad_request(maud! { "Debate not found" }.render());
        }
        Err(e) => {
            return bad_request(maud! { (e.to_string()) }.render());
        }
    };

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    res
}
