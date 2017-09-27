use actor::Actor;
use context::Context;


pub trait ActorFactory<A: Actor> {

    fn create(&mut self, ctx: &mut Context<A>) -> A;
}


impl<A> ActorFactory<A> for FnMut(&mut Context<A>) -> A
    where A: Actor
{
    fn create(&mut self, ctx: &mut Context<A>) -> A {
        self(ctx)
    }
}


impl<A> ActorFactory<A> for A where A: Actor + Default
{
    fn create(&mut self, _: &mut Context<A>) -> A {
        <A as Default>::default()
    }
}
