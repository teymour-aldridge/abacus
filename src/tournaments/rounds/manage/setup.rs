use crate::{
    schema::{judge_availability, judges, team_availability, teams},
    tournaments::rounds::Round,
};
use diesel::prelude::*;
use hypertext::prelude::*;

pub(crate) struct SetupSummary {
    rows: Vec<(String, i64, i64)>,
    teams: i64,
    judges: i64,
    tid: String,
    seq: i64,
}
impl SetupSummary {
    pub(crate) fn load(
        tid: &str,
        seq: i64,
        rounds: &[Round],
        conn: &mut diesel::SqliteConnection,
    ) -> Result<Self, diesel::result::Error> {
        let teams = teams::table
            .filter(teams::tournament_id.eq(tid))
            .count()
            .get_result(conn)?;
        let judges = judges::table
            .filter(judges::tournament_id.eq(tid))
            .count()
            .get_result(conn)?;
        let mut rows = Vec::new();
        for round in rounds {
            let available_teams = team_availability::table
                .filter(team_availability::tournament_id.eq(tid))
                .filter(team_availability::round_id.eq(&round.id))
                .filter(team_availability::available.eq(true))
                .count()
                .get_result(conn)?;
            let available_judges = judge_availability::table
                .filter(judge_availability::tournament_id.eq(tid))
                .filter(judge_availability::round_id.eq(&round.id))
                .filter(judge_availability::available.eq(true))
                .count()
                .get_result(conn)?;
            rows.push((round.name.clone(), available_teams, available_judges));
        }
        Ok(Self {
            rows,
            teams,
            judges,
            tid: tid.into(),
            seq,
        })
    }
}
impl Renderable for SetupSummary {
    fn render_to(&self, out: &mut hypertext::Buffer<hypertext::context::Node>) {
        maud! {
            section class="my-4" {
                h2 class="h4" { "Round setup" }
                span class="badge text-bg-secondary" { "Restricted: Tournament organisers" }
                table class="table" {
                    thead { tr { th { "Round" } th { "Teams available" } th { "Judges available" } } }
                    tbody { @for (name,teams,judges) in &self.rows { tr { th scope="row" { (name) } td { (teams) "/" (self.teams) } td { (judges) "/" (self.judges) } } } }
                }
                div class="d-flex gap-2" {
                    a class="btn btn-outline-primary" href=(format!("/tournaments/{}/rounds/{}/availability/teams",self.tid,self.seq)) { "Team availability" }
                    a class="btn btn-outline-primary" href=(format!("/tournaments/{}/rounds/{}/availability/judges",self.tid,self.seq)) { "Judge availability" }
                }
            }
        }.render_to(out)
    }
}
