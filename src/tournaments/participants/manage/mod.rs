use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    permission::{Permission, TournamentAccess},
    schema::{institutions, judges, speakers, speakers_of_team, teams},
    state::{Conn, DbPool},
    template::Page,
    tournaments::Tournament,
    ui::{self, View},
    util_resp::{FailureResponse, StandardResponse, success},
};
use axum::{
    extract::{
        Extension, Path, Query,
        ws::{Message, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::{sync::broadcast::Sender, task::spawn_blocking};
pub mod constraints;
pub mod create_judge;
pub mod create_speaker;
pub mod create_team;
pub mod edit_speaker;
pub mod forms;
pub mod gen_private_url;
pub mod manage_judge;
pub mod manage_private_urls;
pub mod manage_team;

#[derive(Default)]
struct ParticipantDisplay {
    teams: Vec<TeamDisplay>,
    judges: Vec<JudgeDisplay>,
}
struct TeamDisplay {
    id: String,
    name: String,
    speakers: Vec<SpeakerDisplay>,
}
struct SpeakerDisplay {
    id: String,
    name: String,
    email: Option<String>,
}
struct JudgeDisplay {
    id: String,
    name: String,
    institution: String,
    email: Option<String>,
}

fn load(
    tid: &str,
    access: &TournamentAccess,
    conn: &mut diesel::SqliteConnection,
) -> Result<ParticipantDisplay, FailureResponse> {
    let private = access.allows(Permission::ManageParticipants);
    let teams = teams::table
        .left_join(
            institutions::table.on(institutions::id
                .nullable()
                .eq(teams::institution_id)
                .and(institutions::tournament_id.eq(tid))),
        )
        .filter(teams::tournament_id.eq(tid))
        .order(teams::number.asc())
        .select((teams::id, teams::name, institutions::code.nullable()))
        .load::<(String, String, Option<String>)>(conn)?;
    let mut display = ParticipantDisplay::default();
    for (id, name, code) in teams {
        let rows = speakers_of_team::table
            .inner_join(speakers::table)
            .filter(speakers_of_team::team_id.eq(&id))
            .filter(speakers::tournament_id.eq(tid))
            .order(speakers::name.asc());
        let speaker_rows = if private {
            rows.select((speakers::id, speakers::name, speakers::email))
                .load::<(String, String, String)>(conn)?
                .into_iter()
                .map(|(id, name, email)| SpeakerDisplay {
                    id,
                    name,
                    email: Some(email),
                })
                .collect()
        } else {
            rows.select((speakers::id, speakers::name))
                .load::<(String, String)>(conn)?
                .into_iter()
                .map(|(id, name)| SpeakerDisplay {
                    id,
                    name,
                    email: None,
                })
                .collect()
        };
        display.teams.push(TeamDisplay {
            id,
            name: code.map(|c| format!("{c} {name}")).unwrap_or(name),
            speakers: speaker_rows,
        });
    }
    let rows = judges::table
        .left_join(
            institutions::table.on(institutions::id
                .nullable()
                .eq(judges::institution_id)
                .and(institutions::tournament_id.eq(tid))),
        )
        .filter(judges::tournament_id.eq(tid))
        .order(judges::name.asc());
    display.judges = if private {
        rows.select((
            judges::id,
            judges::name,
            institutions::code.nullable(),
            judges::email,
        ))
        .load::<(String, String, Option<String>, String)>(conn)?
        .into_iter()
        .map(|(id, name, institution, email)| JudgeDisplay {
            id,
            name,
            institution: institution.unwrap_or_default(),
            email: Some(email),
        })
        .collect()
    } else {
        rows.select((judges::id, judges::name, institutions::code.nullable()))
            .load::<(String, String, Option<String>)>(conn)?
            .into_iter()
            .map(|(id, name, institution)| JudgeDisplay {
                id,
                name,
                institution: institution.unwrap_or_default(),
                email: None,
            })
            .collect()
    };
    Ok(display)
}
fn table<'a>(
    tid: &'a str,
    data: &'a ParticipantDisplay,
    access: &'a TournamentAccess,
) -> impl Renderable + 'a {
    maud! { (ui::html(|ctx: &TournamentAccess, out| {
        maud! {
            div id="participant-list" hx-swap-oob="outerHTML" {
                (ui::protected_or_empty(|c: &TournamentAccess| c.allows(Permission::ManageParticipants), || ui::html(|_: &TournamentAccess, out| {
                    maud! { p {
                        (ui::presentation::publication::<TournamentAccess>("Names and institutions",ui::presentation::Publication::Public).bind(ctx))
                        span class="ms-2" { "Email addresses " (ui::presentation::restricted::<TournamentAccess>("Participant managers").bind(ctx)) }
                    }
                        (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants), &format!("/tournaments/{tid}/teams/create"), "Add team", true).bind(ctx))
                        (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants), &format!("/tournaments/{tid}/judges/create"), "Add judge", true).bind(ctx))
                    }.render_to(out);
                })).bind(ctx))
                h2 { "Teams" }
                table class="table" { thead { tr { th { "Team" } th { "Speakers" } @if ctx.allows(Permission::ManageParticipants) { th { "Actions" } } } }
                    tbody { @for team in &data.teams { tr {
                        td { a href=(format!("/tournaments/{tid}/teams/{}",team.id)) { (team.name) } }
                        td { @for speaker in &team.speakers { div { (speaker.name)
                            @if let Some(email) = &speaker.email { span class="text-muted ms-2" { (email) }
                                (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants), &format!("/tournaments/{tid}/teams/{}/speakers/{}/edit",team.id,speaker.id),"Edit speaker",true).bind(ctx))
                            }
                            @if ctx.is_superuser() { (ui::presentation::action(|c: &TournamentAccess| c.is_superuser(), &format!("/tournaments/{tid}/participants/speaker/{}/constraints",speaker.id),"Room preferences",true).bind(ctx)) }
                        } } }
                        @if ctx.allows(Permission::ManageParticipants) { td {
                            (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants),&format!("/tournaments/{tid}/teams/{}/edit",team.id),"Edit",true).bind(ctx))
                            (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants),&format!("/tournaments/{tid}/teams/{}/speakers/create",team.id),"Add speaker",true).bind(ctx))
                        } }
                    } } }
                }
                h2 { "Judges" }
                table class="table" { thead { tr { th { "Name" } th { "Institution" } @if ctx.allows(Permission::ManageParticipants) { th { "Actions" } } } }
                    tbody { @for judge in &data.judges { tr {
                        td { (judge.name) @if let Some(email)=&judge.email { div class="text-muted" { (email) } } }
                        td { (judge.institution) }
                        @if ctx.allows(Permission::ManageParticipants) { td {
                            (ui::presentation::action(|c: &TournamentAccess| c.allows(Permission::ManageParticipants),&format!("/tournaments/{tid}/judges/{}/edit",judge.id),"Edit",true).bind(ctx))
                            @if ctx.is_superuser() { (ui::presentation::action(|c: &TournamentAccess| c.is_superuser(),&format!("/tournaments/{tid}/participants/judge/{}/constraints",judge.id),"Room preferences",true).bind(ctx)) }
                        } }
                    } } }
                }
            }
        }.render_to(out);
    }).bind(access)) }
}
#[derive(Deserialize)]
pub struct ManageParticipantsQuery {
    table_only: Option<bool>,
}
pub async fn manage_tournament_participants(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    Query(query): Query<ManageParticipantsQuery>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    let data = load(&tid, &access, &mut *conn)?;
    let tid = tid.as_str();
    let table = table(tid, &data, &access);
    if query.table_only.unwrap_or(false) {
        return success(table.render());
    }
    success(Page::new().user_opt(user).tournament(tournament).active_nav(crate::template::ActiveNav::Participants).body(maud! {
        div class="container py-4" hx-ext="ws" "ws-connect"=(format!("/tournaments/{tid}/participants/ws")) { h1 { "Participants" } (table) }
    }).render())
}
pub async fn tournament_participant_updates(
    ws: WebSocketUpgrade,
    Path(tid): Path<String>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    user: Option<User<false>>,
) -> axum::response::Response {
    let uid = user.map(|u| u.id);
    let check_pool = pool.clone();
    let check_tid = tid.clone();
    let check_uid = uid.clone();
    let check = spawn_blocking(move || {
        let mut conn = check_pool
            .get()
            .map_err(|_| FailureResponse::ServerError(()))?;
        Tournament::fetch(&check_tid, &mut *conn)?;
        TournamentAccess::resolve(&check_tid, check_uid.as_deref(), &mut *conn)
    })
    .await;
    let access = match check {
        Ok(Ok(access)) => access,
        Ok(Err(e)) => return e.into_response(),
        Err(_) => {
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR
                .into_response();
        }
    };
    let mut prior = (
        access.allows(Permission::ManageParticipants),
        access.is_superuser(),
    );
    ws.on_upgrade(move |mut socket| async move {
        let mut rx=tx.subscribe(); let mut tick=tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            tokio::select! {
                msg=socket.recv()=>{if !matches!(msg,Some(Ok(Message::Ping(_)|Message::Pong(_)|Message::Text(_)|Message::Binary(_)))) { break; } continue;},
                msg=rx.recv()=>match msg { Ok(msg) if msg.tournament.id==tid && matches!(msg.inner, MsgContents::ParticipantsUpdate)=>{}, Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{},Err(_)=>break,_=>continue },
                _=tick.tick()=>{}
            }
            let pool=pool.clone(); let tid=tid.clone(); let uid=uid.clone();
            let update=spawn_blocking(move || -> Result<(String,bool,bool), FailureResponse> { let mut conn=pool.get().map_err(|_|FailureResponse::ServerError(()))?; Tournament::fetch(&tid,&mut *conn)?; let access=TournamentAccess::resolve(&tid,uid.as_deref(),&mut *conn)?; let data=load(&tid,&access,&mut *conn)?; Ok((table(&tid,&data,&access).render().into_inner(),access.allows(Permission::ManageParticipants),access.is_superuser())) }).await;
            match update { Ok(Ok((mut html,manager,superuser)))=>{
                if (prior.0 && !manager) || (prior.1 && !superuser) { html.push_str("<div id=\"ui-modal\" hx-swap-oob=\"innerHTML\"></div>"); }
                prior=(manager,superuser);
                if socket.send(Message::Text(html)).await.is_err(){break;}
            }, _=>{let _=socket.send(Message::Text("<div id=\"participant-list\" hx-swap-oob=\"outerHTML\">Participant information is unavailable.</div><div id=\"ui-modal\" hx-swap-oob=\"innerHTML\"></div>".into())).await; break;} }
        }
    }).into_response()
}

pub async fn team_details(
    Path((tid, id)): Path<(String, String)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    let mut data = load(&tid, &access, &mut *conn)?;
    data.teams.retain(|team| team.id == id);
    data.judges.clear();
    if data.teams.is_empty() {
        return Err(FailureResponse::NotFound(()));
    }
    success(Page::new().user_opt(user).tournament(tournament).active_nav(crate::template::ActiveNav::Participants).body(maud!{div class="container py-4" {h1 {"Team details"} (table(&tid,&data,&access))}}).render())
}
