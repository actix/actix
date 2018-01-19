# CHANGES
=======

## Master

* Allow returning Box<ActorFuture> in ActorFutures for non tail recursive futures

## 0.4.4 (2018-01-19)

* Add `Clone` implementation for `Box<Subscriber<M> + Send>`

* Stop stream polling if context is wating for future completion

* Upgraded address stops working after all references are dropped #38


## 0.4.3 (2018-01-09)

* Cleanup `FramedActor` error and close state handling.

* Do not exit early from framed polling


## 0.4.2 (2018-01-07)

* Cleanup actor stopping process

* Unify context implementation


## 0.4.1 (2018-01-06)

* Remove StreamHandler requirements from add_message_stream()

* Fix items length check


## 0.4.0 (2018-01-05)

* Simplify `Handler` trait (E type removed).

* Use assosiated type for handler response for `Handler` trait.

* Added framed `drain` method.

* Allow to replace framed object in framed context.

* Enable signal actor by default, make it compatible with windows.

* Added `SyncContext::restart()` method, which allow to restart sync actor.

* Changed behaviour of `Address::call`, if request get drop message cancels.


## 0.3.5 (2017-12-23)

* Re-export `actix_derive` package

* Added conversion implementation `From<Result<I, E>> for Response<A, M>`

* Expose the Framed underneath FramedContext #29


## 0.3.4 (2017-12-20)

* Fix memory leak when sending messages recursively to self #28

* Add convenience impl for boxed Subscriber objects. #27

* Add `ActorStream::fold()` method.

* Add helper combinator `Stream::finish()` method.


## 0.3.3 (2017-11-21)

* SystemRegistry does not store created actor #21

## 0.3.2 (2017-11-06)

* Disable `signal` feature by default


## 0.3.1 (2017-10-30)

* Simplify `ToEnvelope` trait, do not generalize over Message type.

* `ActorContext` requires `ToEnvelope` trait.

* Added `Subscriber::subscriber() -> Box<Subscriber>`

* Simplify `ActorContext` trait, it does not need to know about `Actor`

* Cancel `notify` and `run_later` futures on context stop


## 0.3.0 (2017-10-23)

* Added `Either` future

* Message has to provide `ResponseType` impl instead of Actor


## 0.2.0 (2017-10-17)

* Added `ActorStream`


## 0.1.0 (2017-10-11)

* First release
