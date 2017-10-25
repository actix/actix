# CHANGES


## 0.3.1 (2017-10-xx)

* Simplify `ToEnvelope` trait, do not generalize over Message type.

* `ActorContext` requires `ToEnvelope` trait.

* Added `Subscriber::subscriber() -> Box<Subscriber>`

## 0.3.0 (2017-10-23)

* Added `Either` future

* Message has to provide `ResponseType` impl instead of Actor


## 0.2.0 (2017-10-17)

* Added `ActorStream`


## 0.1.0 (2017-10-11)

* First release
