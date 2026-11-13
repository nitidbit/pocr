//! Public arithmetic helper.

use api_grpc::add;

#[test]
fn add_sums_both_operands() {
    assert_eq!(add(2, 2), 4);
}
