pub mod actions;
pub mod alert;
pub mod non_public;
pub mod presentation;
pub mod response;

use hypertext::{Buffer, Renderable, context::Node};

pub type HtmlBuffer = Buffer<Node>;

pub trait View<C: ?Sized> {
    fn render_to(&self, context: &C, output: &mut HtmlBuffer);

    fn bind<'a>(&'a self, context: &'a C) -> Bound<'a, Self, C> {
        Bound {
            view: self,
            context,
        }
    }
}

pub struct Bound<'a, V: ?Sized, C: ?Sized> {
    view: &'a V,
    context: &'a C,
}
impl<V: View<C> + ?Sized, C: ?Sized> Renderable for Bound<'_, V, C> {
    fn render_to(&self, output: &mut HtmlBuffer) {
        self.view.render_to(self.context, output);
    }
}

pub struct Html<F>(F);
pub fn html<C: ?Sized, F: Fn(&C, &mut HtmlBuffer)>(render: F) -> Html<F> {
    Html(render)
}
impl<C: ?Sized, F: Fn(&C, &mut HtmlBuffer)> View<C> for Html<F> {
    fn render_to(&self, context: &C, output: &mut HtmlBuffer) {
        (self.0)(context, output);
    }
}

pub struct Sequence<T>(T);
pub fn sequence<T>(children: T) -> Sequence<T> {
    Sequence(children)
}
macro_rules! tuple_sequence {
    ($($t:ident : $i:tt),+) => {
        impl<C: ?Sized, $($t: View<C>),+> View<C> for Sequence<($($t,)+)> {
            fn render_to(&self, context: &C, output: &mut HtmlBuffer) {
                $(self.0.$i.render_to(context, output);)+
            }
        }
    };
}
tuple_sequence!(A:0);
tuple_sequence!(A:0, B:1);
tuple_sequence!(A:0, B:1, D:2);
tuple_sequence!(A:0, B:1, D:2, E:3);
tuple_sequence!(A:0, B:1, D:2, E:3, F:4);
tuple_sequence!(A:0, B:1, D:2, E:3, F:4, G:5);

pub struct Protected<P, B, F> {
    predicate: P,
    body: B,
    fallback: F,
}
pub fn protected<C: ?Sized, P, B, F, BV, FV>(
    predicate: P,
    body: B,
    fallback: F,
) -> Protected<P, B, F>
where
    P: Fn(&C) -> bool,
    B: Fn() -> BV,
    F: Fn() -> FV,
    BV: View<C>,
    FV: View<C>,
{
    Protected {
        predicate,
        body,
        fallback,
    }
}
impl<C: ?Sized, P, B, F, BV, FV> View<C> for Protected<P, B, F>
where
    P: Fn(&C) -> bool,
    B: Fn() -> BV,
    F: Fn() -> FV,
    BV: View<C>,
    FV: View<C>,
{
    fn render_to(&self, context: &C, output: &mut HtmlBuffer) {
        if (self.predicate)(context) {
            (self.body)().render_to(context, output);
        } else {
            (self.fallback)().render_to(context, output);
        }
    }
}
#[derive(Clone, Copy)]
pub struct Empty;
impl<C: ?Sized> View<C> for Empty {
    fn render_to(&self, _: &C, _: &mut HtmlBuffer) {}
}
pub fn protected_or_empty<C: ?Sized, P, B, BV>(
    predicate: P,
    body: B,
) -> impl View<C>
where
    P: Fn(&C) -> bool,
    B: Fn() -> BV,
    BV: View<C>,
{
    protected(predicate, body, || Empty)
}

pub struct WithContext<P, V> {
    project: P,
    body: V,
}
pub fn with_context<C: ?Sized, D, P: Fn(&C) -> D, V: View<D>>(
    project: P,
    body: V,
) -> WithContext<P, V> {
    WithContext { project, body }
}
impl<C: ?Sized, D, P: Fn(&C) -> D, V: View<D>> View<C> for WithContext<P, V> {
    fn render_to(&self, context: &C, output: &mut HtmlBuffer) {
        self.body.render_to(&(self.project)(context), output);
    }
}

pub struct ForEach<'a, T, F> {
    items: &'a [T],
    render_item: F,
}
pub fn for_each<'a, C: ?Sized, T, F, V>(
    items: &'a [T],
    render_item: F,
) -> ForEach<'a, T, F>
where
    F: Fn(&'a T) -> V,
    V: View<C>,
{
    ForEach { items, render_item }
}
impl<'a, C: ?Sized, T, F, V> View<C> for ForEach<'a, T, F>
where
    F: Fn(&'a T) -> V,
    V: View<C>,
{
    fn render_to(&self, context: &C, output: &mut HtmlBuffer) {
        for item in self.items {
            (self.render_item)(item).render_to(context, output);
        }
    }
}
