use std::fmt::Write;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use itertools::Either;
use rocket::{
    FromForm, Responder, State,
    form::Form,
    futures::{SinkExt, StreamExt},
    get, post,
};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        tournament_debate_judges, tournament_debates, tournament_judges,
        tournament_rounds,
    },
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        participants::{DebateJudge, Judge, TournamentParticipants},
        rounds::{
            Round,
            draws::{
                Debate, DebateRepr, RoundDrawRepr, manage::DrawTableRenderer,
            },
        },
    },
    util_resp::{StandardResponse, bad_request, err_not_found, success},
    widgets::alert::ErrorAlert,
};

#[get("/tournaments/<tournament_id>/rounds/<round_id>/draw/edit?<table_only>")]
pub async fn edit_draw_page(
    tournament_id: &str,
    round_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    table_only: bool,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::id.eq(round_id))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return err_not_found(),
    };

    let repr = RoundDrawRepr::of_round(round.clone(), &mut *conn);

    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

    let table = DrawTableRenderer {
        tournament: &tournament,
        repr: &repr,
        actions: |_: &DebateRepr| maud! {"None"},
        participants: &participants,
    };

    if table_only {
        success(table.render())
    } else {
        success(
            Page::new()
                .tournament(tournament.clone())
                .user(user)
                .body(maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-response-targets@2.0.2" integrity="sha384-T41oglUPvXLGBVyRdZsVRxNWnOOqCynaPubjUVjxhsjFTKrFJGEMm3/0KGmNQ+Pg" crossorigin="anonymous" {
                    }

                    h1 {
                        "Edit draw for " (round.name)
                    }

                    div id="cmdBar" {
                        div id = "cmdErrMsg" {}
                        form hx-post=(format!("/tournaments/{tournament_id}/rounds/{round_id}/draw/edit"))
                             hx-target="#tableContainer"
                             "hx-target-4*"="cmdErrMsg" {
                            div class="mb-3" {
                              label for="cmd" class="form-label" { "Enter a command" }
                              input type="text"
                                    class="form-control"
                                    id="cmd"
                                    aria-describedby="cmdHelp"
                                    name="cmd";
                              div id="cmdHelp" class="form-text" {
                                  "Enter a command to modify the draw."
                              }
                            }
                            button type="submit" class="btn btn-primary" { "Submit" }
                        }
                    }

                    div id="tableContainer"
                        hx-swap-oob="morphdom"
                        "ws-connect"=(format!("/tournaments/{tournament_id}/rounds/{round_id}/draw/edit?channel"))
                    {
                        (table)
                    }
                })
                .render(),
        )
    }
}

#[get("/tournaments/<tournament_id>/rounds/<round_id>/draw/edit?channel")]
/// Provides a WebSocket channel which notifies clients when the draw has been
/// updated. After receiving this message, the client should then reload the
/// draw (using [`edit_draw_page_tab_dir`], with the `table_only` flag set to
/// true).
pub async fn draw_updates(
    tournament_id: &str,
    round_id: &str,
    pool: &State<DbPool>,
    rx: &State<Receiver<Msg>>,
    ws: rocket_ws::WebSocket,
    user: User<false>,
) -> Option<rocket_ws::Channel<'static>> {
    let pool: DbPool = pool.inner().clone();

    let pool1 = pool.clone();
    let (tournament, round) = {
        let round_id = round_id.to_string();
        let tournament_id = tournament_id.to_string();

        match spawn_blocking(move || {
            let mut conn = pool1.get().unwrap();

            let tournament =
                Tournament::fetch(&tournament_id, &mut conn).ok()?;
            tournament
                .check_user_is_superuser(&user.id, &mut conn)
                .ok()?;

            let round = tournament_rounds::table
                .filter(tournament_rounds::id.eq(&round_id))
                .first::<Round>(&mut conn)
                .optional()
                .unwrap();

            round.map(|r| (tournament, r))
        })
        .await
        .unwrap()
        {
            Some(t) => t,
            None => return None,
        }
    };

    let mut rx: Receiver<Msg> = rx.inner().resubscribe();

    let tournament_id = tournament.id.clone();
    let round_id: String = round_id.to_string();
    Some(ws.channel(move |mut stream| {
        Box::pin(async move {
            loop {
                let msg = {
                    let msg = tokio::select! {
                        msg = rx.recv() => Either::Left(msg.unwrap()),
                        msg = stream.next() => Either::Right(msg.unwrap()),
                    };
                    let msg = match msg {
                        Either::Left(left) => left,
                        Either::Right(close) => {
                            let close = close?;
                            if matches!(close, rocket_ws::Message::Close(_)) {
                                return Ok(());
                            } else {
                                continue;
                            }
                        }
                    };

                    if msg.tournament.id == tournament.id
                        && let MsgContents::DrawUpdated(updated_round_id) =
                            &msg.inner
                        && updated_round_id == &round.id
                    {
                        msg
                    } else {
                        continue;
                    }
                };

                if msg.tournament.id == tournament.id
                    && let MsgContents::DrawUpdated(updated_round_id) =
                        msg.inner
                    && updated_round_id == round.id
                {
                    let pool1 = pool.clone();
                    let tournament = tournament.clone();
                    let round_id = round_id.clone();
                    let round = round.clone();
                    let tournament_id = tournament_id.clone();
                    let rendered = spawn_blocking(move || {
                        let mut conn = pool1.get().unwrap();

                        let repr =
                            RoundDrawRepr::of_round(round.clone(), &mut *conn);

                        let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

                        let table = DrawTableRenderer {
                            tournament: &tournament,
                            repr: &repr,
                            actions: |_: &DebateRepr| maud! {"None"},
                            participants: &participants
                        };

                        maud! {
                            div id="tableContainer"
                                hx-swap-oob="morphdom"
                                "ws-connect"=(format!("/tournaments/{tournament_id}/rounds/{round_id}/draw/edit?channel"))
                            {
                                (table)
                            }
                        }
                        .render()
                        .into_inner()
                    })
                    .await.unwrap();

                    let _ = stream
                        .send(rocket_ws::Message::Text(rendered))
                        .await;
                }
            }
        })
    }))
}

