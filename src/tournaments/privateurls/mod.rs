use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    schema::{tournament_judges, tournament_participants, tournament_speakers},
    tournaments::participants::{Judge, Speaker},
    util_resp::{FailureResponse, err_not_found},
};

#[derive(Queryable, QueryableByName)]
#[diesel(table_name = tournament_participants)]
pub struct ParticipantUrl {
    pub id: String,
    pub tournament_id: String,
    pub private_url: String,
}

pub struct Participant {
    pub url: ParticipantUrl,
    pub kind: ParticipantKind,
}

impl Participant {
    /// Retrieves the participant with the given private URL.
    pub fn fetch(
        private_url: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, FailureResponse> {
        let private_url = tournament_participants::table
            .filter(tournament_participants::private_url.eq(private_url))
            .first::<ParticipantUrl>(conn)
            .optional()
            .unwrap();

        let ret = private_url
            .map(|url| {
                let speaker = tournament_speakers::table
                    .filter(tournament_speakers::participant_id.eq(&url.id))
                    .first::<Speaker>(conn)
                    .optional()
                    .unwrap();

                match speaker {
                    Some(s) => Some((url, ParticipantKind::Speaker(s))),
                    None => tournament_judges::table
                        .filter(tournament_judges::participant_id.eq(&url.id))
                        .first::<Judge>(conn)
                        .optional()
                        .unwrap()
                        .map(ParticipantKind::Judge)
                        .map(|t| (url, t)),
                }
            })
            .flatten()
            .map(|(url, kind)| Participant { url, kind });

        match ret {
            Some(t) => Ok(t),
            None => err_not_found()
                .map(|_| unreachable!("err_not_found always returns `Err`")),
        }
    }
}

pub enum ParticipantKind {
    Speaker(Speaker),
    Judge(Judge),
}
