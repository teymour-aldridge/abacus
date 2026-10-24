use super::{forms, gen_private_url::get_unique_private_url};
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::judges,
    state::Conn,
    ui::response::{ModalQuery, after_edit},
    util_resp::StandardResponse,
};
use axum::extract::{Extension, Form, Path, Query};
use diesel::prelude::*;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
#[derive(Deserialize, Default)]
pub struct CreateJudgeForm {
    pub name: String,
    pub email: String,
    pub institution_id: String,
}
pub async fn create_judge_page(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    forms::render(
        query,
        "Create judge",
        &format!("/tournaments/{tid}/judges/create"),
        &t,
        user,
        "",
        Some(""),
        Some("-----"),
        None,
        None,
        &mut *conn,
    )
}
pub async fn do_create_judge(
    Path(tid): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateJudgeForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let inst = forms::institution(&tid, &form.institution_id, &mut *conn)?;
    if let Some(error) = forms::person_error(&form.name, &form.email) {
        return forms::render(
            query,
            "Create judge",
            &format!("/tournaments/{tid}/judges/create"),
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
    let number = judges::table
        .filter(judges::tournament_id.eq(&tid))
        .select(diesel::dsl::max(judges::number))
        .first::<Option<i64>>(&mut *conn)?
        .unwrap_or(0)
        + 1;
    let private_url = get_unique_private_url(&tid, &mut *conn);
    let result = diesel::insert_into(judges::table)
        .values((
            judges::id.eq(uuid::Uuid::now_v7().to_string()),
            judges::tournament_id.eq(&tid),
            judges::name.eq(&form.name),
            judges::email.eq(&form.email),
            judges::institution_id.eq(inst),
            judges::number.eq(number),
            judges::private_url.eq(private_url),
        ))
        .execute(&mut *conn);
    if let Err(diesel::result::Error::DatabaseError(
        diesel::result::DatabaseErrorKind::UniqueViolation,
        _,
    )) = &result
    {
        return forms::render(
            query,
            "Create judge",
            &format!("/tournaments/{tid}/judges/create"),
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
