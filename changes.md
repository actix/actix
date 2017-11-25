# CHANGES
=======

## Master

* Allow returning Box<ActorFuture> in ActorFutures for non tail recursive futures

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
