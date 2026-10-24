use super::forms;
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::teams,
    state::Conn,
    tournaments::{
        snapshots::take_snapshot,
        standings::compute::refresh_saved_team_standings,
    },
    ui::response::{ModalQuery, after_edit},
    util_resp::StandardResponse,
};
use axum::extract::{Extension, Form, Path, Query};
use diesel::prelude::*;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
#[derive(Deserialize, Default)]
pub struct CreateTeamForm {
    pub name: String,
    pub institution_id: String,
}
pub async fn create_teams_page(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    forms::render(
        query,
        "Create team",
        &format!("/tournaments/{tid}/teams/create"),
        &t,
        user,
        "",
        None,
        Some("-----"),
        None,
        None,
        &mut *conn,
    )
}
pub async fn do_create_team(
    Path(tid): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateTeamForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let inst = forms::institution(&tid, &form.institution_id, &mut *conn)?;
    let exists = forms::team_exists(
        &tid,
        &form.name,
        inst.as_deref(),
        None,
        &mut *conn,
    )?;
    if let Some(error) = forms::team_error(&form.name).or(if exists {
        Some("A team with that name already exists.")
    } else {
        None
    }) {
        return forms::render(
            query,
            "Create team",
            &format!("/tournaments/{tid}/teams/create"),
            &t,
            user,
            &form.name,
            None,
            Some(&form.institution_id),
            None,
            Some(error),
            &mut *conn,
        );
    }
    let number = teams::table
        .filter(teams::tournament_id.eq(&tid))
        .select(diesel::dsl::max(teams::number))
        .first::<Option<i64>>(&mut *conn)?
        .unwrap_or(0)
        + 1;
    diesel::insert_into(teams::table)
        .values((
            teams::id.eq(uuid::Uuid::now_v7().to_string()),
            teams::tournament_id.eq(&tid),
            teams::name.eq(&form.name),
            teams::institution_id.eq(inst),
            teams::number.eq(number),
        ))
        .execute(&mut *conn)?;
    refresh_saved_team_standings(&tid, &mut *conn)?;
    take_snapshot(&tid, &mut *conn);
    let _ = tx.send(Msg {
        tournament: t,
        inner: MsgContents::ParticipantsUpdate,
    });
    after_edit(query, format!("/tournaments/{tid}/participants"))
}
