use std::{collections::HashMap, fmt::Write};

use crate::{
    msg::{Msg, MsgContents},
    permission::IsTabDirectorNoTx,
    schema::{tournament_debate_judges, tournament_debates, tournament_judges},
    state::DbPool,
    tournaments::{
        TournamentNoTx, WEBSOCKET_SCHEME,
        participants::{DebateJudge, Judge},
        rounds::draws::Debate,
    },
    widgets::alert::ErrorAlert,
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use lalrpop_util::lalrpop_mod;
use rocket::{
    FromForm, Responder, State, form::Form, futures::SinkExt, get, post,
};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::{tournament_draws, tournament_rounds, tournament_teams},
    state::LockedConn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::{DebateRepr, Draw, DrawRepr, manage::DrawTableRenderer},
        },
        teams::Team,
    },
};

#[get(
    "/touraments/<tournament_id>/rounds/<round_id>/draws/<draw_id>/edit?<table_only>"
)]
pub async fn edit_draw_page_tab_dir(
    tournament_id: &str,
    round_id: &str,
    draw_id: &str,
    tournament: Tournament,
    _dir: IsTabDirector,
    user: User,
    mut conn: LockedConn<'_>,
    table_only: bool,
) -> Option<Rendered<String>> {
    let (draw, round) = match tournament_draws::table
        .filter(
            tournament_draws::tournament_id
                .eq(&tournament_id)
                .and(tournament_draws::id.eq(draw_id)),
        )
        .inner_join(tournament_rounds::table)
        .filter(tournament_rounds::id.eq(&round_id))
        .first::<(Draw, Round)>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return None,
    };

    let repr = DrawRepr::of_draw(draw, &mut *conn);

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(tournament_id))
        .load::<Team>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect();

    let table = DrawTableRenderer {
        tournament: &tournament,
        repr: &repr,
        actions: |_: &DebateRepr| maud! {"None"},
        teams: &teams,
    };

    if table_only {
        Some(table.render())
    } else {
        Some(
            Page::new()
                .tournament(tournament.clone())
                .user(user)
                .hx_ext("ws")
                .body(maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-response-targets@2.0.2" integrity="sha384-T41oglUPvXLGBVyRdZsVRxNWnOOqCynaPubjUVjxhsjFTKrFJGEMm3/0KGmNQ+Pg" crossorigin="anonymous" {
                    }

                    h1 {
                        "Edit draw for " (round.name)
                    }

                    div id="tableContainer"
                        hx-get = (format!("/tournaments/{tournament_id}/rounds/<round_id>/draws/<draw_id>/edit?tableonly=1"))
                        hx-trigger = "refreshDraw"
                    {
                        (table)
                    }

                    div id="cmdBar" {
                        div id = "cmdErrMsg" {}
                        form hx-post=(format!("/tournaments/{tournament_id}/rounds/<round_id>/draws/<draw_id>/edit"))
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
                        }
                    }

                    script {
                        (format!(r#"
                            const ws = new WebSocket(`{WEBSOCKET_SCHEME}${{window.location.host}}/tournaments/{tournament_id}/rounds/{round_id}/edit?channel`);

                            socket.onmessage = function(event) {{
                                htmx.trigger('#tableContainer', 'refreshDraw');
                            }};
                            "#))
                    }
                })
                .render(),
        )
    }
}

