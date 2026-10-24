pub use super::team_details as manage_team_page;
use super::{create_team::CreateTeamForm, forms};
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::teams,
    state::Conn,
    tournaments::{
        snapshots::take_snapshot,
        standings::compute::refresh_saved_team_standings, teams::Team,
    },
    ui::response::{ModalQuery, after_edit},
    util_resp::StandardResponse,
};
use axum::extract::{Extension, Form, Path, Query};
use diesel::prelude::*;
use tokio::sync::broadcast::Sender;
pub async fn edit_team_details_page(
    Path((tid, id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let team = Team::fetch(&id, &tid, &mut *conn)?;
    forms::render(
        query,
        "Edit team",
        &format!("/tournaments/{tid}/teams/{id}/edit"),
        &t,
        user,
        &team.name,
        None,
        Some(team.institution_id.as_deref().unwrap_or("-----")),
        None,
        None,
        &mut *conn,
    )
}
pub async fn do_edit_team_details(
    Path((tid, id)): Path<(String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateTeamForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let _team = Team::fetch(&id, &tid, &mut *conn)?;
    let inst = forms::institution(&tid, &form.institution_id, &mut *conn)?;
    let exists = forms::team_exists(
        &tid,
        &form.name,
        inst.as_deref(),
        Some(&id),
        &mut *conn,
    )?;
    if let Some(error) = forms::team_error(&form.name).or(if exists {
        Some("A team with that name already exists.")
    } else {
        None
    }) {
        return forms::render(
            query,
            "Edit team",
            &format!("/tournaments/{tid}/teams/{id}/edit"),
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
    diesel::update(
        teams::table
            .filter(teams::tournament_id.eq(&tid))
            .filter(teams::id.eq(&id)),
    )
    .set((teams::name.eq(&form.name), teams::institution_id.eq(inst)))
    .execute(&mut *conn)?;
    refresh_saved_team_standings(&tid, &mut *conn)?;
    take_snapshot(&tid, &mut *conn);
    let _ = tx.send(Msg {
        tournament: t,
        inner: MsgContents::ParticipantsUpdate,
    });
    after_edit(query, format!("/tournaments/{tid}/teams/{id}"))
}
