use druid::{Env, Event, EventCtx, Widget, widget::Controller};

pub struct EventLogger;

impl<T, W: Widget<T>> Controller<T, W> for EventLogger {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        // Always pass on the event!
        child.event(ctx, event, data, env)
    }
}
