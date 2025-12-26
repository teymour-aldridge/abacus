use crate::{
    auth::User,
    schema::{
        rooms_of_room_categories, tournament_room_categories, tournament_rooms,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rooms::{Room, RoomCategory},
        rounds::TournamentRounds,
    },
    util_resp::{FailureResponse, StandardResponse, see_other_ok, success},
};
use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CreateRoomForm {
    name: String,
    priority: i64,
}

#[derive(Deserialize)]
pub struct CreateCategoryForm {
    private_name: String,
    public_name: String,
    description: String,
}

#[derive(Deserialize)]
pub struct AddRoomToCategoryForm {
    room_id: String,
}

use std::collections::HashMap;

pub async fn manage_rooms_page(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    let rooms = tournament_rooms::table
        .filter(tournament_rooms::tournament_id.eq(&tid))
        .order_by(tournament_rooms::priority.asc())
        .load::<Room>(&mut *conn)
        .unwrap();

    let categories = tournament_room_categories::table
        .filter(tournament_room_categories::tournament_id.eq(&tid))
        .load::<RoomCategory>(&mut *conn)
        .unwrap();

    use crate::tournaments::rooms::RoomsOfRoomCategory;
    let category_ids: Vec<String> =
        categories.iter().map(|c| c.id.clone()).collect();

    let links = rooms_of_room_categories::table
        .filter(rooms_of_room_categories::category_id.eq_any(&category_ids))
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

    success(
        Page::new()
            .active_nav("rooms")
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(current_rounds.first().map(|r| r.seq)) active_page=(Some("rooms")) {
                    div class="d-flex flex-column gap-5" {
                        // Rooms Section
                        div {
                            div class="d-flex justify-content-between align-items-center mb-3" {
                                h2 class="h4 fw-bold mb-0" { "Rooms" }
                            }

                            div class="card mb-4" {
                                div class="card-body bg-light" {
                                    form action=(format!("/tournaments/{}/rooms/create", tournament.id)) method="post" class="row g-3 align-items-end" {
                                        div class="col-md-6" {
                                            label for="name" class="form-label" { "Room Name" }
                                            input type="text" class="form-control" name="name" required;
                                        }
                                        div class="col-md-4" {
                                            label for="priority" class="form-label" { "Priority (Lower = Higher)" }
                                            input type="number" class="form-control" name="priority" value="10" required;
                                        }
                                        div class="col-md-2" {
                                            button type="submit" class="btn btn-primary w-100" { "Add Room" }
                                        }
                                    }
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
                                                    form action=(format!("/tournaments/{}/rooms/{}/delete", tournament.id, room.id)) method="post"
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
                                    form action=(format!("/tournaments/{}/rooms/categories/create", tournament.id)) method="post" {
                                        div class="row g-3" {
                                            div class="col-md-4" {
                                                label for="private_name" class="form-label" { "Internal Name" }
                                                input type="text" class="form-control" name="private_name" placeholder="e.g. Access" required;
                                            }
                                            div class="col-md-4" {
                                                label for="public_name" class="form-label" { "Public Name" }
                                                input type="text" class="form-control" name="public_name" placeholder="e.g. Accessibility Room" required;
                                            }
                                            div class="col-md-12" {
                                                label for="description" class="form-label" { "Description" }
                                                input type="text" class="form-control" name="description" placeholder="Short description of the category";
                                            }
                                            div class="col-12 text-end" {
                                                button type="submit" class="btn btn-primary" { "Create Category" }
                                            }
                                        }
                                    }
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
                                            form action=(format!("/tournaments/{}/rooms/categories/{}/delete", tournament.id, category.id)) method="post"
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
                                                            form action=(format!("/tournaments/{}/rooms/categories/{}/remove_room", tournament.id, category.id)) method="post" class="m-0 p-0 d-flex" {
                                                                input type="hidden" name="room_id" value=(room.id);
                                                                button type="submit" class="btn-close" style="width: 0.5em; height: 0.5em;" aria-label="Remove" {}
                                                            }
                                                        }
                                                    }
                                                    @if rooms_in_cat.is_empty() {
                                                        span class="text-muted small fst-italic" { "No rooms assigned" }
                                                    }
                                                }

                                                form action=(format!("/tournaments/{}/rooms/categories/{}/add_room", tournament.id, category.id)) method="post" class="d-flex gap-2" {
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
            })
            .render(),
    )
}

pub async fn create_room(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateRoomForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;

    // Check permission
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let new_room = Room {
        id: uuid::Uuid::now_v7().to_string(),
        tournament_id: tid.clone(),
        name: form.name,
        url: None,
        priority: form.priority,
        number: tournament_rooms::table
            .filter(tournament_rooms::tournament_id.eq(&tournament.id))
            .order_by(tournament_rooms::number.desc())
            .select(tournament_rooms::number)
            .first(&mut *conn)
            .optional()
            .unwrap()
            .map(|x: i64| x + 1)
            .unwrap_or(0),
    };

    diesel::insert_into(tournament_rooms::table)
        .values(&new_room)
        .execute(&mut *conn)
        .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}

pub async fn delete_room(
    Path((tid, room_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    diesel::delete(
        tournament_rooms::table.filter(tournament_rooms::id.eq(room_id)),
    )
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}

pub async fn create_category(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let new_cat = RoomCategory {
        id: uuid::Uuid::new_v4().to_string(),
        tournament_id: tid.clone(),
        private_name: form.private_name,
        public_name: form.public_name,
        description: form.description,
    };

    diesel::insert_into(tournament_room_categories::table)
        .values(&new_cat)
        .execute(&mut *conn)
        .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}

pub async fn delete_category(
    Path((tid, cat_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    diesel::delete(
        tournament_room_categories::table
            .filter(tournament_room_categories::id.eq(cat_id)),
    )
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}

pub async fn add_room_to_category(
    Path((tid, cat_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddRoomToCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    use crate::tournaments::rooms::RoomsOfRoomCategory;

    // Check if relation already exists to avoid unique constraint error
    let exists = rooms_of_room_categories::table
        .filter(rooms_of_room_categories::category_id.eq(&cat_id))
        .filter(rooms_of_room_categories::room_id.eq(&form.room_id))
        .first::<RoomsOfRoomCategory>(&mut *conn)
        .optional()
        .map_err(FailureResponse::from)?;

    if exists.is_none() {
        let new_rel = RoomsOfRoomCategory {
            id: uuid::Uuid::new_v4().to_string(),
            category_id: cat_id,
            room_id: form.room_id,
        };

        diesel::insert_into(rooms_of_room_categories::table)
            .values(&new_rel)
            .execute(&mut *conn)
            .map_err(FailureResponse::from)?;
    }

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}

pub async fn remove_room_from_category(
    Path((tid, cat_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddRoomToCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    diesel::delete(
        rooms_of_room_categories::table
            .filter(rooms_of_room_categories::category_id.eq(&cat_id))
            .filter(rooms_of_room_categories::room_id.eq(&form.room_id)),
    )
    .execute(&mut *conn)
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}/rooms", tid)))
}
