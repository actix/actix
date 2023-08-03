/// Proc macro compile tests.
///
/// Only run on MSRV to ensure stable compile errors.
#[rustversion::stable(1.68)]
#[test]
fn compile_macros() {
    let t = trybuild::TestCases::new();

    t.pass("tests/trybuild/rt-main.rs");

    t.pass("tests/trybuild/message-response.rs");

    t.pass("tests/trybuild/message.rs");
    t.compile_fail("tests/trybuild/message-fail-bare-rtype.rs");
}
