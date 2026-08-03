use super::AssetStoreManager;

#[test]
fn manager_is_send_and_sync() {
    fn assert_send_and_sync<T: Send + Sync>() {}

    assert_send_and_sync::<AssetStoreManager>();
}
