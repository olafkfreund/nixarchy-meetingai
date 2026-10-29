/// A keyed chat credential; no keyed provider is built in.
fn keyed(key: &str) -> Credential {
    let mut credential = Credential::chat("custom-test", key);
    credential.custom = Some(Default::default());
    credential.model = "m".into();
    credential.base_url = "http://127.0.0.1:1/v1".into();
    credential
}
