/// Proc macro compile tests.
///
/// Only run on MSRV to ensure stable compile errors.
#[rustversion_msrv::msrv]
#[test]
fn compile_macros() {
    let t = trybuild::TestCases::new();

    t.pass("tests/trybuild/rt-main.rs");
    t.pass("tests/trybuild/message-response.rs");
    t.pass("tests/trybuild/message.rs");
}
