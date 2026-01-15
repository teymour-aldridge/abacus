use hypertext::prelude::*;

use super::institution_selector::InstitutionSelector;

pub struct TeamForm<'a> {
    team_name: Option<&'a str>,
    institution_selector: &'a InstitutionSelector<'a>,
}

impl<'a> TeamForm<'a> {
    pub fn new(institution_selector: &'a InstitutionSelector) -> Self {
        Self {
            team_name: None,
            institution_selector,
        }
    }

    pub fn with_team_name(mut self, team_name: &'a str) -> Self {
        self.team_name = Some(team_name);
        self
    }
}

impl<'a> Renderable for TeamForm<'a> {
    fn render_to(&self, buffer: &mut hypertext::Buffer) {
        maud! {
            div class="mb-3" {
                label for="teamName" class="form-label" { "Team name" }
                input
                    type="text"
                    class="form-control"
                    id="teamName"
                    aria-describedby="teamNameHelp"
                    name="name"
                    value=(self.team_name.unwrap_or(""));
                div id="teamNameHelp" class="form-text" {
                    "The team name. Please note that this will be prefixed with"
                    " the institution name (if an institution is selected)."
                }
            }
            (self.institution_selector)
        }
        .render_to(buffer);
    }
}
