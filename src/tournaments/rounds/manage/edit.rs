use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use tokio::task::spawn_blocking;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::tournament_rounds,
    state::{Conn, LockedConn},
    template::Page,
    tournaments::{Tournament, rounds::Round, snapshots::take_snapshot},
    util_resp::GenerallyUsefulResponse,
};

#[get("/tournaments/<tid>/rounds/<rid>/edit")]
pub async fn edit_round_page(
    tid: &str,
    rid: &str,
    tournament: Tournament,
    user: User,
    _tab: IsTabDirector,
    mut conn: LockedConn<'_>,
) -> Option<Rendered<String>> {
    let round = match tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(tid))
        .filter(tournament_rounds::id.eq(rid))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return None,
    };

    Some(
        Page::new()
            .tournament(tournament)
            .user(user)
            .body(maud! {
                form {
                    div class="mb-3" {
                        label for="roundName" class="form-label" {
                            "Round name"
                        }
                        input type="text"
                              name="name"
                              class="form-control"
                              id="roundName"
                              aria-describedby="roundNameHelp"
                              value=(round.name);
                        div id="roundHelp" class="form-text" {
                            "A human-readable description of the round, for"
                            " example 'Round 1', or 'Grand final'"
                        }
                    }
                    div class="mb-3" {
                        label for="roundSeq" class="form-label" {
                            "Round sequence"
                        }
                        input type="integer"
                              name="seq"
                              class="form-control"
                              id="roundSeq"
                              aria-describedby="roundNameHelp"
                              value=(round.seq);
                        div id="roundSeq" class="form-text" {
                            "A human-readable description of the round, for"
                            " example 'Round 1', or 'Grand final'"
                        }
                    }
                    // todo: break categories
                }
            })
            .render(),
    )
}

#[derive(FromForm)]
pub struct EditRoundForm {
    #[field(validate = len(4..=32))]
    name: String,
    #[field(validate = range(1..))]
    seq: u32,
}

#[post("/tournaments/<tid>/rounds/<rid>/edit", data = "<form>")]
pub async fn do_edit_round(
    tid: &str,
    rid: &str,
    _tab: IsTabDirector,
    user: User,
    tournament: Tournament,
    form: Form<EditRoundForm>,
    conn: Conn,
) -> GenerallyUsefulResponse {
    let tid = tid.to_string();
    let rid = rid.to_string();
    spawn_blocking(move || {
        let mut conn = conn.get_sync();
        let round = match tournament_rounds::table
            .filter(tournament_rounds::id.eq(&tid))
            .filter(tournament_rounds::id.eq(&rid))
            .first::<Round>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(round) => round,
            None => return GenerallyUsefulResponse::NotFound(()),
        };

        let max = tournament_rounds::table
            .filter(tournament_rounds::tournament_id.eq(&tid))
            .select(diesel::dsl::max(tournament_rounds::seq))
            .get_result::<Option<i64>>(&mut *conn)
            .unwrap()
            .unwrap_or(1i64);

        if max + 1 < (form.seq as i64) {
            return GenerallyUsefulResponse::BadRequest(
                Page::new()
                    .user(user)
                    .tournament(tournament)
                    .body(maud! {
                        p {
                            "Error: round index is too large. It must be at most
                             one more than the current largest index, which is "
                             (max)
                            "."
                        }
                    })
                    .render(),
            );
        }

        let n = diesel::update(
            tournament_rounds::table.filter(tournament_rounds::id.eq(round.id)),
        )
        .set((
            tournament_rounds::name.eq(&form.name),
            tournament_rounds::seq.eq(&(form.seq as i64)),
        ))
        .execute(&mut *conn)
        .unwrap();
        assert_eq!(n, 1);

        take_snapshot(&tid, &mut *conn);

        return GenerallyUsefulResponse::Success(Redirect::to(format!(
            "/tournaments/{tid}/rounds"
        )));
    })
    .await
    .unwrap()
}
