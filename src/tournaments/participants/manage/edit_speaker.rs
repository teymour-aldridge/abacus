use super::{create_speaker::CreateSpeakerForm, forms};
use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        speaker_categories, speaker_category_memberships, speakers,
        speakers_of_team,
    },
    state::Conn,
    tournaments::participants::Speaker,
    ui::response::{ModalQuery, after_edit},
    util_resp::{FailureResponse, StandardResponse},
};
use axum::extract::{Extension, Path, Query};
use axum_extra::extract::Form;
use diesel::prelude::*;
use tokio::sync::broadcast::Sender;
fn load(
    tid: &str,
    team: &str,
    id: &str,
    conn: &mut SqliteConnection,
) -> Result<Speaker, FailureResponse> {
    Ok(speakers::table
        .inner_join(speakers_of_team::table)
        .filter(speakers::tournament_id.eq(tid))
        .filter(speakers::id.eq(id))
        .filter(speakers_of_team::team_id.eq(team))
        .filter(diesel::dsl::exists(
            crate::schema::teams::table
                .filter(crate::schema::teams::tournament_id.eq(tid))
                .filter(crate::schema::teams::id.eq(team)),
        ))
        .select(speakers::all_columns)
        .first::<Speaker>(conn)?)
}
pub async fn edit_speaker_page(
    Path((tid, team, id)): Path<(String, String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let speaker = load(&tid, &team, &id, &mut *conn)?;
    let categories = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .order(speaker_categories::seq.asc())
        .select((speaker_categories::id, speaker_categories::name))
        .load::<(String, String)>(&mut *conn)?;
    let selected = speaker_category_memberships::table
        .filter(speaker_category_memberships::tournament_id.eq(&tid))
        .filter(speaker_category_memberships::speaker_id.eq(&id))
        .select(speaker_category_memberships::category_id)
        .load::<String>(&mut *conn)?;
    forms::render(
        query,
        "Edit speaker",
        &format!("/tournaments/{tid}/teams/{team}/speakers/{id}/edit"),
        &t,
        user,
        &speaker.name,
        Some(&speaker.email),
        None,
        Some((&categories, &selected)),
        None,
        &mut *conn,
    )
}
pub async fn do_edit_speaker(
    Path((tid, team, id)): Path<(String, String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    Form(form): Form<CreateSpeakerForm>,
) -> StandardResponse {
    let t = forms::authorize(&tid, &user.id, &mut *conn)?;
    let _speaker = load(&tid, &team, &id, &mut *conn)?;
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
    let duplicate = diesel::select(diesel::dsl::exists(
        speakers::table
            .filter(speakers::id.ne(&id))
            .filter(speakers::name.eq(&form.name)),
    ))
    .get_result::<bool>(&mut *conn)?;
    if let Some(error) =
        forms::person_error(&form.name, &form.email).or(if duplicate {
            Some("That speaker name is unavailable.")
        } else {
            None
        })
    {
        return forms::render(
            query,
            "Edit speaker",
            &format!("/tournaments/{tid}/teams/{team}/speakers/{id}/edit"),
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
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        diesel::update(
            speakers::table
                .filter(speakers::tournament_id.eq(&tid))
                .filter(speakers::id.eq(&id)),
        )
        .set((
            speakers::name.eq(&form.name),
            speakers::email.eq(&form.email),
        ))
        .execute(conn)?;
        diesel::delete(
            speaker_category_memberships::table
                .filter(speaker_category_memberships::tournament_id.eq(&tid))
                .filter(speaker_category_memberships::speaker_id.eq(&id)),
        )
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
    })?;
    crate::tournaments::categories::speaker::recompute_break_eligibility(
        &tid, &mut *conn,
    );
    crate::tournaments::snapshots::take_snapshot(&tid, &mut *conn);
    let _ = tx.send(Msg {
        tournament: t,
        inner: MsgContents::ParticipantsUpdate,
    });
    after_edit(query, format!("/tournaments/{tid}/teams/{team}"))
}
