pub(crate) fn encode(value: &impl serde::Serialize) -> anyhow::Result<Vec<u8>> {
    Ok(toml::to_string(value)?.into_bytes())
}

pub(crate) fn encode_value<T: serde::Serialize + ?Sized>(value: &T) -> anyhow::Result<Vec<u8>> {
    #[derive(serde::Serialize)]
    struct Document<'a, T: ?Sized> {
        value: &'a T,
    }

    encode(&Document { value })
}