#[derive(FromForm)]
pub struct EditDrawForm {
    cmd: String,
}

#[derive(Responder)]
// todo: collapse into `GenerallyUsefulResponse`
pub enum FallibleResponse {
    Ok(Rendered<String>),
    #[response(status = 400)]
    BadReq(Rendered<String>),
    #[response(status = 404)]
    NotFound(()),
}

#[post(
    "/tournaments/<tournament_id>/rounds/<round_id>/draw/edit",
    data = "<form>"
)]
pub async fn submit_cmd(
    tournament_id: &str,
    round_id: &str,
    form: Form<EditDrawForm>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::id.eq(round_id))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return err_not_found(),
    };

    let cmd = match Cmd::parse(&form.cmd) {
        Ok(cmd) => cmd,
        Err(e) => {
            return bad_request(
                ErrorAlert {
                    msg: format!("Invalid command provided: {e}"),
                }
                .render(),
            );
        }
    };

    let (judge_no, debate_no, role) = match cmd {
        Cmd::Trainee(judge, debate) => (judge, debate, Role::Trainee),
        Cmd::Panelist(judge, debate) => (judge, debate, Role::Panelist),
        Cmd::Chair(judge, debate) => (judge, debate, Role::Chair),
    };

    let apply_move = apply_move(judge_no, debate_no, role, &round, &mut *conn);
    match apply_move {
        Ok(()) => success(
            maud! {
                p {
                    "Applied move."
                }
            }
            .render(),
        ),
        Err(e) => bad_request(
            ErrorAlert {
                msg: format!("Error evaluating command: {e}"),
            }
            .render(),
        ),
    }
}

fn apply_move(
    judge_no: u32,
    debate_no: Option<u32>,
    role: Role,
    round: &Round,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<(), String> {
    let existing_alloc =
        match tournament_judges::table
            .filter(tournament_judges::number.eq(judge_no as i64))
            .inner_join(tournament_debate_judges::table)
            .inner_join(tournament_debates::table.on(
                tournament_debate_judges::debate_id.eq(tournament_debates::id),
            ))
            .first::<(Judge, DebateJudge, Debate)>(conn)
            .optional()
        {
            Ok(Some(a)) => a,
            Ok(None) => {
                return Err(format!("No such judge with number {judge_no}"));
            }
            Err(e) => {
                return Err(format!("Invalid command: {e}"));
            }
        };

    let debate_to_alloc_to = if let Some(debate_no) = debate_no {
        match tournament_debates::table
            .filter(
                tournament_debates::round_id
                    .eq(&round.id)
                    .and(tournament_debates::number.eq(debate_no as i64)),
            )
            .first::<Debate>(conn)
            .optional()
            .unwrap()
        {
            Some(d) => Some(d),
            None => {
                return Err(format!(
                    "Debate with number {debate_no} does not exist."
                ));
            }
        }
    } else {
        None
    };

    let set = match debate_to_alloc_to {
        Some(debate) => (
            tournament_debate_judges::status.eq(role.to_string()),
            tournament_debate_judges::debate_id.eq(debate.id),
        ),
        None => (
            tournament_debate_judges::status.eq(role.to_string()),
            tournament_debate_judges::debate_id
                .eq(existing_alloc.1.debate_id.clone()),
        ),
    };

    let n = diesel::update(
        tournament_debate_judges::table.filter(
            tournament_debate_judges::debate_id
                .eq(&existing_alloc.1.debate_id)
                .and(
                    tournament_debate_judges::judge_id
                        .eq(&existing_alloc.1.judge_id),
                ),
        ),
    )
    .set(set)
    .execute(conn)
    .unwrap();
    assert_eq!(n, 1);

    Ok(())
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Role {
    Trainee,
    Panelist,
    Chair,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char(match self {
            Role::Trainee => 'T',
            Role::Panelist => 'P',
            Role::Chair => 'C',
        })
    }
}

impl Role {
    pub fn of_str(item: &str) -> Role {
        match item {
            "C" => Role::Chair,
            "P" => Role::Panelist,
            "T" => Role::Trainee,
            _ => unreachable!(
                "should not pass incorrect values to Role::from_str"
            ),
        }
    }
}

pub enum Cmd {
    Trainee(u32, Option<u32>),
    Panelist(u32, Option<u32>),
    Chair(u32, Option<u32>),
}

impl Cmd {
    pub fn parse(input: &str) -> Result<Self, String> {
        crate::cmd::CmdParser::new()
            .parse(input)
            .map_err(|e| e.to_string())
    }
}
