// TODO: at some point it may make sense to separate these out. For the mean
// time, however, we send all data through a single channel.

use serde::{Deserialize, Serialize};

use crate::tournaments::Tournament;

#[derive(Clone, Debug)]
/// A message which is sent following a modification made during a tournament.
/// This is then used by individual pages to update the
pub struct Msg {
    pub tournament: Tournament,
    pub inner: MsgContents,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum MsgContents {
    ParticipantsUpdate,
    AvailabilityUpdate,
    DrawUpdated(String),
}
