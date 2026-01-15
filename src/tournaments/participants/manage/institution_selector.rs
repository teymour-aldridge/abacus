use hypertext::prelude::*;

use crate::tournaments::participants::Institution;

pub struct InstitutionSelector<'r> {
    institutions: &'r [Institution],
    selected: Option<&'r str>,
    name: Option<&'static str>,
}

impl<'r> InstitutionSelector<'r> {
    pub fn new(
        institutions: &'r [Institution],
        selected: Option<&'r str>,
        name: Option<&'static str>,
    ) -> Self {
        Self {
            institutions,
            selected,
            name,
        }
    }
}

impl<'r> Renderable for InstitutionSelector<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="mb-3" {
              label for="institution" { "Institution" }
              select name=(self.name) id="institution" class="form-select" {
                  option value = "-----"
                      selected=(self.selected.is_none())
                  {
                      "No institution"
                  }
                  @for institution in self.institutions {
                      option
                          value = (institution.id)
                          selected= (
                              Some(institution.id.as_str())
                                  == self.selected
                          )
                      {
                          (institution.name)
                      }
                  }
              }
            }
        }
        .render_to(buffer);
    }
}
