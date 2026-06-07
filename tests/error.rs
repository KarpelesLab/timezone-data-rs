//! Tests for the public `Error` type.

use timezone_data::Error;

#[test]
fn implements_std_error() {
    fn assert_std_error<E: std::error::Error>(_: &E) {}
    assert_std_error(&Error::NotFound);
}

#[test]
fn display_messages() {
    assert!(format!("{}", Error::NotFound).contains("not found"));
    assert!(format!("{}", Error::BadPosixTz("bad")).contains("POSIX"));
    assert!(format!("{}", Error::BadPosixTz("bad")).contains("bad"));
}
