use druid::{widget::Controller, Env, Event, EventCtx, Widget};

pub struct EventLogger;

impl<T, W: Widget<T>> Controller<T, W> for EventLogger {
    fn event(&mut self, child: &mut W, ctx: &mut EventCtx, event: &Event, data: &mut T, env: &Env) {
        println!("Controller: {:?}", event);
        // Always pass on the event!
        child.event(ctx, event, data, env)
    }
}
