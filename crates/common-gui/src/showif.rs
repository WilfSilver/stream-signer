//! A widget which only shows its child if the given widget if the given
//! function returns true

use druid::{Data, Env, Lens, LensExt, Widget, WidgetPod, widget::WidgetWrapper};

type ShowIfFunc<T> = dyn Fn(&T, &Env) -> bool;

pub struct ShowIf<T, W> {
    child: WidgetPod<T, W>,
    show_if: Box<ShowIfFunc<T>>,
}

impl<T: Data, W: Widget<T>> ShowIf<T, W> {
    pub fn new(child: W, show_if: impl Fn(&T, &Env) -> bool + 'static) -> Self {
        Self {
            child: WidgetPod::new(child),
            show_if: Box::new(show_if),
        }
    }
}

impl<T, W> WidgetWrapper for ShowIf<T, W> {
    type Wrapped = WidgetPod<T, W>;

    fn wrapped(&self) -> &Self::Wrapped {
        &self.child
    }

    fn wrapped_mut(&mut self) -> &mut Self::Wrapped {
        &mut self.child
    }
}

impl<T: Data, W: Widget<T>> Widget<T> for ShowIf<T, W> {
    fn event(&mut self, ctx: &mut druid::EventCtx, event: &druid::Event, data: &mut T, env: &Env) {
        self.child.event(ctx, event, data, env);
    }

    fn lifecycle(
        &mut self,
        ctx: &mut druid::LifeCycleCtx,
        event: &druid::LifeCycle,
        data: &T,
        env: &Env,
    ) {
        self.child.lifecycle(ctx, event, data, env);
    }

    fn update(&mut self, ctx: &mut druid::UpdateCtx, _old_data: &T, data: &T, env: &Env) {
        self.child.update(ctx, data, env);
    }

    fn layout(
        &mut self,
        ctx: &mut druid::LayoutCtx,
        bc: &druid::BoxConstraints,
        data: &T,
        env: &Env,
    ) -> druid::Size {
        self.child.layout(ctx, bc, data, env)
    }

    fn paint(&mut self, ctx: &mut druid::PaintCtx, data: &T, env: &Env) {
        if (self.show_if)(data, env) {
            self.child.paint(ctx, data, env);
        }
    }
}

pub trait ShowIfExt<T: Data>: Sized {
    fn show_if_state<L: Lens<T, bool> + 'static>(self, lens: L) -> ShowIf<T, Self>;
    fn show_if(self, f: impl Fn(&T) -> bool + 'static) -> ShowIf<T, Self>;
}

impl<T: Data, W: Widget<T> + Sized> ShowIfExt<T> for W {
    fn show_if_state<L: Lens<T, bool> + 'static>(self, lens: L) -> ShowIf<T, Self> {
        ShowIf::new(self, move |data, _env| lens.get(data))
    }

    fn show_if(self, f: impl Fn(&T) -> bool + 'static) -> ShowIf<T, Self> {
        ShowIf::new(self, move |data, _env| f(data))
    }
}
