use super::{View, html, protected_or_empty};
use hypertext::prelude::*;

#[derive(Clone, Copy)]
pub enum Publication {
    Public,
    Unpublished,
}
pub fn publication<C: ?Sized>(
    label: &str,
    state: Publication,
) -> impl View<C> + '_ {
    html(move |_: &C, out| {
        maud! {
        span class=(match state { Publication::Public => "badge text-bg-success", Publication::Unpublished => "badge text-bg-danger" }) {
            (label) ": "
            @match state { Publication::Public => { "Public" } Publication::Unpublished => { "Unpublished" } }
        }
    }.render_to(out)
    })
}
pub fn restricted<C: ?Sized>(audience: &str) -> impl View<C> + '_ {
    html(move |_: &C, out| {
        maud! { span class="badge text-bg-secondary" { "Restricted: " (audience) } }.render_to(out)
    })
}
pub fn action<'a, C: ?Sized + 'a, P: Fn(&C) -> bool + 'a>(
    predicate: P,
    href: &'a str,
    label: &'a str,
    modal: bool,
) -> impl View<C> + 'a {
    protected_or_empty(predicate, move || {
        html(move |_: &C, out| {
            maud! {
        @if modal {
            a class="btn btn-outline-primary" href=(href)
                hx-get=(format!("{href}{}modal=true", if href.contains('?') { "&" } else { "?" }))
                hx-target="#ui-modal" hx-swap="innerHTML" { (label) }
        } @else {
            a class="btn btn-outline-primary" href=(href) { (label) }
        }
    }.render_to(out)
        })
    })
}

pub fn modal<'a>(
    title: &'a str,
    return_url: &'a str,
    body: impl Renderable + 'a,
) -> impl Renderable + 'a {
    maud! {
        dialog class="ui-dialog rounded border-0 shadow p-4" style="width:95vw;max-width:1600px;max-height:90vh;overflow:auto" aria-label=(title) {
            div class="d-flex justify-content-between align-items-center mb-3" {
                h2 class="h4 mb-0" { (title) }
                a class="btn btn-outline-secondary" href=(return_url) { "Close and refresh" }
            }
            (body)
        }
    }
}
