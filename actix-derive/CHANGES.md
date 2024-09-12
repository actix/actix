# Changes

## Unreleased

## 0.6.2

- Fix compile failures when using quoted rtype (i.e., `#[rtype("()")]`) form.

## 0.6.1

- Update `syn` dependency to `2`.

## 0.6.0

- No significant changes from v0.6.0-beta.2.

## 0.6.0-beta.2

- Bump actix dependency to v0.11.0-beta.3.

## 0.6.0-beta.1

- Add `#[actix::main]` and `#[actix::test]` macros.

## 0.5.0

- Update syn & quote to 1.0

## 0.4.0

- Added `MessageResponse` proc derive macro

## 0.3.2

- Fix another warning in rustc 1.29 or later [#12]

[#12]: https://github.com/actix/actix-derive/pull/12

## 0.3.1

- Upgrade `syn` crate to 0.15

## 0.3.0

- Fix warning in rustc 1.29.0-nightly [#9]
- Remove nightly support

[#9]: https://github.com/actix/actix-derive/pull/9

## 0.2.0

- Actix 0.5 support

## 0.1.1

- Move tests to actix

## 0.1.0

- Added `msg` proc macro
- Added `actor` proc macro
