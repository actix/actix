extern crate skeptic;
use std::{env, fs};


#[cfg(unix)]
fn main() {
    if env::var("USE_SKEPTIC").is_ok() {
        // generates doc tests for `README.md`.
        skeptic::generate_doc_tests(
            &["README.md",
              "guide/src/qs_01.md",
              "guide/src/qs_02.md",
              "guide/src/qs_03.md",
              "guide/src/qs_04.md",
              "guide/src/qs_05.md",
              "guide/src/qs_06.md",
              "guide/src/qs_07.md",
              "guide/src/qs_08.md",
              "guide/src/qs_09.md",
              "guide/src/qs_10.md",
              "guide/src/qs_11.md",
              "guide/src/qs_12.md",
            ]);
    } else {
        let f = env::var("OUT_DIR").unwrap() + "/skeptic-tests.rs";
        let _ = fs::File::create(f);
    }
}

#[cfg(not(unix))]
fn main() {
}
