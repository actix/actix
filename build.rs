extern crate skeptic;

#[cfg(unix)]
fn main() {
    // generates doc tests for `README.md`.
    skeptic::generate_doc_tests(&["README.md"]);
}

#[cfg(not(unix))]
fn main() {
}
