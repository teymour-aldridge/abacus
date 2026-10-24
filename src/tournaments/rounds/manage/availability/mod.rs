pub mod judges;
pub mod teams;
use hypertext::prelude::*;

fn removal(message: &str) -> String {
    maud! {
        @for id in ["availability-teams-read", "availability-judges-read"] {
            table id=(id) hx-swap-oob="outerHTML" { tbody { tr { td { (message) } } } }
        }
        @for id in ["availability-teams-editor", "availability-judges-editor"] {
            div id=(id) hx-swap-oob="outerHTML" { (message) }
        }
        div id="ui-modal" hx-swap-oob="innerHTML" {}
    }.render().into_inner()
}
async fn stream_access(
    pool: &crate::state::DbPool,
    tid: &str,
    viewer: &str,
) -> Result<bool, crate::util_resp::FailureResponse> {
    let pool = pool.clone();
    let tid = tid.to_owned();
    let viewer = viewer.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut conn = pool
            .get()
            .map_err(|_| crate::util_resp::FailureResponse::ServerError(()))?;
        crate::permission::TournamentAccess::resolve(
            &tid,
            Some(&viewer),
            &mut conn,
        )
        .map(|access| access.is_superuser())
    })
    .await
    .map_err(|_| crate::util_resp::FailureResponse::ServerError(()))?
}
fn stream_removal(
    access: Result<bool, crate::util_resp::FailureResponse>,
) -> Option<String> {
    match access {
        Ok(true) => None,
        Ok(false) => Some(removal(
            "Access to availability has ended. Refresh this page.",
        )),
        Err(error) => {
            tracing::error!(
                ?error,
                "Availability stream permission resolution failed"
            );
            Some(removal(
                "Availability is temporarily unavailable. Refresh this page.",
            ))
        }
    }
}
fn availability_error(
    query: crate::ui::response::ModalQuery,
    tournament: &crate::tournaments::Tournament,
    user: crate::auth::User<true>,
    seq: i64,
    family: &str,
    action: &str,
    form: &impl serde::Serialize,
    message: &str,
) -> crate::util_resp::StandardResponse {
    if let Some((base, _)) = action.split_once("/all?") {
        let return_url = format!(
            "/tournaments/{}/rounds/{seq}/availability/{family}",
            tournament.id
        );
        return crate::tournaments::manage::workspace::editor_page(
            query,
            "Availability",
            &return_url,
            tournament,
            user,
            maud! {
                div class="alert alert-danger" role="alert" { (message) }
                @for (check,label) in [("in","Check everyone in"),("out","Check everyone out")] {
                    form method="post" action=(format!("{base}/all?check={check}")) hx-post=(format!("{base}/all?check={check}&modal=true")) hx-target="#ui-modal" {
                        button class="btn btn-outline-primary" type="submit" { (label) }
                    }
                }
            },
            true,
        );
    }
    crate::tournaments::manage::workspace::invalid_form(
        query,
        tournament,
        user,
        action,
        &format!(
            "/tournaments/{}/rounds/{seq}/availability/{family}",
            tournament.id
        ),
        form,
        &[],
        message,
    )
}
