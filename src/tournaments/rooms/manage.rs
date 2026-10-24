use crate::{
    auth::User,
    schema::{
        debates, judge_room_constraints, room_categories, rooms,
        rooms_of_category, speaker_room_constraints,
    },
    state::Conn,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rooms::{Room, RoomCategory},
        rounds::TournamentRounds,
    },
    util_resp::{FailureResponse, StandardResponse},
};
use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, serde::Serialize)]
pub struct CreateRoomForm {
    name: String,
    priority: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct CreateCategoryForm {
    private_name: String,
    public_name: String,
    description: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct AddRoomToCategoryForm {
    room_id: String,
}

use std::collections::HashMap;

pub async fn manage_rooms_page(
    Path(tid): Path<String>,
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = crate::permission::TournamentAccess::resolve(
        &tournament.id,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    if !access.is_superuser() {
        if query.editing() {
            return Err(crate::util_resp::FailureResponse::Unauthorized(()));
        }
        return crate::tournaments::manage::workspace::unavailable(
            &tournament,
            user,
            "Rooms",
        );
    }
    let user = user.expect("superuser is authenticated");
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    let rooms = rooms::table
        .filter(rooms::tournament_id.eq(&tid))
        .order_by(rooms::priority.asc())
        .load::<Room>(&mut *conn)
        .unwrap();

    let categories = room_categories::table
        .filter(room_categories::tournament_id.eq(&tid))
        .load::<RoomCategory>(&mut *conn)
        .unwrap();

    use crate::tournaments::rooms::RoomsOfRoomCategory;
    let category_ids: Vec<String> =
        categories.iter().map(|c| c.id.clone()).collect();

    let links = rooms_of_category::table
        .filter(rooms_of_category::category_id.eq_any(&category_ids))
        .load::<RoomsOfRoomCategory>(&mut *conn)
        .unwrap();

    // Map CategoryID -> Vec<Room>
    let mut rooms_by_category: HashMap<String, Vec<Room>> = HashMap::new();

    // Quick lookup for rooms
    let room_map: HashMap<String, Room> =
        rooms.iter().map(|r| (r.id.clone(), r.clone())).collect();

    for link in links {
        if let Some(room) = room_map.get(&link.room_id) {
            rooms_by_category
                .entry(link.category_id)
                .or_default()
                .push(room.clone());
        }
    }

    let canonical = format!("/tournaments/{tid}/rooms");
    if !query.editing() {
        return crate::tournaments::manage::workspace::read_page(
            &tournament,
            user,
            "Rooms",
            &canonical,
            &access,
            maud! { table class="table" { thead { tr { th { "Priority" } th { "Room" } } } tbody { @for room in &rooms { tr { td { (room.priority) } td { (room.name) } } } } } h2 { "Room categories" } @for category in &categories { h3 { (category.private_name) } p { (category.public_name) " — " (category.description) } @if let Some(assigned) = rooms_by_category.get(&category.id) { ul { @for room in assigned { li { (room.name) } } } } } },
        );
    }
    crate::tournaments::manage::workspace::editor_page(
        query.form(),
        "Rooms",
        &canonical,
        &tournament.clone(),
        user,
        maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(current_rounds.first().map(|r| r.seq)) active_page=(None) {
                div class="d-flex flex-column gap-5" {
                    // Rooms Section
                    div {
                        div class="d-flex justify-content-between align-items-center mb-3" {
                            h2 class="h4 fw-bold mb-0" { "Rooms" }
                        }

                        div class="card mb-4" {
                            div class="card-body bg-light" {
                                (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/rooms/create", tournament.id), crate::tournaments::manage::workspace::fields(&CreateRoomForm { name:String::new(),priority:"10".into() }, &[]), None))
                            }
                        }

                        div class="table-responsive border rounded" {
                            table class="table table-hover mb-0" {
                                thead class="bg-light" {
                                    tr {
                                        th { "Priority" }
                                        th { "Name" }
                                        th class="text-end" { "Actions" }
                                    }
                                }
                                tbody {
                                    @for room in &rooms {
                                        tr {
                                            td { (room.priority) }
                                            td class="fw-medium" { (room.name) }
                                            td class="text-end" {
                                                form action=(format!("/tournaments/{}/rooms/{}/delete", tournament.id, room.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rooms/{}/delete", tournament.id, room.id), if format!("/tournaments/{}/rooms/{}/delete", tournament.id, room.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post"
                                                    onsubmit="return confirm('Are you sure? This cannot be undone.');" {
                                                    button type="submit" class="btn btn-sm btn-link text-danger text-decoration-none" { "Delete" }
                                                }
                                            }
                                        }
                                    }
                                    @if rooms.is_empty() {
                                        tr {
                                            td colspan="3" class="text-center text-muted py-4" { "No rooms created yet." }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Categories Section
                    div {
                        div class="d-flex justify-content-between align-items-center mb-3" {
                            h2 class="h4 fw-bold mb-0" { "Room Categories" }
                        }

                        div class="card mb-4" {
                            div class="card-body bg-light" {
                                (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/rooms/categories/create", tournament.id), crate::tournaments::manage::workspace::fields(&CreateCategoryForm { private_name:String::new(),public_name:String::new(),description:String::new() }, &[]), None))
                            }
                        }

                        div class="list-group" {
                            @for category in &categories {
                                div class="list-group-item p-4" {
                                    div class="d-flex justify-content-between align-items-start mb-3" {
                                        div {
                                            h5 class="mb-1 fw-bold" {
                                                (category.private_name)
                                                span class="text-muted fw-normal ms-2 fs-6" { "(" (category.public_name) ")" }
                                            }
                                            p class="mb-0 text-muted small" { (category.description) }
                                        }
                                        form action=(format!("/tournaments/{}/rooms/categories/{}/delete", tournament.id, category.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rooms/categories/{}/delete", tournament.id, category.id), if format!("/tournaments/{}/rooms/categories/{}/delete", tournament.id, category.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post"
                                            onsubmit="return confirm('Delete this category? Assigned rooms will remain.');" {
                                            button type="submit" class="btn btn-sm btn-outline-danger" { "Delete" }
                                        }
                                    }

                                    // Rooms in category
                                    div class="card" {
                                        div class="card-body bg-light p-3" {
                                            h6 class="card-title text-uppercase small fw-bold text-muted mb-3" { "Assigned Rooms" }

                                            // Fetch rooms in this category
                                            @let rooms_in_cat = rooms_by_category.get(&category.id).map(|v| v.as_slice()).unwrap_or(&[]);

                                            div class="d-flex flex-wrap gap-2 mb-3" {
                                                @for room in rooms_in_cat {
                                                    div class="badge bg-white text-dark border px-2 py-2 d-flex align-items-center gap-2" {
                                                        (room.name)
                                                        form action=(format!("/tournaments/{}/rooms/categories/{}/remove_room", tournament.id, category.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rooms/categories/{}/remove_room", tournament.id, category.id), if format!("/tournaments/{}/rooms/categories/{}/remove_room", tournament.id, category.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post" class="m-0 p-0 d-flex" {
                                                            input type="hidden" name="room_id" value=(room.id);
                                                            button type="submit" class="btn-close" style="width: 0.5em; height: 0.5em;" aria-label="Remove" {}
                                                        }
                                                    }
                                                }
                                                @if rooms_in_cat.is_empty() {
                                                    span class="text-muted small fst-italic" { "No rooms assigned" }
                                                }
                                            }

                                            form action=(format!("/tournaments/{}/rooms/categories/{}/add_room", tournament.id, category.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rooms/categories/{}/add_room", tournament.id, category.id), if format!("/tournaments/{}/rooms/categories/{}/add_room", tournament.id, category.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post" class="d-flex gap-2" {
                                                select class="form-select form-select-sm" name="room_id" required style="max_width: 300px;" {
                                                    option value="" selected disabled { "Add room to category..." }
                                                    @for room in &rooms {
                                                        // Avoid suggesting rooms already in category? Optional polish.
                                                        option value=(room.id) { (room.name) }
                                                    }
                                                }
                                                button type="submit" class="btn btn-sm btn-outline-primary" { "Add" }
                                            }
                                        }
                                    }
                                }
                            }
                            @if categories.is_empty() {
                                div class="list-group-item text-center text-muted py-5" { "No categories defined." }
                            }
                        }
                    }
                }
            }
        },
        false,
    )
}

pub async fn create_room(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateRoomForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;

    // Check permission
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let priority = form.priority.parse::<i64>().ok().filter(|p| *p >= 0);
    if priority.is_none() || form.name.trim().is_empty() {
        return invalid_room(
            modal_query,
            &tid,
            &tournament,
            user,
            &form,
            "Enter a room name and a non-negative integer priority.",
        );
    }
    let duplicate_name = diesel::dsl::select(diesel::dsl::exists(
        rooms::table.filter(
            rooms::tournament_id
                .eq(&tournament.id)
                .and(rooms::name.eq(&form.name)),
        ),
    ))
    .get_result::<bool>(&mut *conn)
    .map_err(FailureResponse::from)?;
    if duplicate_name {
        return invalid_room(
            modal_query,
            &tid,
            &tournament,
            user,
            &form,
            "A room with this name already exists.",
        );
    }

    let new_room = Room {
        id: uuid::Uuid::now_v7().to_string(),
        tournament_id: tid.clone(),
        name: form.name,
        url: None,
        priority: priority.unwrap(),
        number: rooms::table
            .filter(rooms::tournament_id.eq(&tournament.id))
            .order_by(rooms::number.desc())
            .select(rooms::number)
            .first(&mut *conn)
            .optional()
            .unwrap()
            .map(|x: i64| x + 1)
            .unwrap_or(0),
    };

    diesel::insert_into(rooms::table)
        .values(&new_room)
        .execute(&mut *conn)
        .map_err(FailureResponse::from)?;

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

pub async fn delete_room(
    Path((tid, room_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    rooms::table
        .filter(rooms::id.eq(&room_id))
        .filter(rooms::tournament_id.eq(&tid))
        .select(rooms::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    diesel::update(
        debates::table.filter(
            debates::tournament_id
                .eq(&tid)
                .and(debates::room_id.eq(Some(room_id.clone()))),
        ),
    )
    .set(debates::room_id.eq(None::<String>))
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    diesel::delete(
        rooms_of_category::table
            .filter(rooms_of_category::room_id.eq(&room_id)),
    )
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    diesel::delete(rooms::table.filter(rooms::id.eq(room_id)))
        .execute(&mut *conn)
        .map_err(FailureResponse::from)?;

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

pub async fn create_category(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    if form.private_name.trim().is_empty() || form.public_name.trim().is_empty()
    {
        return crate::tournaments::manage::workspace::invalid_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/rooms/categories/create"),
            &format!("/tournaments/{tid}/rooms"),
            &form,
            &[],
            "Enter an internal name and a public name.",
        );
    }
    let new_cat = RoomCategory {
        id: uuid::Uuid::new_v4().to_string(),
        tournament_id: tid.clone(),
        private_name: form.private_name,
        public_name: form.public_name,
        description: form.description,
    };

    diesel::insert_into(room_categories::table)
        .values(&new_cat)
        .execute(&mut *conn)
        .map_err(FailureResponse::from)?;

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

pub async fn delete_category(
    Path((tid, cat_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    room_categories::table
        .filter(room_categories::id.eq(&cat_id))
        .filter(room_categories::tournament_id.eq(&tid))
        .select(room_categories::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    conn.transaction(|conn| {
        diesel::delete(
            speaker_room_constraints::table
                .filter(speaker_room_constraints::category_id.eq(&cat_id)),
        )
        .execute(conn)?;
        diesel::delete(
            judge_room_constraints::table
                .filter(judge_room_constraints::category_id.eq(&cat_id)),
        )
        .execute(conn)?;
        diesel::delete(
            rooms_of_category::table
                .filter(rooms_of_category::category_id.eq(&cat_id)),
        )
        .execute(conn)?;
        diesel::delete(
            room_categories::table.filter(room_categories::id.eq(&cat_id)),
        )
        .execute(conn)?;
        Ok::<_, diesel::result::Error>(())
    })
    .map_err(FailureResponse::from)?;

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

pub async fn add_room_to_category(
    Path((tid, cat_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddRoomToCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let category_exists = room_categories::table
        .filter(room_categories::id.eq(&cat_id))
        .filter(room_categories::tournament_id.eq(&tid))
        .select(room_categories::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .is_some();
    let room_exists = rooms::table
        .filter(rooms::id.eq(&form.room_id))
        .filter(rooms::tournament_id.eq(&tid))
        .select(rooms::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .is_some();
    if !category_exists || !room_exists {
        return Err(FailureResponse::NotFound(()));
    }

    use crate::tournaments::rooms::RoomsOfRoomCategory;

    // Check if relation already exists to avoid unique constraint error
    let exists = rooms_of_category::table
        .filter(rooms_of_category::category_id.eq(&cat_id))
        .filter(rooms_of_category::room_id.eq(&form.room_id))
        .first::<RoomsOfRoomCategory>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?;

    if exists.is_none() {
        let new_rel = RoomsOfRoomCategory {
            id: uuid::Uuid::new_v4().to_string(),
            category_id: cat_id,
            room_id: form.room_id,
        };

        diesel::insert_into(rooms_of_category::table)
            .values(&new_rel)
            .execute(&mut *conn)
            .map_err(FailureResponse::from)?;
    }

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

pub async fn remove_room_from_category(
    Path((tid, cat_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddRoomToCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let category_exists = room_categories::table
        .filter(room_categories::id.eq(&cat_id))
        .filter(room_categories::tournament_id.eq(&tid))
        .select(room_categories::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .is_some();
    let room_exists = rooms::table
        .filter(rooms::id.eq(&form.room_id))
        .filter(rooms::tournament_id.eq(&tid))
        .select(rooms::id)
        .first::<String>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?
        .is_some();
    if !category_exists || !room_exists {
        return Err(FailureResponse::NotFound(()));
    }

    diesel::delete(
        rooms_of_category::table
            .filter(rooms_of_category::category_id.eq(&cat_id))
            .filter(rooms_of_category::room_id.eq(&form.room_id)),
    )
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{}/rooms", tid)),
    )
}

fn invalid_room(
    query: crate::ui::response::ModalQuery,
    tid: &str,
    tournament: &Tournament,
    user: User<true>,
    form: &CreateRoomForm,
    error: &str,
) -> StandardResponse {
    crate::tournaments::manage::workspace::invalid_form(
        query,
        tournament,
        user,
        &format!("/tournaments/{tid}/rooms/create"),
        &format!("/tournaments/{tid}/rooms"),
        form,
        &[],
        error,
    )
}
