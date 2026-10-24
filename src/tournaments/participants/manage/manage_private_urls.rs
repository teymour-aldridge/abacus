use crate::{
    auth::User,
    schema::{judges, speakers},
    state::Conn,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
};
use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

pub async fn view_private_urls(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let judges = judges::table
        .filter(judges::tournament_id.eq(&tid))
        .order(judges::name.asc())
        .select((judges::name, judges::email, judges::private_url))
        .load::<(String, String, String)>(&mut *conn)?;
    let speakers = speakers::table
        .filter(speakers::tournament_id.eq(&tid))
        .order(speakers::name.asc())
        .select((speakers::name, speakers::email, speakers::private_url))
        .load::<(String, String, String)>(&mut *conn)?;
    success(Page::new().tournament(tournament).user(user).active_nav(crate::template::ActiveNav::Participants).body(maud! {
        div class="container py-4" {
            h1 { "Participant private links" }
            p class="badge text-bg-secondary" { "Restricted: Tournament organisers. Private-link holders see only their own link." }
            div class="table-responsive" { table class="table private-urls-table" {
                thead { tr { th { "Name" } th { "Email" } th { "Role" } th { "Private URL" } } }
                tbody {
                    @for (name,email,token) in &judges { tr { th scope="row" { (name) } td { (email) } td { "Judge" } td { (token) } } }
                    @for (name,email,token) in &speakers { tr { th scope="row" { (name) } td { (email) } td { "Speaker" } td { (token) } } }
                }
            } }
            a href=(format!("/tournaments/{tid}/participants")) { "Participants" }
        }
    }).render())
}
