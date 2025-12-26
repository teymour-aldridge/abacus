use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use serde::{Deserialize, Serialize};

use crate::schema::{
    judge_room_constraints, rooms_of_room_categories, speaker_room_constraints,
    tournament_room_categories, tournament_rooms,
};

pub mod manage;

#[derive(
    Queryable, Selectable, Insertable, Serialize, Deserialize, Debug, Clone,
)]
#[diesel(table_name = tournament_rooms)]
#[diesel(check_for_backend(Sqlite))]
pub struct Room {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub url: Option<String>,
    pub priority: i64,
    pub number: i64,
}

#[derive(
    Queryable, Selectable, Insertable, Serialize, Deserialize, Debug, Clone,
)]
#[diesel(table_name = tournament_room_categories)]
#[diesel(check_for_backend(Sqlite))]
pub struct RoomCategory {
    pub id: String,
    pub tournament_id: String,
    pub private_name: String,
    pub public_name: String,
    pub description: String,
}

#[derive(
    Queryable, Selectable, Insertable, Serialize, Deserialize, Debug, Clone,
)]
#[diesel(table_name = rooms_of_room_categories)]
#[diesel(check_for_backend(Sqlite))]
pub struct RoomsOfRoomCategory {
    pub id: String,
    pub category_id: String,
    pub room_id: String,
}

#[derive(
    Queryable, Selectable, Insertable, Serialize, Deserialize, Debug, Clone,
)]
#[diesel(table_name = speaker_room_constraints)]
#[diesel(check_for_backend(Sqlite))]
pub struct SpeakerRoomConstraint {
    pub id: String,
    pub speaker_id: String,
    pub category_id: String,
    pub pref: i64,
}

#[derive(
    Queryable, Selectable, Insertable, Serialize, Deserialize, Debug, Clone,
)]
#[diesel(table_name = judge_room_constraints)]
#[diesel(check_for_backend(Sqlite))]
pub struct JudgeRoomConstraint {
    pub id: String,
    pub judge_id: String,
    pub category_id: String,
    pub pref: i64,
}