#[get("/tournaments/<tournament_id>/rounds/<round_id>/draws/<draw_id>?channel")]
/// Provides a WebSocket channel which notifies clients when the draw has been
/// updated. After receiving this message, the client should then reload the
/// draw (using [`edit_draw_page_tab_dir`], with the `table_only` flag set to
/// true).
pub async fn draw_updates(
    tournament_id: &str,
    round_id: &str,
    draw_id: &str,
    _dir: IsTabDirectorNoTx,
    tournament: TournamentNoTx,
    pool: &State<DbPool>,
    rx: &State<Receiver<Msg>>,
    ws: rocket_ws::WebSocket,
) -> Option<rocket_ws::Channel<'static>> {
    let pool: DbPool = pool.inner().clone();

    let draw_id = draw_id.to_string();
    let round_id = round_id.to_string();
    let tournament_id = tournament_id.to_string();

    let pool1 = pool.clone();
    let (_round, draw) = match spawn_blocking(move || {
        let mut conn = pool1.get().unwrap();
        tournament_draws::table
            .filter(
                tournament_draws::tournament_id
                    .eq(&tournament_id)
                    .and(tournament_draws::id.eq(draw_id)),
            )
            .inner_join(tournament_rounds::table)
            .filter(tournament_rounds::id.eq(&round_id))
            .first::<(Draw, Round)>(&mut conn)
            .optional()
            .unwrap().map(|t| t)
    })
    .await
    .unwrap()
    {
        Some(t) => t,
        None => return None,
    };

    let mut rx: Receiver<Msg> = rx.inner().resubscribe();

    Some(ws.channel(move |mut stream| {
        Box::pin(async move {
            loop {
                let msg = rx.recv().await.unwrap();

                if msg.tournament.id == tournament.0.id
                    && let MsgContents::DrawUpdated(draw_id) = msg.inner
                    && draw_id == draw.id
                {
                    let _ =
                        stream.send(rocket_ws::Message::Text(draw_id)).await;
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
    "/touraments/<tournament_id>/rounds/<round_id>/draws/<draw_id>/edit",
    data = "<form>"
)]
pub async fn submit_cmd_tab_dir<'r>(
    tournament_id: &str,
    round_id: &str,
    draw_id: &str,
    tournament: Tournament,
    _dir: IsTabDirector,
    mut conn: LockedConn<'r>,
    form: Form<EditDrawForm>,
) -> FallibleResponse {
    let (draw, _round) = match tournament_draws::table
        .filter(
            tournament_draws::tournament_id
                .eq(&tournament_id)
                .and(tournament_draws::id.eq(draw_id)),
        )
        .inner_join(tournament_rounds::table)
        .filter(tournament_rounds::id.eq(&round_id))
        .first::<(Draw, Round)>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return FallibleResponse::NotFound(()),
    };

    let cmd = match Cmd::parse(&form.cmd) {
        Ok(cmd) => cmd,
        Err(e) => {
            return FallibleResponse::BadReq(
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

    let apply_move = apply_move(judge_no, debate_no, role, &draw, &mut *conn);
    match apply_move {
        Ok(()) => {
            FallibleResponse::Ok({
                let repr = DrawRepr::of_draw(draw, &mut *conn);
                let teams = tournament_teams::table
                    .filter(tournament_teams::tournament_id.eq(&tournament.id))
                    .load::<Team>(&mut *conn)
                    .unwrap()
                    .into_iter()
                    .map(|t| (t.id.clone(), t))
                    .collect::<HashMap<_, _>>();
                let table = DrawTableRenderer {
                    tournament: &tournament,
                    repr: &repr,
                    actions: |_: &DebateRepr| maud! {"None"},
                    teams: &teams,
                };
                table.render()
            })
        }
        Err(e) => {
            FallibleResponse::BadReq(
                ErrorAlert {
                    msg: format!("Error evaluating command: {e}"),
                }
                .render(),
            )
        }
    }
}

fn apply_move(
    judge_no: u32,
    debate_no: Option<u32>,
    role: Role,
    draw: &Draw,
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
                tournament_debates::draw_id
                    .eq(&draw.id)
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
    pub fn from_str(item: &str) -> Role {
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

lalrpop_mod!(grammar, "/tournaments/rounds/draws/manage/cmd.rs");

impl Cmd {
    pub fn parse(input: &str) -> Result<Self, String> {
        grammar::CmdParser::new()
            .parse(input)
            .map_err(|e| e.to_string())
    }
}
