use rocket::get;

use crate::{
    auth::User,
    state::Conn,
    tournaments::{
        Tournament, manage::view::admin_view_tournament,
        public::view::public_tournament_page,
    },
    util_resp::StandardResponse,
};

#[get("/tournaments/<tournament_id>")]
pub async fn view_tournament_page(
    tournament_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;

    if let Some(user) = &user
        && tournament
            .check_user_is_superuser(&user.id, &mut *conn)
            .is_ok()
    {
        admin_view_tournament(tournament_id, user.clone(), conn).await
    } else {
        public_tournament_page(tournament_id, user, conn).await
    }
}
