use pretty_assertions::assert_eq;
use test_utils::read_json_file;

use super::compress_and_encode;

#[test]
fn compress_and_encode_hardcoded_value() {
    let sierra_program = read_json_file("sierra_program.json");
    let expected_value = read_json_file("sierra_program_base64.json").as_str().unwrap().to_owned();
    let value = compress_and_encode(sierra_program).unwrap();
    assert_eq!(value, expected_value);
}
