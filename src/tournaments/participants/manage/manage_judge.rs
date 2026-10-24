use super::{create_judge::CreateJudgeForm, forms};
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::judges,
    state::Conn,
    tournaments::participants::Judge,
    ui::response::{ModalQuery, after_edit},
    util_resp::StandardResponse,
};
use axum::extract::{Extension, Form, Path, Query};
use diesel::prelude::*;
use tokio::sync::broadcast::Sender;
pub async fn edit_judge_details_page(
    Path((tid, id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let judge = judges::table
        .filter(judges::tournament_id.eq(&tid))
        .filter(judges::id.eq(&id))
        .first::<Judge>(&mut *conn)?;
    forms::render(
        query,
        "Edit judge",
        &format!("/tournaments/{tid}/judges/{id}/edit"),
        &t,
        user,
        &judge.name,
        Some(&judge.email),
        Some(judge.institution_id.as_deref().unwrap_or("-----")),
        None,
        None,
        &mut *conn,
    )
}
pub async fn do_edit_judge_details(
    Path((tid, id)): Path<(String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateJudgeForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let _judge = judges::table
        .filter(judges::tournament_id.eq(&tid))
        .filter(judges::id.eq(&id))
        .select(judges::id)
        .first::<String>(&mut *conn)?;
    let inst = forms::institution(&tid, &form.institution_id, &mut *conn)?;
    if let Some(error) = forms::person_error(&form.name, &form.email) {
        return forms::render(
            query,
            "Edit judge",
            &format!("/tournaments/{tid}/judges/{id}/edit"),
            &t,
            user,
            &form.name,
            Some(&form.email),
            Some(&form.institution_id),
            None,
            Some(error),
            &mut *conn,
        );
    }
    let result = diesel::update(
        judges::table
            .filter(judges::tournament_id.eq(&tid))
            .filter(judges::id.eq(&id)),
    )
    .set((
        judges::name.eq(&form.name),
        judges::email.eq(&form.email),
        judges::institution_id.eq(inst),
    ))
    .execute(&mut *conn);
    if let Err(diesel::result::Error::DatabaseError(
        diesel::result::DatabaseErrorKind::UniqueViolation,
        _,
    )) = &result
    {
        return forms::render(
            query,
            "Edit judge",
            &format!("/tournaments/{tid}/judges/{id}/edit"),
            &t,
            user,
            &form.name,
            Some(&form.email),
            Some(&form.institution_id),
            None,
            Some("A judge with that name already exists."),
            &mut *conn,
        );
    }
    result?;
    crate::tournaments::snapshots::take_snapshot(&tid, &mut *conn);
    let _ = tx.send(Msg {
        tournament: t,
        inner: MsgContents::ParticipantsUpdate,
    });
    after_edit(query, format!("/tournaments/{tid}/participants"))
}
