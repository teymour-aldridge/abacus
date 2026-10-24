use super::{forms, gen_private_url::get_unique_private_url};
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        speaker_categories, speaker_category_memberships, speakers,
        speakers_of_team,
    },
    state::Conn,
    tournaments::teams::Team,
    ui::response::{ModalQuery, after_edit},
    util_resp::{FailureResponse, StandardResponse},
};
use axum::extract::{Extension, Path, Query};
use axum_extra::extract::Form;
use diesel::prelude::*;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
#[derive(Deserialize, Default)]
pub struct CreateSpeakerForm {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub category_ids: Vec<String>,
}
pub async fn create_speaker_page(
    Path((tid, team_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let _team = Team::fetch(&team_id, &tid, &mut *conn)?;
    let categories = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .order(speaker_categories::seq.asc())
        .select((speaker_categories::id, speaker_categories::name))
        .load::<(String, String)>(&mut *conn)?;
    forms::render(
        query,
        "Add speaker",
        &format!("/tournaments/{tid}/teams/{team_id}/speakers/create"),
        &t,
        user,
        "",
        Some(""),
        None,
        Some((&categories, &[])),
        None,
        &mut *conn,
    )
}
pub async fn do_create_speaker(
    Path((tid, team_id)): Path<(String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateSpeakerForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let _team = Team::fetch(&team_id, &tid, &mut *conn)?;
    let categories = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .order(speaker_categories::seq.asc())
        .select((speaker_categories::id, speaker_categories::name))
        .load::<(String, String)>(&mut *conn)?;
    if form
        .category_ids
        .iter()
        .any(|id| !categories.iter().any(|(valid, _)| valid == id))
    {
        return Err(FailureResponse::NotFound(()));
    }
    if let Some(error) = forms::person_error(&form.name, &form.email) {
        return forms::render(
            query,
            "Add speaker",
            &format!("/tournaments/{tid}/teams/{team_id}/speakers/create"),
            &t,
            user,
            &form.name,
            Some(&form.email),
            None,
            Some((&categories, &form.category_ids)),
            Some(error),
            &mut *conn,
        );
    }
    let private_url = get_unique_private_url(&tid, &mut *conn);
    let id = uuid::Uuid::now_v7().to_string();
    let result = conn.transaction::<_, diesel::result::Error, _>(|conn| {
        diesel::insert_into(speakers::table)
            .values((
                speakers::id.eq(&id),
                speakers::tournament_id.eq(&tid),
                speakers::name.eq(&form.name),
                speakers::email.eq(&form.email),
                speakers::private_url.eq(private_url),
            ))
            .execute(conn)?;
        diesel::insert_into(speakers_of_team::table)
            .values((
                speakers_of_team::id.eq(uuid::Uuid::now_v7().to_string()),
                speakers_of_team::team_id.eq(&team_id),
                speakers_of_team::speaker_id.eq(&id),
            ))
            .execute(conn)?;
        let ids: std::collections::HashSet<_> =
            form.category_ids.iter().collect();
        for category_id in ids {
            diesel::insert_into(speaker_category_memberships::table)
                .values((
                    speaker_category_memberships::id
                        .eq(uuid::Uuid::now_v7().to_string()),
                    speaker_category_memberships::tournament_id.eq(&tid),
                    speaker_category_memberships::speaker_id.eq(&id),
                    speaker_category_memberships::category_id.eq(category_id),
                ))
                .execute(conn)?;
        }
        Ok(())
    });
    if let Err(diesel::result::Error::DatabaseError(
        diesel::result::DatabaseErrorKind::UniqueViolation,
        _,
    )) = &result
    {
        return forms::render(
            query,
            "Add speaker",
            &format!("/tournaments/{tid}/teams/{team_id}/speakers/create"),
            &t,
            user,
            &form.name,
            Some(&form.email),
            None,
            Some((&categories, &form.category_ids)),
            Some("A speaker with that name already exists."),
            &mut *conn,
        );
    }
    result?;
    crate::tournaments::categories::speaker::recompute_break_eligibility(
        &tid, &mut *conn,
    );
    crate::tournaments::snapshots::take_snapshot(&tid, &mut *conn);
    let _ = tx.send(Msg {
        tournament: t,
        inner: MsgContents::ParticipantsUpdate,
    });
    after_edit(query, format!("/tournaments/{tid}/teams/{team_id}"))
}
